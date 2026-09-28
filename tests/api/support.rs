//! Shared helpers for route-level tests.

use axum::{
    Router,
    body::{Body, Bytes, to_bytes},
    http::{HeaderMap, HeaderName, Method, Request, StatusCode, header},
};
use dut::{AppConfig, build_app};
use jiff::{Timestamp, tz};
use serde_json::{Map, Value, json};
use tower::ServiceExt;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

pub(crate) const NEXT_TRAIN_PATH: &str = "/v1/transport/mtr/getSchedule.php";
pub(crate) const LINE_STATUS_PATH: &str = "/alert/ryg_line_status.json";
const WEATHER_WARNINGS_PATH: &str = "/weatherAPI/opendata/weather.php";
const OPEN_DATA_PATH: &str = "/data/";

/// The files on the MTR's open data portal, as `tests/fixtures` holds them.
/// The two fare files are trimmed to trips between a few stops.
pub(crate) const OPEN_DATA_FILES: [&str; 7] = [
    "mtr_lines_and_stations.csv",
    "mtr_lines_fares.csv",
    "airport_express_fares.csv",
    "light_rail_routes_and_stops.csv",
    "light_rail_fares.csv",
    "barrier_free_facility_category.csv",
    "barrier_free_facilities.csv",
];

/// The `Last-Modified` the fake portal sends with every file.
pub(crate) const OPEN_DATA_LAST_MODIFIED: &str = "Thu, 02 Apr 2026 17:02:50 GMT";

/// The application wired to a fake upstream.
pub(crate) struct TestApp {
    pub router: Router,
    pub upstream: MockServer,
}

impl TestApp {
    /// The application wired to a fake upstream that has nothing mounted
    /// yet, so the line status feed is unavailable until a test mounts it.
    pub(crate) async fn start() -> Self {
        Self::serving(MockServer::start().await).await
    }

    /// The application wired to `upstream`.
    ///
    /// Background polls read line status, weather warnings, and open data as
    /// soon as the application is built, so mount what they should find
    /// first. Weather warnings default to none in force; open data is
    /// unavailable unless [`mount_open_data`] was called.
    pub(crate) async fn serving(upstream: MockServer) -> Self {
        Mock::given(method("GET"))
            .and(path(WEATHER_WARNINGS_PATH))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .mount(&upstream)
            .await;
        let config = AppConfig::default()
            .with_mtr_endpoints(
                format!("{}{NEXT_TRAIN_PATH}", upstream.uri()),
                format!("{}{LINE_STATUS_PATH}", upstream.uri()),
            )
            .with_weather_warnings_endpoint(format!(
                "{}{WEATHER_WARNINGS_PATH}?dataType=warningInfo&lang=en",
                upstream.uri()
            ))
            .with_mtr_open_data_endpoint(format!("{}{OPEN_DATA_PATH}", upstream.uri()))
            .without_proxy();
        let router = build_app(&config).expect("test application should build");

        Self { router, upstream }
    }

    /// Sends a `GET` and decodes the JSON body.
    pub(crate) async fn get(&self, uri: &str) -> TestResponse {
        let RawResponse {
            status,
            headers,
            body,
        } = self.send(Method::GET, uri).await;
        let body = serde_json::from_slice(&body).expect("response should contain valid JSON");

        TestResponse {
            status,
            headers,
            body,
        }
    }

    /// Sends a request without a body and keeps the response body as bytes,
    /// for routes that do not answer with JSON.
    pub(crate) async fn send(&self, method: Method, uri: &str) -> RawResponse {
        self.send_with_headers(method, uri, &[]).await
    }

    /// Sends a request with the given headers, such as `If-None-Match`, and
    /// keeps the response body as bytes.
    pub(crate) async fn send_with_headers(
        &self,
        method: Method,
        uri: &str,
        headers: &[(HeaderName, &str)],
    ) -> RawResponse {
        let mut request = Request::builder().method(method).uri(uri);
        for (name, value) in headers {
            request = request.header(name, *value);
        }
        let response = self
            .router
            .clone()
            .oneshot(
                request
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

        RawResponse {
            status,
            headers,
            body,
        }
    }
}

/// A response whose body is kept as received.
pub(crate) struct RawResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Bytes,
}

impl RawResponse {
    pub(crate) fn header(&self, name: header::HeaderName) -> &str {
        self.headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
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

/// A captured file from the MTR's open data portal.
pub(crate) fn open_data_fixture(file: &str) -> Vec<u8> {
    let path = format!(
        "{}/tests/fixtures/mtr/open_data/{file}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read(path).expect("fixture should exist")
}

/// Serves every captured open data file, except `failing`, which answers
/// `500`.
pub(crate) async fn mount_open_data(upstream: &MockServer, failing: Option<&str>) {
    for file in OPEN_DATA_FILES {
        let response = if Some(file) == failing {
            ResponseTemplate::new(500)
        } else {
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/csv")
                .insert_header("last-modified", OPEN_DATA_LAST_MODIFIED)
                .set_body_bytes(open_data_fixture(file))
        };
        Mock::given(method("GET"))
            .and(path(format!("{OPEN_DATA_PATH}{file}")))
            .respond_with(response)
            .mount(upstream)
            .await;
    }
}
