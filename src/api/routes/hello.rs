use axum::{Json, Router, extract::State, routing::get};

use crate::{api::dto::HelloResponse, state::AppState};

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/hello", get(hello))
        .route("/hello.json", get(hello_json))
}

async fn hello(State(state): State<AppState>) -> String {
    state.hello_service().greeting().into_message()
}

async fn hello_json(State(state): State<AppState>) -> Json<HelloResponse> {
    Json(state.hello_service().greeting().into())
}
