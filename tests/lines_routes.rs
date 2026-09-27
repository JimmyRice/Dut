mod support;

use axum::http::{StatusCode, header};
use serde_json::json;
use support::TestApp;
use wiremock::{Mock, ResponseTemplate, matchers::any};

#[tokio::test]
async fn lines_describe_the_static_network() {
    let app = TestApp::start().await;
    Mock::given(any())
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
