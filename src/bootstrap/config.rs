use std::env;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use dut_http::ProxyMode;
use dut_upstream::CachePolicy;

use super::StartupError;

const DEFAULT_PORT: u16 = 3000;
const DEFAULT_HTTP_TIMEOUT: Duration = Duration::from_secs(10);

/// Overrides the listen address, e.g. `0.0.0.0:3000` in a container, where
/// the default localhost address cannot be reached from outside.
const BIND_ADDRESS_VAR: &str = "DUT_BIND_ADDRESS";

const NEXT_TRAIN_ENDPOINT: &str = "https://rt.data.gov.hk/v1/transport/mtr/getSchedule.php";
const LINE_STATUS_ENDPOINT: &str = "https://tnews.mtr.com.hk/alert/ryg_line_status.json";

/// The Hong Kong Observatory's current weather report. No endpoint serves
/// weather yet; the startup connectivity check probes it alongside the MTR
/// feeds, with the client-wide timeout.
const WEATHER_ENDPOINT: &str =
    "https://data.weather.gov.hk/weatherAPI/opendata/weather.php?dataType=rhrread&lang=en";

/// Short enough that a slow upstream falls back to stale data quickly.
const MTR_REQUEST_TIMEOUT: Duration = Duration::from_secs(3);

/// Next Train data is served fresh or not at all under normal conditions:
/// expired boards are refreshed before answering (no stale-while-revalidate),
/// and freshness follows the upstream CDN's remaining `max-age`.
const NEXT_TRAIN_CACHE: CachePolicy = CachePolicy {
    default_ttl: Duration::from_secs(10),
    ttl_floor: Duration::from_secs(2),
    ttl_ceiling: Duration::from_secs(15),
    stale_while_revalidate: Duration::ZERO,
    stale_if_error: Duration::from_secs(90),
    failure_backoff: Duration::from_secs(5),
};

/// Line status changes rarely and the feed is one shared document, so it is
/// refreshed in the background and may stand in for longer during outages.
/// Upstream's `max-age=5` is ignored in favour of a fixed period.
const LINE_STATUS_CACHE: CachePolicy = CachePolicy {
    default_ttl: Duration::from_secs(30),
    ttl_floor: Duration::from_secs(30),
    ttl_ceiling: Duration::from_secs(30),
    stale_while_revalidate: Duration::from_secs(60),
    stale_if_error: Duration::from_secs(15 * 60),
    failure_backoff: Duration::from_secs(10),
};

/// Settings for one server process. The default listens on localhost and
/// reads the live MTR feeds; tests swap the endpoints for a fake upstream.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppConfig {
    bind_address: SocketAddr,
    outbound_http_timeout: Duration,
    outbound_proxy: ProxyMode,
    mtr: MtrConfig,
    weather_endpoint: String,
}

/// Where and how to read the MTR's real-time feeds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MtrConfig {
    pub next_train_endpoint: String,
    pub line_status_endpoint: String,
    pub request_timeout: Duration,
    pub next_train_cache: CachePolicy,
    pub line_status_cache: CachePolicy,
}

impl AppConfig {
    /// The defaults, adjusted by the process environment.
    ///
    /// Only the listen address can be set this way: it is the one setting
    /// that depends on where the process runs rather than on what it serves.
    pub fn from_env() -> Result<Self, StartupError> {
        let config = Self::default();
        match env::var_os(BIND_ADDRESS_VAR) {
            Some(value) => config.with_bind_address(&value.to_string_lossy()),
            None => Ok(config),
        }
    }

    const fn new(
        bind_address: SocketAddr,
        outbound_http_timeout: Duration,
        outbound_proxy: ProxyMode,
        mtr: MtrConfig,
        weather_endpoint: String,
    ) -> Self {
        Self {
            bind_address,
            outbound_http_timeout,
            outbound_proxy,
            mtr,
            weather_endpoint,
        }
    }

    /// Points the MTR adapters at other endpoints, e.g. a test double.
    #[must_use]
    pub fn with_mtr_endpoints(
        mut self,
        next_train: impl Into<String>,
        line_status: impl Into<String>,
    ) -> Self {
        self.mtr.next_train_endpoint = next_train.into();
        self.mtr.line_status_endpoint = line_status.into();
        self
    }

    /// Connects to upstreams directly, ignoring any proxy the environment or
    /// the operating system configures. Tests use it to reach a fake upstream
    /// on this machine.
    #[must_use]
    pub const fn without_proxy(mut self) -> Self {
        self.outbound_proxy = ProxyMode::Direct;
        self
    }

    fn with_bind_address(mut self, value: &str) -> Result<Self, StartupError> {
        self.bind_address = value
            .parse()
            .map_err(|source| StartupError::InvalidBindAddress {
                variable: BIND_ADDRESS_VAR,
                value: value.to_owned(),
                source,
            })?;
        Ok(self)
    }

    pub(crate) const fn bind_address(&self) -> SocketAddr {
        self.bind_address
    }

    pub(crate) const fn outbound_http_timeout(&self) -> Duration {
        self.outbound_http_timeout
    }

    pub(crate) const fn outbound_proxy(&self) -> ProxyMode {
        self.outbound_proxy
    }

    pub(crate) const fn mtr(&self) -> &MtrConfig {
        &self.mtr
    }

    pub(crate) fn weather_endpoint(&self) -> &str {
        &self.weather_endpoint
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self::new(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), DEFAULT_PORT),
            DEFAULT_HTTP_TIMEOUT,
            ProxyMode::System,
            MtrConfig::default(),
            WEATHER_ENDPOINT.to_owned(),
        )
    }
}

impl Default for MtrConfig {
    fn default() -> Self {
        Self {
            next_train_endpoint: NEXT_TRAIN_ENDPOINT.to_owned(),
            line_status_endpoint: LINE_STATUS_ENDPOINT.to_owned(),
            request_timeout: MTR_REQUEST_TIMEOUT,
            next_train_cache: NEXT_TRAIN_CACHE,
            line_status_cache: LINE_STATUS_CACHE,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::Ipv6Addr;

    use super::*;

    #[test]
    fn listens_on_localhost_by_default() {
        let config = AppConfig::default();

        assert_eq!(
            config.bind_address(),
            SocketAddr::from((Ipv4Addr::LOCALHOST, DEFAULT_PORT))
        );
    }

    #[test]
    fn uses_the_system_proxy_unless_told_otherwise() {
        assert_eq!(AppConfig::default().outbound_proxy(), ProxyMode::System);
        assert_eq!(
            AppConfig::default().without_proxy().outbound_proxy(),
            ProxyMode::Direct
        );
    }

    #[test]
    fn accepts_ipv4_and_ipv6_bind_addresses() {
        let ipv4 = AppConfig::default().with_bind_address("0.0.0.0:3000");
        let ipv6 = AppConfig::default().with_bind_address("[::]:8080");

        assert_eq!(
            ipv4.map(|config| config.bind_address()).ok(),
            Some(SocketAddr::from((Ipv4Addr::UNSPECIFIED, 3000)))
        );
        assert_eq!(
            ipv6.map(|config| config.bind_address()).ok(),
            Some(SocketAddr::from((Ipv6Addr::UNSPECIFIED, 8080)))
        );
    }

    #[test]
    fn rejects_bind_addresses_without_an_ip_and_port() {
        for value in ["3000", "localhost:3000", "0.0.0.0", ""] {
            let result = AppConfig::default().with_bind_address(value);

            assert!(
                matches!(
                    result,
                    Err(StartupError::InvalidBindAddress { value: ref rejected, .. })
                        if rejected == value
                ),
                "{value:?} should be rejected"
            );
        }
    }
}
