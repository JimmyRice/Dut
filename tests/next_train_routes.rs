mod support;

use axum::http::{StatusCode, header};
use serde_json::{Value, json};
use support::{Moment, NEXT_TRAIN_PATH, TestApp, Train, schedule_body};
use wiremock::{
    Mock, ResponseTemplate,
    matchers::{any, method, path, query_param},
};

fn upstream_ok(body: Value) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .set_body_json(body)
        .insert_header("cache-control", "public, must-revalidate, max-age=10")
}

async fn mount_board(app: &TestApp, line: &str, station: &str, response: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path(NEXT_TRAIN_PATH))
        .and(query_param("line", line))
        .and(query_param("sta", station))
        .and(query_param("lang", "EN"))
        .respond_with(response)
        .expect(1)
        .mount(&app.upstream)
        .await;
}

fn station(code: &str, en: &str, tc: &str) -> Value {
    json!({ "code": code, "name": { "en": en, "tc": tc } })
}

#[tokio::test]
async fn line_board_returns_upcoming_trains_in_hong_kong_time() {
    let app = TestApp::start().await;
    let now = Moment::now();
    let departed = now.plus_seconds(-300);
    let first = now.plus_seconds(60);
    let second = now.plus_seconds(180);
    let down = now.plus_seconds(120);
    mount_board(
        &app,
        "TKL",
        "TKO",
        upstream_ok(schedule_body(
            "TKL",
            "TKO",
            now,
            &[
                Train {
                    dest: "POA",
                    at: departed,
                },
                Train {
                    dest: "LHP",
                    at: first,
                },
                Train {
                    dest: "POA",
                    at: second,
                },
            ],
            &[Train {
                dest: "NOP",
                at: down,
            }],
        )),
    )
    .await;

    let mut response = app.get("/api/lines/tkl/stations/tko/next-trains").await;

    response.assert_json(StatusCode::OK);
    assert!((1..=10).contains(&response.max_age()));
    let fetched_at = response.body["fetched_at"].take();
    assert!(fetched_at.is_string());
    assert_eq!(
        response.body,
        json!({
            "line": { "code": "TKL", "name": { "en": "Tseung Kwan O Line", "tc": "將軍澳綫" } },
            "station": station("TKO", "Tseung Kwan O", "將軍澳"),
            "generated_at": now.api(),
            "fetched_at": null,
            "stale": false,
            "delayed": false,
            "alert": null,
            "directions": [
                {
                    "direction": "up",
                    "towards": [
                        station("POA", "Po Lam", "寶琳"),
                        station("LHP", "LOHAS Park", "康城"),
                    ],
                    "trains": [
                        {
                            "destination": station("LHP", "LOHAS Park", "康城"),
                            "platform": 1,
                            "arrival_at": first.api(),
                            "time_type": null,
                            "via_racecourse": false,
                        },
                        {
                            "destination": station("POA", "Po Lam", "寶琳"),
                            "platform": 1,
                            "arrival_at": second.api(),
                            "time_type": null,
                            "via_racecourse": false,
                        },
                    ],
                },
                {
                    "direction": "down",
                    "towards": [station("NOP", "North Point", "北角")],
                    "trains": [
                        {
                            "destination": station("NOP", "North Point", "北角"),
                            "platform": 1,
                            "arrival_at": down.api(),
                            "time_type": null,
                            "via_racecourse": false,
                        },
                    ],
                },
            ],
        })
    );
}

#[tokio::test]
async fn a_terminus_only_shows_the_direction_leaving_it() {
    let app = TestApp::start().await;
    let now = Moment::now();
    let departure = now.plus_seconds(120);
    mount_board(
        &app,
        "TKL",
        "POA",
        upstream_ok(schedule_body(
            "TKL",
            "POA",
            now,
            &[],
            &[Train {
                dest: "NOP",
                at: departure,
            }],
        )),
    )
    .await;

    let response = app.get("/api/lines/TKL/stations/POA/next-trains").await;

    response.assert_json(StatusCode::OK);
    assert_eq!(
        response.body["directions"],
        json!([
            {
                "direction": "down",
                "towards": [station("NOP", "North Point", "北角")],
                "trains": [
                    {
                        "destination": station("NOP", "North Point", "北角"),
                        "platform": 1,
                        "arrival_at": departure.api(),
                        "time_type": null,
                        "via_racecourse": false,
                    },
                ],
            },
        ])
    );
}

#[tokio::test]
async fn line_board_is_served_from_cache_on_repeat_requests() {
    let app = TestApp::start().await;
    let now = Moment::now();
    let body = schedule_body(
        "TKL",
        "TKO",
        now,
        &[Train {
            dest: "POA",
            at: now.plus_seconds(120),
        }],
        &[],
    );
    // `expect(1)` fails the test on drop if upstream is called again.
    mount_board(&app, "TKL", "TKO", upstream_ok(body)).await;

    let first = app.get("/api/lines/TKL/stations/TKO/next-trains").await;
    let second = app.get("/api/lines/TKL/stations/TKO/next-trains").await;

    first.assert_json(StatusCode::OK);
    second.assert_json(StatusCode::OK);
    assert_eq!(first.body["directions"], second.body["directions"]);
}

#[tokio::test]
async fn invalid_lines_and_stations_are_rejected_without_calling_upstream() {
    let app = TestApp::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&app.upstream)
        .await;

    let cases = [
        ("/api/lines/XYZ/stations/TKO/next-trains", "unknown_line"),
        ("/api/lines/TKL/stations/XYZ/next-trains", "unknown_station"),
        (
            "/api/lines/TKL/stations/TOO-LONG/next-trains",
            "unknown_station",
        ),
        (
            "/api/lines/TKL/stations/ADM/next-trains",
            "station_not_on_line",
        ),
        (
            "/api/lines/LR/stations/TKO/next-trains",
            "station_not_on_line",
        ),
        ("/api/stations/XYZ/next-trains", "unknown_station"),
    ];

    for (uri, code) in cases {
        let response = app.get(uri).await;
        response.assert_json(StatusCode::NOT_FOUND);
        assert_eq!(response.body["error"]["code"], code, "{uri}");
        assert!(response.body["error"]["message"].is_string());
    }
}

#[tokio::test]
async fn upstream_failure_without_cached_data_returns_bad_gateway() {
    let app = TestApp::start().await;
    mount_board(&app, "TKL", "TKO", ResponseTemplate::new(500)).await;

    let response = app.get("/api/lines/TKL/stations/TKO/next-trains").await;

    response.assert_json(StatusCode::BAD_GATEWAY);
    assert_eq!(
        response.body,
        json!({
            "error": {
                "code": "upstream_unavailable",
                "message": "An upstream service is unavailable",
            }
        })
    );
}

#[tokio::test]
async fn special_arrangement_notices_are_bilingual() {
    let app = TestApp::start().await;
    let now = Moment::now();
    let alert = |message: &str| {
        json!({
            "status": 0,
            "message": message,
            "url": "https://example.com/notice",
            "curr_time": now.upstream(),
            "data": {},
        })
    };
    mount_board(&app, "TKL", "TKO", upstream_ok(alert("Special service"))).await;
    Mock::given(method("GET"))
        .and(path(NEXT_TRAIN_PATH))
        .and(query_param("lang", "TC"))
        .respond_with(upstream_ok(alert("特別服務安排")))
        .expect(1)
        .mount(&app.upstream)
        .await;

    let response = app.get("/api/lines/TKL/stations/TKO/next-trains").await;

    response.assert_json(StatusCode::OK);
    assert_eq!(
        response.body["alert"],
        json!({
            "en": { "message": "Special service", "url": "https://example.com/notice" },
            "tc": { "message": "特別服務安排", "url": "https://example.com/notice" },
        })
    );
    let trains: Vec<&Value> = response.body["directions"]
        .as_array()
        .expect("directions should be an array")
        .iter()
        .map(|direction| &direction["trains"])
        .collect();
    assert_eq!(trains, [&json!([]), &json!([])]);
}

#[tokio::test]
async fn station_boards_cover_every_line_and_isolate_failures() {
    let app = TestApp::start().await;
    let now = Moment::now();
    for (line, dest) in [("EAL", "LOW"), ("TWL", "TSW"), ("ISL", "CHW")] {
        let body = schedule_body(
            line,
            "ADM",
            now,
            &[Train {
                dest,
                at: now.plus_seconds(90),
            }],
            &[],
        );
        mount_board(&app, line, "ADM", upstream_ok(body)).await;
    }
    mount_board(&app, "SIL", "ADM", ResponseTemplate::new(503)).await;

    let response = app.get("/api/stations/adm/next-trains").await;

    response.assert_json(StatusCode::OK);
    assert!(response.max_age() <= 10);
    assert_eq!(
        response.body["station"],
        station("ADM", "Admiralty", "金鐘")
    );

    let lines = response.body["lines"]
        .as_array()
        .expect("lines should be an array");
    let codes: Vec<&str> = lines
        .iter()
        .map(|entry| entry["line"]["code"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(codes, ["EAL", "SIL", "TWL", "ISL"]);

    for entry in lines {
        if entry["line"]["code"] == "SIL" {
            assert_eq!(entry["board"], Value::Null);
            assert_eq!(entry["error"]["code"], "upstream_unavailable");
            continue;
        }
        assert_eq!(entry["error"], Value::Null);
        let directions = entry["board"]["directions"]
            .as_array()
            .expect("directions should be an array");
        assert_eq!(directions[0]["direction"], "up");
        assert_eq!(
            directions[0]["trains"][0]["arrival_at"],
            now.plus_seconds(90).api()
        );
        // Admiralty ends the East Rail Line, so it has no down direction.
        let expected = if entry["line"]["code"] == "EAL" { 1 } else { 2 };
        assert_eq!(directions.len(), expected, "{}", entry["line"]["code"]);
    }
}

#[tokio::test]
async fn station_boards_fail_when_every_line_fails() {
    let app = TestApp::start().await;
    mount_board(&app, "TKL", "TKO", ResponseTemplate::new(500)).await;

    let response = app.get("/api/stations/TKO/next-trains").await;

    response.assert_json(StatusCode::BAD_GATEWAY);
    assert_eq!(response.body["error"]["code"], "upstream_unavailable");
    assert_eq!(response.header(header::CACHE_CONTROL), "");
}
