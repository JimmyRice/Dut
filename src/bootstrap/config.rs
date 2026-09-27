use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use crate::infrastructure::cache::CachePolicy;

const DEFAULT_PORT: u16 = 3000;
const DEFAULT_HTTP_TIMEOUT: Duration = Duration::from_secs(10);

const NEXT_TRAIN_ENDPOINT: &str = "https://rt.data.gov.hk/v1/transport/mtr/getSchedule.php";
const LINE_STATUS_ENDPOINT: &str = "https://tnews.mtr.com.hk/alert/ryg_line_status.json";

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
    mtr: MtrConfig,
}

/// Where and how to read the MTR's real-time feeds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MtrConfig {
    pub next_train_endpoint: String,
    pub line_status_endpoint: String,
    pub request_timeout: Duration,
    pub next_train_cache: CachePolicy,
    pub line_status_cache: CachePolicy,
}

impl AppConfig {
    const fn new(
        bind_address: SocketAddr,
        outbound_http_timeout: Duration,
        mtr: MtrConfig,
    ) -> Self {
        Self {
            bind_address,
            outbound_http_timeout,
            mtr,
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

    pub(crate) const fn bind_address(&self) -> SocketAddr {
        self.bind_address
    }

    pub(crate) const fn outbound_http_timeout(&self) -> Duration {
        self.outbound_http_timeout
    }

    pub(crate) const fn mtr(&self) -> &MtrConfig {
        &self.mtr
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self::new(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), DEFAULT_PORT),
            DEFAULT_HTTP_TIMEOUT,
            MtrConfig::default(),
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
