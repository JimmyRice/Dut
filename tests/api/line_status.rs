use crate::support::{LINE_STATUS_PATH, TestApp, line_status_fixture};
use axum::http::StatusCode;
use serde_json::{Value, json};
use wiremock::{
    Mock, ResponseTemplate,
    matchers::{method, path},
};

async fn mount_feed(app: &TestApp, response: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path(LINE_STATUS_PATH))
        .respond_with(response)
        .expect(1)
        .mount(&app.upstream)
        .await;
}

#[tokio::test]
async fn line_status_returns_every_line() {
    let app = TestApp::start().await;
    mount_feed(
        &app,
        ResponseTemplate::new(200).set_body_json(line_status_fixture()),
    )
    .await;

    let response = app.get("/api/lines/status").await;

    response.assert_json(StatusCode::OK);
    assert!((29..=30).contains(&response.max_age()));
    assert_eq!(response.body["updated_at"], "2026-09-27T06:15:00+08:00");
    assert_eq!(response.body["stale"], false);
    assert!(response.body["fetched_at"].is_string());

    let lines = response.body["lines"]
        .as_array()
        .expect("lines should be an array");
    assert_eq!(lines.len(), 11);
    assert_eq!(
        lines[0],
        json!({
            "line": { "code": "TWL", "name": { "en": "Tsuen Wan Line", "tc": "荃灣綫" } },
            "color": "#FF0000",
            "condition": "normal",
            "display": "green",
            "message": null,
        })
    );
    assert_eq!(lines[10]["line"]["code"], "LR");
}

#[tokio::test]
async fn line_status_is_served_from_cache_on_repeat_requests() {
    let app = TestApp::start().await;
    mount_feed(
        &app,
        ResponseTemplate::new(200).set_body_json(line_status_fixture()),
    )
    .await;

    let first = app.get("/api/lines/status").await;
    let second = app.get("/api/lines/status").await;

    first.assert_json(StatusCode::OK);
    second.assert_json(StatusCode::OK);
    assert_eq!(first.body, second.body);
}

#[tokio::test]
async fn line_status_reports_meaning_and_website_colour() {
    let app = TestApp::start().await;
    let feed = json!({
        "ryg_status": {
            "lastBuildDate": "2026-09-27 08:00:00",
            "line": [
                { "line_code": "KTL", "status": "pink", "messages": "Trains are delayed" },
                { "line_code": "ISL", "status": "red", "messages": "" },
                { "line_code": "EAL", "status": "typhoon", "messages": "" },
                { "line_code": "AEL", "status": "grey", "messages": "" },
            ],
        }
    });
    mount_feed(&app, ResponseTemplate::new(200).set_body_json(feed)).await;

    let response = app.get("/api/lines/status").await;

    response.assert_json(StatusCode::OK);
    let summary: Vec<Value> = response.body["lines"]
        .as_array()
        .expect("lines should be an array")
        .iter()
        .map(|entry| {
            json!([
                entry["line"]["code"],
                entry["condition"],
                entry["display"],
                entry["message"]
            ])
        })
        .collect();
    assert_eq!(
        summary,
        [
            json!([
                "KTL",
                "delayed_or_disrupted",
                "yellow",
                "Trains are delayed"
            ]),
            json!(["ISL", "disrupted", "red", null]),
            json!(["EAL", "typhoon_signal", "typhoon", null]),
            json!(["AEL", "non_service_hours", "grey", null]),
        ]
    );
}

#[tokio::test]
async fn line_status_upstream_failure_returns_bad_gateway() {
    let app = TestApp::start().await;
    mount_feed(&app, ResponseTemplate::new(500)).await;

    let response = app.get("/api/lines/status").await;

    response.assert_json(StatusCode::BAD_GATEWAY);
    assert_eq!(response.body["error"]["code"], "upstream_unavailable");
}
