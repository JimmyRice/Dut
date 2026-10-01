use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use dut_http::ProxyMode;
use dut_poll::Schedule;
use dut_upstream::CachePolicy;

/// Localhost, so a process started by hand is not reachable from the network
/// until its operator says so.
pub(super) const DEFAULT_BIND_ADDRESS: SocketAddr =
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 3000);

const DEFAULT_HTTP_TIMEOUT: Duration = Duration::from_secs(10);

const NEXT_TRAIN_ENDPOINT: &str = "https://rt.data.gov.hk/v1/transport/mtr/getSchedule.php";
const LINE_STATUS_ENDPOINT: &str = "https://tnews.mtr.com.hk/alert/ryg_line_status.json";

/// The directory on the MTR's open data portal that holds the CSV files.
const OPEN_DATA_ENDPOINT: &str = "https://opendata.mtr.com.hk/data/";

/// The Hong Kong Observatory's weather warning information. Unlike the
/// shorter warning summary, it lists the pre-No. 8 special announcement.
const WEATHER_WARNINGS_ENDPOINT: &str =
    "https://data.weather.gov.hk/weatherAPI/opendata/weather.php?dataType=warningInfo&lang=en";

/// Short enough that a slow upstream falls back to stale data quickly.
const MTR_REQUEST_TIMEOUT: Duration = Duration::from_secs(3);

/// Warnings are read in the background, so nobody waits on this; it only
/// bounds how long a hung request delays the next poll.
const HKO_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// The largest open data file is about 750 KB, and it is read in the
/// background, so a slow transfer is given time to finish.
const OPEN_DATA_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// How long a polled value may stand in, marked stale, while polls fail.
const POLL_STALE_IF_ERROR: Duration = Duration::from_secs(15 * 60);

/// Line status changes rarely, and the MTR website itself refreshes it only
/// every three minutes, so a poll every 30 seconds loses nothing. A value
/// stays fresh until the next poll has had its timeout to finish.
const LINE_STATUS_POLL: Schedule = Schedule {
    interval: Duration::from_secs(30),
    first_poll_after: Duration::ZERO,
    retry_after: Duration::from_secs(30),
    fresh_for: Duration::from_secs(30).saturating_add(MTR_REQUEST_TIMEOUT),
    stale_if_error: POLL_STALE_IF_ERROR,
    blind_after: Duration::from_secs(2 * 60),
};

/// Warnings are issued minutes to hours ahead of their effect, so a poll
/// every minute is plenty.
const WEATHER_WARNINGS_POLL: Schedule = Schedule {
    interval: Duration::from_secs(60),
    first_poll_after: Duration::ZERO,
    retry_after: Duration::from_secs(60),
    fresh_for: Duration::from_secs(60).saturating_add(HKO_REQUEST_TIMEOUT),
    stale_if_error: POLL_STALE_IF_ERROR,
    blind_after: Duration::from_secs(5 * 60),
};

/// Sampling only watches for changes, so it starts a minute after startup
/// rather than competing with riders' first requests.
const NEXT_TRAIN_SIGNALS_POLL: Schedule = Schedule {
    interval: Duration::from_secs(60),
    first_poll_after: Duration::from_secs(60),
    retry_after: Duration::from_secs(60),
    fresh_for: Duration::from_secs(60).saturating_add(MTR_REQUEST_TIMEOUT),
    stale_if_error: POLL_STALE_IF_ERROR,
    blind_after: Duration::from_secs(5 * 60),
};

/// Open data changes a few times a year, so it is read at startup and then
/// once a day, and responses may be reused for up to a day. A failed read is
/// retried within minutes, and the last good files are served, marked
/// stale, for a month before the endpoints give up.
const OPEN_DATA_POLL: Schedule = Schedule {
    interval: Duration::from_secs(24 * 60 * 60),
    first_poll_after: Duration::ZERO,
    retry_after: Duration::from_secs(5 * 60),
    fresh_for: Duration::from_secs(24 * 60 * 60).saturating_add(OPEN_DATA_REQUEST_TIMEOUT),
    stale_if_error: Duration::from_secs(30 * 24 * 60 * 60),
    blind_after: Duration::from_secs(48 * 60 * 60),
};

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

/// Settings for one server process. The default listens on localhost and
/// reads the live upstreams; the command line can change where it listens,
/// and tests swap the endpoints for a fake upstream.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppConfig {
    bind_address: SocketAddr,
    outbound_http_timeout: Duration,
    outbound_proxy: ProxyMode,
    mtr: MtrConfig,
    hko: HkoConfig,
    polling: PollingConfig,
}

/// Where and how to read the MTR's real-time feeds and open data.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MtrConfig {
    pub next_train_endpoint: String,
    pub line_status_endpoint: String,
    pub request_timeout: Duration,
    pub next_train_cache: CachePolicy,
    /// The open data portal's data directory, ending in `/`.
    pub open_data_endpoint: String,
    pub open_data_timeout: Duration,
}

/// Where and how to read the Hong Kong Observatory's weather warnings.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HkoConfig {
    pub warnings_endpoint: String,
    pub request_timeout: Duration,
}

/// How often each feed is read in the background, and how long what was
/// read stays usable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PollingConfig {
    pub line_status: Schedule,
    pub weather_warnings: Schedule,
    pub next_train_signals: Schedule,
    pub open_data: Schedule,
}

impl AppConfig {
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

    /// Points the open data adapter at another directory, e.g. a test double.
    /// The endpoint must end in `/`, since file names are joined onto it.
    #[must_use]
    pub fn with_mtr_open_data_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.mtr.open_data_endpoint = endpoint.into();
        self
    }

    /// Points the weather warning adapter at another endpoint, e.g. a test
    /// double. The endpoint is the full URL, query included.
    #[must_use]
    pub fn with_weather_warnings_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.hko.warnings_endpoint = endpoint.into();
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

    pub(crate) const fn with_bind_address(mut self, address: SocketAddr) -> Self {
        self.bind_address = address;
        self
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

    pub(crate) const fn hko(&self) -> &HkoConfig {
        &self.hko
    }

    pub(crate) const fn polling(&self) -> PollingConfig {
        self.polling
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            bind_address: DEFAULT_BIND_ADDRESS,
            outbound_http_timeout: DEFAULT_HTTP_TIMEOUT,
            outbound_proxy: ProxyMode::System,
            mtr: MtrConfig {
                next_train_endpoint: NEXT_TRAIN_ENDPOINT.to_owned(),
                line_status_endpoint: LINE_STATUS_ENDPOINT.to_owned(),
                request_timeout: MTR_REQUEST_TIMEOUT,
                next_train_cache: NEXT_TRAIN_CACHE,
                open_data_endpoint: OPEN_DATA_ENDPOINT.to_owned(),
                open_data_timeout: OPEN_DATA_REQUEST_TIMEOUT,
            },
            hko: HkoConfig {
                warnings_endpoint: WEATHER_WARNINGS_ENDPOINT.to_owned(),
                request_timeout: HKO_REQUEST_TIMEOUT,
            },
            polling: PollingConfig {
                line_status: LINE_STATUS_POLL,
                weather_warnings: WEATHER_WARNINGS_POLL,
                next_train_signals: NEXT_TRAIN_SIGNALS_POLL,
                open_data: OPEN_DATA_POLL,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listens_on_localhost_by_default() {
        let config = AppConfig::default();

        assert_eq!(
            config.bind_address(),
            SocketAddr::from((Ipv4Addr::LOCALHOST, 3000))
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
}
