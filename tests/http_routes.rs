mod support;

use axum::http::StatusCode;
use serde_json::json;
use support::TestApp;

#[tokio::test]
async fn unknown_route_returns_structured_json_error() {
    let app = TestApp::start().await;

    let response = app.get("/api/does-not-exist").await;

    response.assert_json(StatusCode::NOT_FOUND);
    assert_eq!(
        response.body,
        json!({
            "error": {
                "code": "not_found",
                "message": "No route matches /api/does-not-exist",
            }
        })
    );
}
