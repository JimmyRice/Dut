use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use dut::{
    AppConfig,
    api::dto::{ErrorResponse, HelloResponse},
    build_app,
};
use tower::ServiceExt;

const HELLO_MESSAGE: &str = "Hello, World!";

fn app() -> axum::Router {
    build_app(&AppConfig::default()).expect("test application should build")
}

#[tokio::test]
async fn hello_route_returns_plain_text() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/api/hello")
                .body(Body::empty())
                .expect("request should be valid"),
        )
        .await
        .expect("route should return a response");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE),
        Some(&header::HeaderValue::from_static(
            "text/plain; charset=utf-8"
        ))
    );

    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should be readable");
    assert_eq!(body.as_ref(), HELLO_MESSAGE.as_bytes());
}

#[tokio::test]
async fn hello_json_route_returns_json() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/api/hello.json")
                .body(Body::empty())
                .expect("request should be valid"),
        )
        .await
        .expect("route should return a response");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE),
        Some(&header::HeaderValue::from_static("application/json"))
    );

    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should be readable");
    assert_eq!(body.as_ref(), br#"{"message":"Hello, World!"}"#);

    let payload: HelloResponse =
        serde_json::from_slice(&body).expect("response should contain valid JSON");
    assert_eq!(
        payload,
        HelloResponse {
            message: HELLO_MESSAGE.to_owned(),
        }
    );
}

#[tokio::test]
async fn unknown_route_returns_structured_json_error() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/api/does-not-exist")
                .body(Body::empty())
                .expect("request should be valid"),
        )
        .await
        .expect("fallback should return a response");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE),
        Some(&header::HeaderValue::from_static("application/json"))
    );

    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should be readable");
    let payload: ErrorResponse =
        serde_json::from_slice(&body).expect("response should contain valid JSON");
    assert_eq!(payload.error.code, "not_found");
    assert_eq!(
        payload.error.message,
        "No route matches /api/does-not-exist"
    );
}
