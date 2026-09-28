//! Shared helpers for route-level tests.

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{HeaderMap, Request, StatusCode, header},
};
use dut::{AppConfig, build_app};
use jiff::{Timestamp, tz};
use serde_json::{Map, Value, json};
use tower::ServiceExt;
use wiremock::MockServer;

pub(crate) const NEXT_TRAIN_PATH: &str = "/v1/transport/mtr/getSchedule.php";
pub(crate) const LINE_STATUS_PATH: &str = "/alert/ryg_line_status.json";

/// The application wired to a fake upstream.
pub(crate) struct TestApp {
    pub router: Router,
    pub upstream: MockServer,
}

impl TestApp {
    pub(crate) async fn start() -> Self {
        let upstream = MockServer::start().await;
        let config = AppConfig::default()
            .with_mtr_endpoints(
                format!("{}{NEXT_TRAIN_PATH}", upstream.uri()),
                format!("{}{LINE_STATUS_PATH}", upstream.uri()),
            )
            .without_proxy();
        let router = build_app(&config).expect("test application should build");

        Self { router, upstream }
    }

    pub(crate) async fn get(&self, uri: &str) -> TestResponse {
        let response = self
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .uri(uri)
                    .body(Body::empty())
                    .expect("request should be valid"),
            )
            .await
            .expect("route should return a response");

        let status = response.status();
        let headers = response.headers().clone();
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body should be readable");
        let body = serde_json::from_slice(&body).expect("response should contain valid JSON");

        TestResponse {
            status,
            headers,
            body,
        }
    }
}

pub(crate) struct TestResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Value,
}

impl TestResponse {
    pub(crate) fn header(&self, name: header::HeaderName) -> &str {
        self.headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
    }

    pub(crate) fn assert_json(&self, status: StatusCode) {
        assert_eq!(
            self.status, status,
            "unexpected status; body: {}",
            self.body
        );
        assert_eq!(self.header(header::CONTENT_TYPE), "application/json");
    }

    /// The `max-age` of a `public, max-age=N` cache header.
    pub(crate) fn max_age(&self) -> u64 {
        self.header(header::CACHE_CONTROL)
            .strip_prefix("public, max-age=")
            .and_then(|seconds| seconds.parse().ok())
            .unwrap_or_else(|| {
                panic!(
                    "expected a public max-age, got {:?}",
                    self.header(header::CACHE_CONTROL)
                )
            })
    }
}

/// A point in time relative to the start of a test, in both the upstream's
/// local format and this API's RFC 3339 format.
#[derive(Clone, Copy)]
pub(crate) struct Moment(Timestamp);

impl Moment {
    pub(crate) fn now() -> Self {
        let now = Timestamp::now();
        Self(Timestamp::from_second(now.as_second()).expect("now is in range"))
    }

    pub(crate) fn plus_seconds(self, seconds: i64) -> Self {
        Self(Timestamp::from_second(self.0.as_second() + seconds).expect("time is in range"))
    }

    /// `yyyy-MM-dd HH:mm:ss` in Hong Kong Time, as MTR feeds publish it.
    pub(crate) fn upstream(self) -> String {
        self.0
            .to_zoned(tz::TimeZone::fixed(tz::offset(8)))
            .strftime("%Y-%m-%d %H:%M:%S")
            .to_string()
    }

    /// RFC 3339 in Hong Kong Time, as this API returns it.
    pub(crate) fn api(self) -> String {
        self.0.display_with_offset(tz::offset(8)).to_string()
    }
}

/// A train in an upstream Next Train response.
pub(crate) struct Train {
    pub dest: &'static str,
    pub at: Moment,
}

/// A successful Next Train API response for one station.
pub(crate) fn schedule_body(
    line: &str,
    station: &str,
    generated_at: Moment,
    up: &[Train],
    down: &[Train],
) -> Value {
    let trains = |trains: &[Train]| -> Value {
        trains
            .iter()
            .enumerate()
            .map(|(index, train)| {
                json!({
                    "seq": (index + 1).to_string(),
                    "dest": train.dest,
                    "plat": "1",
                    "time": train.at.upstream(),
                    "ttnt": "0",
                    "valid": "Y",
                    "source": "-",
                })
            })
            .collect()
    };

    let mut data = Map::new();
    data.insert(
        format!("{line}-{station}"),
        json!({
            "curr_time": generated_at.upstream(),
            "sys_time": generated_at.upstream(),
            "UP": trains(up),
            "DOWN": trains(down),
        }),
    );

    json!({
        "sys_time": generated_at.upstream(),
        "curr_time": generated_at.upstream(),
        "data": data,
        "isdelay": "N",
        "status": 1,
        "message": "successful",
    })
}

/// The captured live line status feed.
pub(crate) fn line_status_fixture() -> Value {
    let path = format!(
        "{}/tests/fixtures/mtr/line_status.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let body = std::fs::read(path).expect("fixture should exist");
    serde_json::from_slice(&body).expect("fixture should be valid JSON")
}
