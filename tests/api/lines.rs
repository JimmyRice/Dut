use crate::support::{NEXT_TRAIN_PATH, TestApp, assert_gzip_matches_plain};
use axum::http::{Method, StatusCode, header};
use serde_json::json;
use wiremock::{Mock, ResponseTemplate, matchers::path};

#[tokio::test]
async fn lines_describe_the_static_network() {
    let app = TestApp::start().await;
    // Background polls read their own feeds; only a per-request call would
    // reach the Next Train API.
    Mock::given(path(NEXT_TRAIN_PATH))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&app.upstream)
        .await;

    let response = app.get("/api/lines").await;

    response.assert_json(StatusCode::OK);
    assert_eq!(
        response.header(header::CACHE_CONTROL),
        "public, max-age=86400"
    );

    let lines = response.body["lines"]
        .as_array()
        .expect("lines should be an array");
    let codes: Vec<&str> = lines
        .iter()
        .map(|line| line["code"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(
        codes,
        [
            "AEL", "TCL", "TML", "TKL", "EAL", "SIL", "TWL", "ISL", "KTL", "DRL"
        ]
    );

    assert_eq!(
        lines[9],
        json!({
            "code": "DRL",
            "name": { "en": "Disneyland Resort Line", "tc": "迪士尼綫" },
            "color": "#F550A6",
            "stations": [
                { "code": "SUN", "name": { "en": "Sunny Bay", "tc": "欣澳" } },
                { "code": "DIS", "name": { "en": "Disneyland Resort", "tc": "迪士尼" } },
            ],
            "directions": [
                {
                    "direction": "up",
                    "towards": [{ "code": "SUN", "name": { "en": "Sunny Bay", "tc": "欣澳" } }],
                },
                {
                    "direction": "down",
                    "towards": [{ "code": "DIS", "name": { "en": "Disneyland Resort", "tc": "迪士尼" } }],
                },
            ],
        })
    );
    assert_eq!(lines[3]["stations"].as_array().map(Vec::len), Some(8));
    assert_eq!(
        lines[3]["directions"][0]["towards"],
        json!([
            { "code": "POA", "name": { "en": "Po Lam", "tc": "寶琳" } },
            { "code": "LHP", "name": { "en": "LOHAS Park", "tc": "康城" } },
        ])
    );
}

#[tokio::test]
async fn lines_carry_an_etag_that_revalidates() {
    let app = TestApp::start().await;

    let first = app.send(Method::GET, "/api/lines").await;
    let etag = first.header(header::ETAG).to_owned();
    assert!(etag.starts_with("W/\""), "unexpected ETag {etag}");
    assert_eq!(
        app.send(Method::GET, "/api/lines")
            .await
            .header(header::ETAG),
        etag
    );

    let revalidated = app
        .send_with_headers(
            Method::GET,
            "/api/lines",
            &[(header::IF_NONE_MATCH, etag.as_str())],
        )
        .await;
    assert_eq!(revalidated.status, StatusCode::NOT_MODIFIED);
    assert!(revalidated.body.is_empty());
    assert_eq!(revalidated.header(header::ETAG), etag);
    assert_eq!(
        revalidated.header(header::CACHE_CONTROL),
        "public, max-age=86400"
    );

    let outdated = app
        .send_with_headers(
            Method::GET,
            "/api/lines",
            &[(header::IF_NONE_MATCH, "W/\"0.1.0-0\"")],
        )
        .await;
    assert_eq!(outdated.status, StatusCode::OK);
    assert_eq!(outdated.body, first.body);
}

#[tokio::test]
async fn lines_are_gzipped_for_clients_that_accept_it() {
    let app = TestApp::start().await;

    assert_gzip_matches_plain(&app, "/api/lines").await;
}
