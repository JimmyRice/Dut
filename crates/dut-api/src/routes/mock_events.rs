//! The mock event stream: line status changes as Server-Sent Events, paced
//! by the client, so an app can build its live view without waiting for the
//! MTR to have a bad day.

use std::{future, time::Duration};

use axum::{
    extract::{Query, rejection::QueryRejection},
    http::{HeaderName, HeaderValue},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
};
use futures::{Stream, StreamExt, stream};
use jiff::Timestamp;
use serde::Serialize;

use dut_core::domain::line_status::LineStatusChange;
use dut_mock::{SimulatedStatusChanges, StatusScenario};

use crate::{
    dto::{HelloEvent, LineStatusEvent, MockEventsQuery, common::HktTime},
    error::ApiError,
    routes::{mock::simulated, params},
};

/// Proxies such as nginx hold a response back until it is large unless told
/// otherwise, which would keep every event from reaching the client.
const NO_PROXY_BUFFERING: (HeaderName, HeaderValue) = (
    HeaderName::from_static("x-accel-buffering"),
    HeaderValue::from_static("no"),
);

/// Idle connections are closed by many proxies; a comment line every so
/// often keeps them open without telling the client anything.
const KEEP_ALIVE: Duration = Duration::from_secs(15);

/// `GET /api/mock/events`
///
/// The scenarios are the line status ones: the incident is reported, service
/// recovers, and the cycle starts again.
pub(super) async fn events(
    query: Result<Query<MockEventsQuery>, QueryRejection>,
) -> Result<Response, ApiError> {
    let Query(query) = query.map_err(|_| ApiError::InvalidQuery)?;
    let (scenario, seed) =
        params::scenario_named::<StatusScenario>(query.scenario.as_deref(), query.seed)?;
    let interval = params::event_interval(query.interval)?;
    let batches = SimulatedStatusChanges::new(scenario, seed)
        .batches()
        .map_err(ApiError::from)?;

    let sse =
        Sse::new(event_stream(batches, interval)).keep_alive(KeepAlive::new().interval(KEEP_ALIVE));
    let response = ([NO_PROXY_BUFFERING], sse).into_response();
    Ok(simulated(scenario, seed, Ok(response)))
}

/// `hello`, then each batch of changes after `interval`, forever. With no
/// batches the stream stays open and silent, as a quiet network would.
fn event_stream(
    batches: impl Iterator<Item = Vec<LineStatusChange>> + Send + 'static,
    interval: Duration,
) -> impl Stream<Item = Result<Event, axum::Error>> + Send {
    let hello = stream::once(future::ready(event(
        "hello",
        &HelloEvent {
            server_time: HktTime(Timestamp::now()),
        },
    )));
    let changes = stream::unfold(batches, move |mut batches| async move {
        let Some(batch) = batches.next() else {
            return future::pending().await;
        };
        tokio::time::sleep(interval).await;
        let observed_at = Timestamp::now();
        let events: Vec<_> = batch
            .iter()
            .map(|change| event("line_status", &LineStatusEvent::new(observed_at, change)))
            .collect();
        Some((stream::iter(events), batches))
    })
    .flatten();
    hello.chain(changes)
}

fn event(name: &str, body: &impl Serialize) -> Result<Event, axum::Error> {
    Event::default().event(name).json_data(body)
}

#[cfg(test)]
mod tests {
    use dut_mock::Seed;

    use super::*;

    const INTERVAL: Duration = Duration::from_secs(5);

    fn changes(scenario: StatusScenario) -> impl Iterator<Item = Vec<LineStatusChange>> + Send {
        SimulatedStatusChanges::new(scenario, Seed::new(1))
            .batches()
            .expect("the scenario should simulate")
    }

    #[tokio::test(start_paused = true)]
    async fn changes_arrive_one_interval_apart_and_cycle_forever() {
        let mut events = Box::pin(event_stream(changes(StatusScenario::Delayed), INTERVAL));
        assert!(events.next().await.is_some(), "hello comes at once");

        for _ in 0..4 {
            let early = tokio::time::timeout(INTERVAL - Duration::from_millis(1), events.next());
            assert!(early.await.is_err(), "a change must wait for its interval");
            assert!(events.next().await.is_some());
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_batch_is_sent_together() {
        let mut events = Box::pin(event_stream(
            changes(StatusScenario::TyphoonSignal),
            INTERVAL,
        ));
        events.next().await;
        events.next().await;

        for _ in 0..10 {
            let same_moment = tokio::time::timeout(Duration::ZERO, events.next()).await;
            assert!(matches!(same_moment, Ok(Some(_))));
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_quiet_scenario_says_hello_and_then_nothing() {
        let mut events = Box::pin(event_stream(changes(StatusScenario::Normal), INTERVAL));
        assert!(events.next().await.is_some());

        let later = tokio::time::timeout(Duration::from_secs(3600), events.next()).await;

        assert!(later.is_err(), "the stream should stay open and silent");
    }
}
