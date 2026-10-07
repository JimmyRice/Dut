use crate::support::{NEXT_TRAIN_PATH, TestApp, TestResponse};
use axum::http::{StatusCode, header};
use serde_json::{Value, json};
use wiremock::{Mock, ResponseTemplate, matchers::path};

const NEXT_TRAIN_SCENARIOS: [&str; 11] = [
    "peak",
    "off_peak",
    "late_night",
    "last_train",
    "non_service_hours",
    "delayed",
    "special_arrangement",
    "race_day",
    "stale",
    "partial_outage",
    "upstream_unavailable",
];

const LINE_STATUS_SCENARIOS: [&str; 9] = [
    "normal",
    "delayed",
    "disrupted",
    "delayed_or_disrupted",
    "typhoon_signal",
    "non_service_hours",
    "unknown_condition",
    "stale",
    "upstream_unavailable",
];

fn station(code: &str, en: &str, tc: &str) -> Value {
    json!({ "code": code, "name": { "en": en, "tc": tc } })
}

/// The scenario and seed a response says it was simulated in.
fn simulated_in(response: &TestResponse) -> (&str, u64) {
    let seed = response
        .header(header::HeaderName::from_static("x-mock-seed"))
        .parse()
        .expect("the seed should be a number");
    (
        response.header(header::HeaderName::from_static("x-mock-scenario")),
        seed,
    )
}

fn array(value: &Value) -> &Vec<Value> {
    value.as_array().expect("value should be an array")
}

#[tokio::test]
async fn the_mock_api_is_off_unless_enabled() {
    let app = TestApp::start().await;

    let response = app.get("/api/mock/lines/status?scenario=normal").await;

    response.assert_json(StatusCode::NOT_FOUND);
    assert_eq!(response.body["error"]["code"], "not_found");
}

#[tokio::test]
async fn lists_the_scenarios_of_each_kind_of_data() {
    let app = TestApp::with_mock_api().await;

    let response = app.get("/api/mock/scenarios").await;

    response.assert_json(StatusCode::OK);
    assert_eq!(
        response.header(header::CACHE_CONTROL),
        "public, max-age=86400"
    );
    let names = |kind: &str| -> Vec<String> {
        array(&response.body[kind])
            .iter()
            .map(|entry| entry["scenario"].as_str().unwrap_or_default().to_owned())
            .collect()
    };
    assert_eq!(names("next_trains"), NEXT_TRAIN_SCENARIOS);
    assert_eq!(names("line_status"), LINE_STATUS_SCENARIOS);
    assert_eq!(
        response.body["line_status"][0],
        json!({
            "scenario": "normal",
            "random_weight": 8,
            "description": { "en": "Good service on every line.", "tc": "所有綫路服務正常。" },
        })
    );
}

#[tokio::test]
async fn simulates_the_line_status_feed() {
    let app = TestApp::with_mock_api().await;

    let response = app
        .get("/api/mock/lines/status?scenario=typhoon_signal&seed=1")
        .await;

    response.assert_json(StatusCode::OK);
    assert_eq!(simulated_in(&response), ("typhoon_signal", 1));
    assert!((4..=33).contains(&response.max_age()));
    assert_eq!(response.body["stale"], false);
    assert!(response.body["updated_at"].is_string());
    assert!(response.body["fetched_at"].is_string());
    let lines = array(&response.body["lines"]);
    assert_eq!(lines.len(), 11);
    assert_eq!(
        lines[0],
        json!({
            "line": { "code": "TWL", "name": { "en": "Tsuen Wan Line", "tc": "荃灣綫" } },
            "color": "#FF0000",
            "condition": "typhoon_signal",
            "display": "typhoon",
            "message": null,
        })
    );
    assert!(lines.iter().all(|line| line["display"] == "typhoon"));
    assert_eq!(lines[10]["line"]["code"], "LR");
}

#[tokio::test]
async fn a_delayed_line_comes_with_an_explanation() {
    let app = TestApp::with_mock_api().await;

    let response = app.get("/api/mock/lines/status?scenario=delayed").await;

    response.assert_json(StatusCode::OK);
    assert_eq!(simulated_in(&response), ("delayed", 0));
    let delayed: Vec<&Value> = array(&response.body["lines"])
        .iter()
        .filter(|line| line["condition"] != "normal")
        .collect();
    assert_eq!(delayed.len(), 1);
    assert_eq!(delayed[0]["condition"], "delayed");
    assert_eq!(delayed[0]["display"], "yellow");
    let message = delayed[0]["message"].as_str().unwrap_or_default();
    assert!(
        message.ends_with("Passengers please allow extra travelling time."),
        "{message}"
    );
}

#[tokio::test]
async fn simulates_a_board_shaped_like_the_real_one() {
    let app = TestApp::with_mock_api().await;

    let response = app
        .get("/api/mock/lines/tkl/stations/tko/next-trains?scenario=off_peak&seed=4")
        .await;

    response.assert_json(StatusCode::OK);
    assert_eq!(simulated_in(&response), ("off_peak", 4));
    assert!((2..=10).contains(&response.max_age()));
    let body = &response.body;
    assert_eq!(
        body["line"],
        json!({ "code": "TKL", "name": { "en": "Tseung Kwan O Line", "tc": "將軍澳綫" } })
    );
    assert_eq!(body["station"], station("TKO", "Tseung Kwan O", "將軍澳"));
    assert_eq!(
        (&body["stale"], &body["delayed"], &body["alert"]),
        (&json!(false), &json!(false), &Value::Null)
    );
    let generated_at = body["generated_at"].as_str().unwrap_or_default();
    assert!(body["fetched_at"].as_str().unwrap_or_default() > generated_at);

    let directions = array(&body["directions"]);
    assert_eq!(directions.len(), 2);
    for (direction, towards, platform) in [
        (
            &directions[0],
            json!([
                station("POA", "Po Lam", "寶琳"),
                station("LHP", "LOHAS Park", "康城"),
            ]),
            1,
        ),
        (
            &directions[1],
            json!([station("NOP", "North Point", "北角")]),
            2,
        ),
    ] {
        assert_eq!(direction["towards"], towards);
        let trains = array(&direction["trains"]);
        assert_eq!(trains.len(), 4);
        for train in trains {
            assert!(array(&towards).contains(&train["destination"]), "{train}");
            assert_eq!(train["platforms"], json!([platform]));
            assert!(train["arrival_at"].as_str().unwrap_or_default() >= generated_at);
            assert_eq!(train["time_type"], Value::Null);
            assert_eq!(train["via_racecourse"], false);
        }
    }
}

#[tokio::test]
async fn a_special_arrangement_comes_with_the_mtr_notice() {
    let app = TestApp::with_mock_api().await;

    let response = app
        .get("/api/mock/lines/ISL/stations/CEN/next-trains?scenario=special_arrangement")
        .await;

    response.assert_json(StatusCode::OK);
    let url = "https://www.mtr.com.hk/alert/alert_title_wap.html";
    assert_eq!(
        response.body["alert"],
        json!({
            "en": {
                "message": "Special train service arrangements are now in place on this line. Please click here for more information.",
                "url": url,
            },
            "tc": { "message": "此綫路現正實施特別列車服務安排，詳情請按此。", "url": url },
        })
    );
}

#[tokio::test]
async fn a_stale_board_must_not_be_reused() {
    let app = TestApp::with_mock_api().await;

    let response = app
        .get("/api/mock/lines/KTL/stations/KOT/next-trains?scenario=stale")
        .await;

    response.assert_json(StatusCode::OK);
    assert_eq!(response.header(header::CACHE_CONTROL), "no-cache");
    assert_eq!(response.body["stale"], true);
}

#[tokio::test]
async fn a_partial_outage_fails_one_line_at_an_interchange() {
    let app = TestApp::with_mock_api().await;

    let response = app
        .get("/api/mock/stations/ADM/next-trains?scenario=partial_outage")
        .await;

    response.assert_json(StatusCode::OK);
    assert_eq!(
        response.body["station"],
        station("ADM", "Admiralty", "金鐘")
    );
    let lines = array(&response.body["lines"]);
    assert_eq!(lines.len(), 4);
    let failed: Vec<&Value> = lines
        .iter()
        .filter(|line| line["board"].is_null())
        .collect();
    assert_eq!(failed.len(), 1);
    assert_eq!(
        failed[0]["error"],
        json!({ "code": "upstream_unavailable", "message": "An upstream service is unavailable" })
    );
}

#[tokio::test]
async fn an_unreachable_mtr_answers_502_naming_the_scenario() {
    let app = TestApp::with_mock_api().await;

    let response = app
        .get("/api/mock/stations/TKO/next-trains?scenario=upstream_unavailable&seed=9")
        .await;

    response.assert_json(StatusCode::BAD_GATEWAY);
    assert_eq!(simulated_in(&response), ("upstream_unavailable", 9));
    assert_eq!(response.header(header::CACHE_CONTROL), "");
    assert_eq!(response.body["error"]["code"], "upstream_unavailable");
}

#[tokio::test]
async fn a_random_scenario_is_reproduced_by_its_seed() {
    let app = TestApp::with_mock_api().await;

    let drawn = app.get("/api/mock/lines/status").await;
    let (scenario, seed) = simulated_in(&drawn);
    let replayed = app
        .get(&format!(
            "/api/mock/lines/status?scenario=random&seed={seed}"
        ))
        .await;

    assert!(LINE_STATUS_SCENARIOS.contains(&scenario), "{scenario}");
    assert_eq!(simulated_in(&replayed), (scenario, seed));
    assert_eq!(replayed.status, drawn.status);
}

#[tokio::test]
async fn rejects_unknown_scenarios_and_malformed_queries() {
    let app = TestApp::with_mock_api().await;

    let cases = [
        (
            "/api/mock/lines/status?scenario=typhoon",
            StatusCode::BAD_REQUEST,
            "unknown_scenario",
        ),
        (
            "/api/mock/stations/ADM/next-trains?scenario=normal",
            StatusCode::BAD_REQUEST,
            "unknown_scenario",
        ),
        (
            "/api/mock/lines/status?seed=-1",
            StatusCode::BAD_REQUEST,
            "invalid_query",
        ),
        (
            "/api/mock/lines/status?seed=1&seed=2",
            StatusCode::BAD_REQUEST,
            "invalid_query",
        ),
        (
            "/api/mock/lines/XYZ/stations/TKO/next-trains",
            StatusCode::NOT_FOUND,
            "unknown_line",
        ),
        (
            "/api/mock/lines/TKL/stations/ADM/next-trains",
            StatusCode::NOT_FOUND,
            "station_not_on_line",
        ),
    ];
    for (uri, status, code) in cases {
        let response = app.get(uri).await;

        response.assert_json(status);
        assert_eq!(response.body["error"]["code"], code, "{uri}");
    }
}

#[tokio::test]
async fn simulated_boards_never_reach_the_mtr() {
    let app = TestApp::with_mock_api().await;
    Mock::given(path(NEXT_TRAIN_PATH))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&app.upstream)
        .await;

    for scenario in NEXT_TRAIN_SCENARIOS {
        app.get(&format!(
            "/api/mock/stations/ADM/next-trains?scenario={scenario}"
        ))
        .await;
    }
}

#[tokio::test]
async fn the_event_stream_opens_with_hello_and_reports_an_incident_and_its_recovery() {
    let app = TestApp::with_mock_api().await;

    let mut stream = app
        .open_stream("/api/mock/events?scenario=delayed&seed=7&interval=1")
        .await;

    assert_eq!(stream.status, StatusCode::OK);
    assert_eq!(stream.header(header::CONTENT_TYPE), "text/event-stream");
    assert_eq!(stream.header(header::CACHE_CONTROL), "no-cache");
    assert_eq!(
        stream.header(header::HeaderName::from_static("x-mock-scenario")),
        "delayed"
    );
    assert_eq!(
        stream.header(header::HeaderName::from_static("x-mock-seed")),
        "7"
    );

    let (name, hello) = stream.next_event().await;
    assert_eq!(name, "hello");
    assert!(
        hello["server_time"]
            .as_str()
            .is_some_and(|time| time.ends_with("+08:00"))
    );

    let (name, incident) = stream.next_event().await;
    assert_eq!(name, "line_status");
    assert_eq!(incident["previous"]["condition"], "normal");
    assert_eq!(incident["current"]["condition"], "delayed");
    assert_eq!(incident["current"]["display"], "yellow");
    assert!(incident["current"]["message"].as_str().is_some());
    assert!(
        incident["observed_at"]
            .as_str()
            .is_some_and(|time| time.ends_with("+08:00"))
    );
    assert!(incident["line"]["name"]["tc"].as_str().is_some());
    assert!(
        incident["color"]
            .as_str()
            .is_some_and(|color| color.starts_with('#'))
    );

    let (name, recovery) = stream.next_event().await;
    assert_eq!(name, "line_status");
    assert_eq!(recovery["line"]["code"], incident["line"]["code"]);
    assert_eq!(recovery["previous"]["condition"], "delayed");
    assert_eq!(recovery["current"]["condition"], "normal");
    assert_eq!(recovery["current"]["message"], Value::Null);
}

#[tokio::test]
async fn a_network_wide_scenario_sends_one_event_per_line() {
    let app = TestApp::with_mock_api().await;
    let mut stream = app
        .open_stream("/api/mock/events?scenario=typhoon_signal&interval=1")
        .await;

    assert_eq!(stream.next_event().await.0, "hello");
    let mut lines = Vec::new();
    for _ in 0..11 {
        let (name, event) = stream.next_event().await;
        assert_eq!(name, "line_status");
        assert_eq!(event["current"]["display"], "typhoon");
        lines.push(
            event["line"]["code"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
        );
    }
    lines.sort();
    lines.dedup();
    assert_eq!(lines.len(), 11);
}

#[tokio::test]
async fn the_event_stream_rejects_what_it_cannot_pace_or_simulate() {
    let app = TestApp::with_mock_api().await;

    for (uri, status, code) in [
        (
            "/api/mock/events?interval=0",
            StatusCode::BAD_REQUEST,
            "invalid_query",
        ),
        (
            "/api/mock/events?interval=61",
            StatusCode::BAD_REQUEST,
            "invalid_query",
        ),
        (
            "/api/mock/events?interval=soon",
            StatusCode::BAD_REQUEST,
            "invalid_query",
        ),
        (
            "/api/mock/events?scenario=heatwave",
            StatusCode::BAD_REQUEST,
            "unknown_scenario",
        ),
    ] {
        let response = app.get(uri).await;

        response.assert_json(status);
        assert_eq!(response.body["error"]["code"], code, "{uri}");
    }
}

#[tokio::test]
async fn the_event_stream_is_off_unless_the_mock_api_is_enabled() {
    let app = TestApp::start().await;

    let response = app.get("/api/mock/events").await;

    response.assert_json(StatusCode::NOT_FOUND);
}
