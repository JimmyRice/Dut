use crate::support::TestApp;
use axum::http::{Method, StatusCode, header};

#[tokio::test]
async fn health_check_answers_ok_with_no_body() {
    // Nothing is mounted on the fake upstream: the check must not need it.
    let app = TestApp::start().await;

    let response = app.send(Method::GET, "/api/health").await;

    assert_eq!(response.status, StatusCode::OK);
    assert!(response.headers.get(header::CONTENT_TYPE).is_none());
    assert_eq!(
        response.headers.get(header::CACHE_CONTROL),
        Some(&header::HeaderValue::from_static("no-store"))
    );
    assert!(response.body.is_empty());
}

#[tokio::test]
async fn health_check_answers_head_requests() {
    let app = TestApp::start().await;

    let response = app.send(Method::HEAD, "/api/health").await;

    assert_eq!(response.status, StatusCode::OK);
    assert!(response.body.is_empty());
}

#[tokio::test]
async fn health_check_is_left_out_of_request_tracing() {
    let app = TestApp::start().await;

    let response = app.send(Method::GET, "/api/health").await;

    assert!(response.headers.get("x-request-id").is_none());
}
