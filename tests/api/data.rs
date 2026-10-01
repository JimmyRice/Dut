use std::io::Read;

use axum::http::{Method, StatusCode, header};
use flate2::read::GzDecoder;
use serde_json::{Value, json};
use wiremock::MockServer;

use crate::support::{OPEN_DATA_FILES, TestApp, TestResponse, mount_open_data, open_data_fixture};

/// Open data as `OPEN_DATA_LAST_MODIFIED` dates it, in Hong Kong Time.
const UPDATED_AT: &str = "2026-04-03T01:02:50+08:00";

const DATASETS: [&str; 6] = [
    "stations",
    "fares",
    "airport-express-fares",
    "light-rail",
    "light-rail-fares",
    "accessibility",
];

/// The application with every open data file served. The files are read
/// once when the application starts, and not again within a test.
async fn app() -> TestApp {
    let upstream = MockServer::start().await;
    mount_open_data(&upstream, None).await;
    TestApp::serving(upstream).await
}

/// Asserts what every dataset response shares, and returns its body.
async fn dataset(app: &TestApp, name: &str) -> TestResponse {
    let response = app.get(&format!("/api/data/{name}")).await;
    response.assert_json(StatusCode::OK);
    assert!(
        (86_000..=86_430).contains(&response.max_age()),
        "{name} should be cacheable for about a day"
    );
    assert!(response.header(header::ETAG).starts_with("W/\""), "{name}");
    assert_eq!(response.body["updated_at"], UPDATED_AT, "{name}");
    assert_eq!(response.body["stale"], false, "{name}");
    assert!(response.body["fetched_at"].is_string(), "{name}");
    response
}

fn find<'a>(items: &'a Value, key: &str, value: impl Into<Value>) -> &'a Value {
    let value = value.into();
    items
        .as_array()
        .and_then(|items| items.iter().find(|item| item[key] == value))
        .unwrap_or_else(|| panic!("no item with {key} = {value}"))
}

#[tokio::test]
async fn data_index_lists_every_dataset_and_file() {
    let app = app().await;

    let response = app.get("/api/data").await;

    response.assert_json(StatusCode::OK);
    let datasets = response.body["datasets"]
        .as_array()
        .expect("datasets should be an array");
    let names: Vec<_> = datasets.iter().map(|dataset| &dataset["name"]).collect();
    assert_eq!(names, DATASETS);
    assert_eq!(datasets[1]["path"], "/api/data/fares");
    assert_eq!(datasets[1]["updated_at"], UPDATED_AT);

    let sources = response.body["sources"]
        .as_array()
        .expect("sources should be an array");
    assert_eq!(sources.len(), OPEN_DATA_FILES.len());
    let fares = &sources[1];
    assert_eq!(fares["file"], "mtr_lines_fares.csv");
    assert_eq!(fares["path"], "/api/data/sources/mtr_lines_fares.csv");
    assert_eq!(
        fares["bytes"],
        open_data_fixture("mtr_lines_fares.csv").len()
    );
    assert_eq!(fares["updated_at"], UPDATED_AT);
}

#[tokio::test]
async fn every_indexed_revision_is_the_etag_its_path_answers_with() {
    let app = app().await;
    let index = app.get("/api/data").await;

    let entries = index.body["datasets"]
        .as_array()
        .into_iter()
        .chain(index.body["sources"].as_array())
        .flatten();
    for entry in entries {
        let path = entry["path"].as_str().expect("path should be a string");
        let response = app.send(Method::GET, path).await;

        assert_eq!(response.status, StatusCode::OK, "{path}");
        assert_eq!(
            response.header(header::ETAG),
            format!("W/\"{}\"", entry["revision"].as_str().unwrap_or_default()),
            "{path}"
        );
    }
}

#[tokio::test]
async fn stations_are_served_as_published() {
    let app = app().await;

    let response = dataset(&app, "stations").await;

    let stations = &response.body["stations"];
    assert_eq!(stations.as_array().map(Vec::len), Some(97));
    assert_eq!(
        find(stations, "code", "LAK"),
        &json!({
            "code": "LAK",
            "name": { "en": "Lai King", "tc": "荔景" },
            "lines": ["TCL", "TWL"],
        })
    );

    let tseung_kwan_o = find(&response.body["lines"], "code", "TKL");
    assert_eq!(tseung_kwan_o["name"]["en"], "Tseung Kwan O Line");
    assert_eq!(
        tseung_kwan_o["routes"][1],
        json!({
            "direction": "up",
            "from": { "code": "TIK", "name": { "en": "Tiu Keng Leng", "tc": "調景嶺" } },
            "towards": { "code": "LHP", "name": { "en": "LOHAS Park", "tc": "康城" } },
            "stations": ["TIK", "TKO", "LHP"],
        })
    );
}

#[tokio::test]
async fn fares_are_whole_cents_between_station_codes() {
    let app = app().await;

    let response = dataset(&app, "fares").await;

    let fares = response.body["fares"]
        .as_array()
        .expect("fares should be an array");
    assert_eq!(fares.len(), 7 * 6);
    let central_to_admiralty = fares
        .iter()
        .find(|trip| trip["from"] == "CEN" && trip["to"] == "ADM")
        .expect("Central to Admiralty should be listed");
    assert_eq!(
        central_to_admiralty,
        &json!({
            "from": "CEN",
            "to": "ADM",
            "octopus": {
                "adult": 490,
                "student": 320,
                "joyyou_sixty": 200,
                "child": 320,
                "elderly": 200,
                "disability": 200,
            },
            "single_journey": { "adult": 500, "child": 350, "elderly": 350 },
        })
    );
    assert!(
        fares.iter().any(|trip| trip["from"] == "RAC"),
        "Racecourse should be resolved"
    );
}

#[tokio::test]
async fn airport_express_fares_use_shared_station_codes() {
    let app = app().await;

    let response = dataset(&app, "airport-express-fares").await;

    let fares = response.body["fares"]
        .as_array()
        .expect("fares should be an array");
    assert_eq!(fares.len(), 14);
    assert_eq!(
        fares
            .iter()
            .find(|trip| trip["from"] == "KOW" && trip["to"] == "AIR"),
        Some(&json!({
            "from": "KOW",
            "to": "AIR",
            "octopus": { "adult": 10500, "child": 5250 },
            "single_journey": { "adult": 11500, "child": 5750 },
        }))
    );
}

#[tokio::test]
async fn light_rail_lists_stops_and_routes_by_number() {
    let app = app().await;

    let response = dataset(&app, "light-rail").await;

    let ferry_pier = find(&response.body["stops"], "id", 1);
    assert_eq!(ferry_pier["code"], "FEP");
    assert_eq!(ferry_pier["name"]["tc"], "屯門碼頭");
    assert_eq!(
        ferry_pier["routes"],
        json!(["507", "610", "614", "614P", "615", "615P"])
    );

    let routes = &response.body["routes"];
    assert_eq!(routes[0]["route"], "505");
    let route_751 = find(routes, "route", "751");
    let towards_tin_yat = &route_751["directions"][0];
    assert_eq!(towards_tin_yat["towards"]["name"]["en"], "Tin Yat");
    assert_eq!(towards_tin_yat["stops"].as_array().map(Vec::len), Some(23));
}

#[tokio::test]
async fn light_rail_fares_are_keyed_by_stop_number() {
    let app = app().await;

    let response = dataset(&app, "light-rail-fares").await;

    let fares = response.body["fares"]
        .as_array()
        .expect("fares should be an array");
    assert_eq!(fares.len(), 5 * 4);
    assert_eq!(fares[0]["from"], 1);
    assert_eq!(fares[0]["to"], 10);
    assert_eq!(fares[0]["octopus"]["adult"], 510);
    assert_eq!(fares[0]["single_journey"]["adult"], 550);
}

#[tokio::test]
async fn accessibility_lists_the_catalogue_and_what_stations_provide() {
    let app = app().await;

    let response = dataset(&app, "accessibility").await;

    let categories = &response.body["categories"];
    assert_eq!(categories[0]["category"], "station_access");
    assert_eq!(categories[0]["name"]["tc"], "出入口設施");
    let hearing = find(categories, "category", "hearing_impaired");
    assert_eq!(
        find(&hearing["facilities"], "code", "HJ3")["name"]["tc"],
        "閃燈路綫圖"
    );

    let stations = response.body["stations"]
        .as_array()
        .expect("stations should be an array");
    let racecourse = stations
        .iter()
        .find(|station| station["station"]["code"] == "RAC")
        .expect("Racecourse should be resolved");
    assert_eq!(racecourse["station"]["name"]["en"], "Racecourse");
    let located = stations
        .iter()
        .flat_map(|station| station["facilities"].as_array().into_iter().flatten())
        .find(|facility| facility["location"].is_object())
        .expect("some facility should have a location");
    assert!(located["location"]["en"].is_string());
}

#[tokio::test]
async fn source_files_are_served_byte_for_byte() {
    let app = app().await;

    for file in OPEN_DATA_FILES {
        let response = app
            .send(Method::GET, &format!("/api/data/sources/{file}"))
            .await;

        assert_eq!(response.status, StatusCode::OK, "{file}");
        assert_eq!(
            response.header(header::CONTENT_TYPE),
            "text/csv; charset=utf-8"
        );
        assert!(
            response
                .header(header::CACHE_CONTROL)
                .starts_with("public, max-age=")
        );
        assert_eq!(response.body, open_data_fixture(file), "{file}");
    }
}

#[tokio::test]
async fn unknown_source_files_are_not_found() {
    let app = app().await;

    let response = app.get("/api/data/sources/passwords.csv").await;

    response.assert_json(StatusCode::NOT_FOUND);
    assert_eq!(response.body["error"]["code"], "unknown_source");
}

#[tokio::test]
async fn a_matching_etag_is_not_modified() {
    let app = app().await;
    let first = app.get("/api/data/fares").await;
    let etag = first.header(header::ETAG).to_owned();

    let revalidated = app
        .send_with_headers(
            Method::GET,
            "/api/data/fares",
            &[(header::IF_NONE_MATCH, etag.as_str())],
        )
        .await;

    assert_eq!(revalidated.status, StatusCode::NOT_MODIFIED);
    assert!(revalidated.body.is_empty());
    assert_eq!(revalidated.header(header::ETAG), etag);
    assert!(
        revalidated
            .header(header::CACHE_CONTROL)
            .starts_with("public, max-age=")
    );

    let changed = app
        .send_with_headers(
            Method::GET,
            "/api/data/fares",
            &[(header::IF_NONE_MATCH, "W/\"an-older-revision\"")],
        )
        .await;
    assert_eq!(changed.status, StatusCode::OK);
}

#[tokio::test]
async fn responses_are_gzipped_for_clients_that_accept_it() {
    let app = app().await;
    let paths = DATASETS
        .map(|name| format!("/api/data/{name}"))
        .into_iter()
        .chain(OPEN_DATA_FILES.map(|file| format!("/api/data/sources/{file}")));

    for path in paths {
        let plain = app.send(Method::GET, &path).await;
        let gzipped = app
            .send_with_headers(Method::GET, &path, &[(header::ACCEPT_ENCODING, "gzip")])
            .await;

        assert_eq!(gzipped.status, StatusCode::OK, "{path}");
        assert_eq!(gzipped.header(header::CONTENT_ENCODING), "gzip", "{path}");
        assert_eq!(
            gzipped.header(header::CONTENT_TYPE),
            plain.header(header::CONTENT_TYPE),
            "{path}"
        );
        assert_eq!(gzipped.header(header::VARY), "accept-encoding", "{path}");
        assert_eq!(plain.header(header::VARY), "accept-encoding", "{path}");
        assert_eq!(gunzip(&gzipped.body), plain.body, "{path}");
    }

    let plain = app.send(Method::GET, "/api/data/fares").await;
    let gzipped = app
        .send_with_headers(
            Method::GET,
            "/api/data/fares",
            &[(header::ACCEPT_ENCODING, "gzip")],
        )
        .await;
    assert!(gzipped.body.len() < plain.body.len() / 4);
}

/// Open data bodies are compressed ahead of time, bypassing the compression
/// layer that encodes every other response, so they must choose a coding as
/// it does.
#[tokio::test]
async fn open_data_negotiates_compression_as_other_routes_do() {
    let app = app().await;

    for accept in [
        "gzip",
        "x-gzip",
        "identity",
        "br",
        "gzip;q=0",
        "gzip;q=0.5, identity",
        "GZIP;Q=0.9, identity;q=0.8",
        "identity;q=0, gzip",
        "gzip;q=1.5",
        "*",
        "*;q=0, identity",
        "*;q=0",
        "identity;q=0",
    ] {
        let headers = [(header::ACCEPT_ENCODING, accept)];
        let expected = app
            .send_with_headers(Method::GET, "/api/lines", &headers)
            .await;

        for path in ["/api/data/fares", "/api/data/sources/mtr_lines_fares.csv"] {
            let response = app.send_with_headers(Method::GET, path, &headers).await;

            assert_eq!(response.status, expected.status, "{path} for {accept}");
            assert_eq!(
                response.header(header::CONTENT_ENCODING),
                expected.header(header::CONTENT_ENCODING),
                "{path} for {accept}"
            );
        }
    }
}

fn gunzip(body: &[u8]) -> Vec<u8> {
    let mut plain = Vec::new();
    GzDecoder::new(body)
        .read_to_end(&mut plain)
        .expect("the body should be gzip");
    plain
}

#[tokio::test]
async fn one_failing_file_makes_open_data_unavailable() {
    let upstream = MockServer::start().await;
    mount_open_data(&upstream, Some("barrier_free_facilities.csv")).await;
    let app = TestApp::serving(upstream).await;

    for path in [
        "/api/data",
        "/api/data/fares",
        "/api/data/sources/mtr_lines_fares.csv",
    ] {
        let response = app.get(path).await;

        response.assert_json(StatusCode::BAD_GATEWAY);
        assert_eq!(
            response.body["error"]["code"], "upstream_unavailable",
            "{path}"
        );
    }
}
