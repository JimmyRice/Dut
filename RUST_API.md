# Rust API

[English](RUST_API.md) · [繁體粵語](docs/zh-HK/RUST_API.md) · [简体中文](docs/zh-CN/RUST_API.md)

Use this reference when writing business logic in Dut: which domain types to pass around, which services to call, and which traits to implement. Concrete production dependencies are constructed in `src/bootstrap/app.rs`. Update this document and its translations whenever a business-facing trait, handle, event, or entry point changes.

## Contents

- [Where to start](#where-to-start)
- [Application ports](#ports)
- [Reading data](#reading-data)
- [Monitor and events](#monitor)
- [Domain vocabulary](#domain-vocabulary)
- [Outbound HTTP](#outbound-http)
- [Upstream adapters](#upstream-adapters)
- [Simulated data](#simulated-data)
- [Errors, telemetry, and tests](#conventions)

Read [HTTP_API.md](HTTP_API.md) for client contracts, [RUST_API.md](RUST_API.md) for business-facing types, [ARCHITECTURE.md](ARCHITECTURE.md) for design decisions, and [AGENTS.md](AGENTS.md) for contribution rules.

Markdown snippets are reference fragments, not a compiled program. Rustdoc examples on the public types are compiled by `cargo test`; view them with `cargo doc --open`. Keep signatures aligned with source. Adapter sketches that introduce new source IDs require corresponding domain changes before they compile.

<a id="where-to-start"></a>

## Where to start

| Task | Entry point |
| --- | --- |
| React to a change | [`Subscriber`](#subscriber) + `MonitorHandle::attach` |
| Read a polled value | [`FeedHandle::snapshot`](#feedhandle) |
| Read train boards | [`NextTrainService`](#nexttrainservice) |
| Read reference datasets/files | [`ReferenceDataService`](#referencedataservice) |
| Add a background document | [`Feed`](#feed) + `dut_poll::spawn` |
| Fetch over HTTP | [`OutboundHttpClient::fetch`](#outbound-http) |
| Model railway concepts | [`dut_core::domain`](#domain-vocabulary) |
| Add startup options | `CommandLine`; [configuration](ARCHITECTURE.md#configuration) |
| Simulate an incident | [`dut-mock`](#simulated-data) |

<a id="ports"></a>

## Application ports

| Trait | Purpose | Implementations |
| --- | --- | --- |
| `Subscriber` | Consume monitor events | `EventLog` |
| `Feed` | Fetch a whole document without caching | `MtrLineStatusFeed`, `HkoWarningFeed`, `MtrOpenDataFeed`, `NextTrainSignalFeed` |
| `NextTrainSource` | Provide board snapshots | `MtrNextTrainSource`; private mock source |
| `LineStatusSource` | Provide network status snapshots | `FeedHandle<NetworkStatus>` |
| `ReferenceDataSource` | Provide a complete reference snapshot | `FeedHandle<ReferenceData>` |

Public async ports declare `fn … -> impl Future<Output = …> + Send`. Implementations can use `async fn`; the compiler verifies the future is Send. Prefer generic/static dispatch. Add a trait at a real seam with a concrete implementation and a test double, not for a possible future consumer.

<a id="subscriber"></a>

### Subscriber

`dut_core::application::subscriber::Subscriber`

```rust
pub trait Subscriber: Send + 'static {
    const NAME: &'static str;
    fn on_event(&mut self, event: &MonitorEvent) -> impl Future<Output = ()> + Send;
    fn on_lagged(&mut self, missed: u64) -> impl Future<Output = ()> + Send;
}
```

Each subscriber runs in its own Tokio task and processes events one at a time in publication order. State belongs in `&mut self`; never hold a std lock across await. `NAME` labels its log span. Attach during bootstrap because events are not replayed. The broadcast capacity is 256; `on_lagged` defaults to no action, so override it to resynchronise if loss matters.

Example: recognise the transition from normal service to the end of service. Keep the decision pure and the state change in `on_event`.

```rust
use dut_core::{
    application::subscriber::Subscriber,
    domain::{
        event::{Change, MonitorEvent},
        line_status::LineCondition,
        network::Line,
    },
};

/// Remembers which lines have ended service tonight.
#[derive(Debug, Default)]
pub struct ServiceEnds {
    ended: Vec<Line>,
}

impl Subscriber for ServiceEnds {
    const NAME: &'static str = "service_ends";

    async fn on_event(&mut self, event: &MonitorEvent) {
        if let Some(line) = service_ended(event) {
            self.ended.push(line);
        }
    }
}

/// The line that has just ended service, if this event says so. Pure, so it
/// is unit-tested without a task or a channel.
fn service_ended(event: &MonitorEvent) -> Option<Line> {
    match &event.change {
        Change::LineStatus(change)
            if change.previous.condition == LineCondition::Normal
                && change.current.condition == LineCondition::NonServiceHours =>
        {
            Some(change.current.line)
        }
        _ => None,
    }
}
```

Attach in bootstrap, immediately after starting the monitor:

```rust
let monitor = dut_monitor::spawn(&line_status, &weather_warnings, &next_train_signals);
monitor.attach(ServiceEnds::default());
```

A focused test can build the event without running the monitor. In the following fragment, import `LineStatus`, `LineStatusChange`, and `jiff::Timestamp` in addition to the imports above:

```rust
let event = MonitorEvent {
    observed_at: Timestamp::UNIX_EPOCH,
    change: Change::LineStatus(LineStatusChange {
        previous: LineStatus { line: Line::KwunTong, condition: LineCondition::Normal, message: None },
        current: LineStatus { line: Line::KwunTong, condition: LineCondition::NonServiceHours, message: None },
    }),
};
assert_eq!(service_ended(&event), Some(Line::KwunTong));
```

<a id="feed"></a>

### Feed

`dut_core::application::feed::Feed`

```rust
pub trait Feed: Send + Sync + 'static {
    type Item: Send + Sync + 'static;
    const SOURCE: SourceId;
    fn fetch(&self) -> impl Future<Output = Result<Self::Item, SourceUnavailable>> + Send;
}
```

Fetch the whole document afresh on each call; the poller owns timing and the latest value. Decode transport DTOs in `dut-upstream`, convert explicitly into domain types, and wrap adapter errors with `SourceUnavailable::new(error)`. `SOURCE` identifies the feed in logs and health events. A new source needs a `SourceId` variant before its adapter can compile.

Adapter sketch (the `Example*` types and variant are placeholders):

```rust
#[derive(Clone, Debug)]
pub struct ExampleFeed {
    http: OutboundHttpClient,
    endpoint: Url,
    request_timeout: Duration,
}

impl Feed for ExampleFeed {
    type Item = ExampleDocument; // a domain type in dut-core
    const SOURCE: SourceId = SourceId::Example;

    async fn fetch(&self) -> Result<ExampleDocument, SourceUnavailable> {
        self.fetch_document().await.map_err(SourceUnavailable::new)
    }
}
```

```rust
let example = dut_poll::spawn(example_feed, polling.example);
```

To monitor a new document, add and unit-test its `changes_since`, add a `Change` variant, extend `dut_monitor::spawn` and its watcher wiring, and add an event-log arm. Polling alone does not publish domain changes.

<a id="source-ports"></a>

### Source signatures

```rust
pub trait NextTrainSource: Send + Sync + 'static {
    fn board(&self, line: Line, station: StationCode)
        -> impl Future<Output = Result<Snapshot<NextTrainBoard>, SourceUnavailable>> + Send;
}

pub trait LineStatusSource: Send + Sync + 'static {
    fn status(&self)
        -> impl Future<Output = Result<Snapshot<NetworkStatus>, SourceUnavailable>> + Send;
}

pub trait ReferenceDataSource: Send + Sync + 'static {
    fn reference_data(&self)
        -> impl Future<Output = Result<Snapshot<ReferenceData>, SourceUnavailable>> + Send;
}
```

<a id="reading-data"></a>

## Reading data

<a id="snapshot"></a>

### Snapshots and freshness

`dut_core::application::source`

| Item | Behaviour |
| --- | --- |
| `Snapshot<T>` | Shared Arc value: `value()`, `fetched_at()`, `freshness()` |
| `Freshness::Fresh { expires_in }` | Reusable for remaining duration |
| `Freshness::Stale` | Expired fallback; `is_stale()` checks it, `combine` selects the stalest/shortest-lived input |
| `SourceUnavailable` | No usable value; causes are for logs, not clients |

<a id="feedhandle"></a>

### FeedHandle and Schedule

`dut_poll::spawn(feed, schedule) -> FeedHandle<F::Item>` starts a feed in a Tokio runtime. Cloning the handle is cheap.

| Method / field | Contract |
| --- | --- |
| `snapshot().await -> Result<Snapshot<T>, SourceUnavailable>` | Current snapshot; bounded wait while the first poll is pending |
| `subscribe() -> watch::Receiver<FeedState<T>>` | Notified after each attempt; latest state only |
| `source() -> SourceId` | Stable source identity |
| `FeedState<T>` | `latest() -> Option<&Polled<T>>`, `health()`, `attempts()` |
| `Polled<T>` | `value() -> &Arc<T>`, `fetched_at()` |
| `Schedule` | `interval`, `first_poll_after`, `retry_after`, `fresh_for`, `stale_if_error`, `blind_after`: all Duration |

Borrow a watch value briefly; holding it prevents the poller from publishing. `FeedHandle<NetworkStatus>` implements `LineStatusSource`, and `FeedHandle<ReferenceData>` implements `ReferenceDataSource`. `retry_after` below the interval retries sooner after failure; at or above the interval keeps the normal schedule. Defaults are in [polled feeds](ARCHITECTURE.md#polled-feeds).

<a id="nexttrainservice"></a>

### NextTrainService

`dut_core::application::next_train::NextTrainService<S>`

| Call | Result |
| --- | --- |
| `new(source: S) -> Self` | Shares a source through Arc |
| `board(line, station).await` | `Result<BoardView, NextTrainError>` |
| `station_boards(station).await` | `Result<StationBoards, NextTrainError>` |

Validation uses the compiled network before reading a source, bounding cache keys. Station boards fetch concurrently and keep per-line failures; the call fails only if no line succeeds. `BoardView` exposes `board()`, `snapshot()`, `upcoming(direction)`, and `directions()`, filtering departures more than 30 seconds old. `StationBoards` exposes `station()`, `lines()`, and `freshness()`; each `LineBoard` has `line` and `board: Result<BoardView, SourceUnavailable>`. Errors are `UnknownStation(StationCode)`, `StationNotOnLine { line, station }`, or `Unavailable(SourceUnavailable)`.

<a id="referencedataservice"></a>

### ReferenceDataService and LineStatusService

`dut_core::application::reference_data::ReferenceDataService<S>::reference_data().await` returns `Result<Snapshot<ReferenceData>, SourceUnavailable>`. All datasets and source files belong to one successful poll. `dut_core::application::line_status::LineStatusService<S>::status().await` returns `Result<Snapshot<NetworkStatus>, SourceUnavailable>`. Both are created with `new(source)` and are cheap to clone.

```rust
let snapshot = service.reference_data().await?;
let data = snapshot.value();
let fare = data.fares.value().get(from, to);
let csv = data.files.get(SourceFile::LinesFares);
```

<a id="monitor"></a>

## Monitor and events

| Entry point | Contract |
| --- | --- |
| `dut_monitor::spawn(&line_status, &weather_warnings, &next_train_signals) -> MonitorHandle` | One watcher per feed, event logger attached first |
| `MonitorHandle::attach<S: Subscriber>(&self, subscriber: S)` | Spawn serial subscriber delivery in its own task |
| `MonitorHandle::subscribe() -> broadcast::Receiver<MonitorEvent>` | Raw receive loop must handle Lagged and Closed |
| `NextTrainSignalFeed::new(next_trains)` | Sample one mid-line station per supported line |

`dut_core::domain::event::MonitorEvent { observed_at: Timestamp, change: Change }` records facts. The first successful data poll emits no data changes; health transitions may emit. Failed polls preserve data and can only change source health. Per-feed observation order is preserved. Only one deployment role should run event side effects; role selection is not yet a CLI feature.

| Change | Payload / trigger |
| --- | --- |
| `LineStatus(LineStatusChange)` | Previous/current LineStatus; condition or message changes, including end of service |
| `WeatherWarning(WarningChange)` | Issued, Changed { previous, current }, Cancelled; warning level/update time changes |
| `NextTrainSignal(SignalChange)` | Line/station, previous/current signal; delay flag or notice changes |
| `SourceHealth(HealthChange)` | Source, previous/current HealthState, including Starting→Healthy |

<a id="domain-vocabulary"></a>

## Domain vocabulary

All modules below live under `dut_core::domain`. Parse input once at the boundary and pass typed values through business code.

<a id="network"></a>

### network

| Type | Use |
| --- | --- |
| `Line` | Case-insensitive parse, `code()`, `name()`, `color()`, `stations()`, `termini()`, `towards(station, direction)`, `leads(from, to, direction)`, `serves(station)`, `Line::serving(station)`, `Line::with_next_train()`, `Line::ALL`. `leads` requires a shared branch and a destination beyond the origin. |
| `StationCode` | Copy newtype, three uppercase letters. Runtime: `"tko".parse::<StationCode>()`; literals in source (tests, tables): `dut_core::station!("TKO")`, which expands to a `const` block. `StationCode::from_static("TKO")` is the underlying `const fn`. A mistyped literal fails the build. |
| `Station` | `find(code)` validates membership and provides names; `all()` is code-sorted |
| `Direction`, `ByDirection<T>` | MTR Up/Down; `ByDirection::get(direction)` |

A syntactically valid station code can still be unknown. Check `Station::find` or `Line::serves`. `scripts/sync-network.py --write` regenerates `STATIONS` between GENERATED markers; line layouts, branches, and termini are reviewed by hand in `line.rs`.

<a id="reference"></a>

### reference

| Type | Contract |
| --- | --- |
| `ReferenceData` | One successful poll: `files: BySourceFile<PublishedFile>` and `Dataset`s for stations, fares, airport_express_fares, light_rail, light_rail_fares, accessibility |
| `Dataset<T>` | `new(value, updated_at)` hashes the value; `value()`, `revision()`, `updated_at()` |
| `Revision` | Deterministic FNV-1a: `of(&value)`, `of_bytes(bytes)`, Display as 16 hex digits |
| `SourceFile`, `BySourceFile<T>` | Seven bounded source files: `file_name()`, FromStr, ALL; container `get(file)`, `from_fn`, `try_from_fn` |
| `PublishedFile` | Original `body() -> &Arc<[u8]>`, `updated_at()`, `revision()` |
| `Fare` | Whole cents; `"4.90".parse::<Fare>()` is 490, `cents()`, `from_cents()` |
| `FareTable<K, F>`, `Trip<K, F>` | Origin/destination lookup via `get(from, to)`, sorted `trips()`; same-stop trips discarded |
| `RailFares` | `octopus: OctopusFares`, `single_journey: SingleJourneyFares`; shared by heavy rail and Light Rail |
| `AirportExpressFares` | Octopus/single journey each use `AdultAndChildFares` |
| `PublishedNetwork` | Published stations and Routes (line, direction, stations); `station(code)`, `drift() -> Vec<NetworkDrift>` |
| `LightRailNetwork` | Stops (StopId, StopCode, name) and LightRailRoutes (RouteNumber, directional stops); `stop(id)`, `routes_serving(id)` |
| `Accessibility` | FacilityGroups (category, name, facilities) and StationAccessibility entries (provided StationFacility values, optional location) |

`StopCode` and `StationCode` are intentionally separate: Light Rail and heavy rail use independent code systems.

<a id="other-domain-types"></a>

### Other domain types

| Module | Types / behaviour |
| --- | --- |
| `localized` | `Localized<T> { en, tc }` |
| `time` | `HONG_KONG` fixed UTC+8; jiff Timestamp, `display_with_offset(HONG_KONG)` |
| `line_status` | LineCondition: Normal, Delayed, Disrupted, DelayedOrDisrupted, NonServiceHours, TyphoonSignal, Unknown(String); `display_color()`; LineStatus; `NetworkStatus::changes_since` |
| `next_train` | NextTrainBoard `signal()`; TrainArrival absolute times; Platforms is Copy with `one(n)`, `pair(1, 3)`, NONE, `"1/3".parse()`, `as_slice()`; AlertNotice; NextTrainSignal; `NextTrainSignals::changes_since` |
| `weather` | WeatherWarning includes TropicalCyclone(CycloneSignal), PreNo8Announcement, Rainstorm(RainstormLevel), Unrecognised(String), and other HKO warnings; ActiveWarning; `WeatherWarnings::changes_since` |
| `source_health` | SourceId: MtrLineStatus, MtrNextTrain, HkoWarnings, MtrOpenData; HealthState: Starting, Healthy, Failing, Blind; HealthChange |

Unknown upstream values remain in `Unknown`/`Unrecognised` and are logged rather than invalidating a whole document. Unreadable platforms become `Platforms::NONE`; the train stays on the board.

<a id="outbound-http"></a>

## Outbound HTTP

Use `dut-http` and the shared connection pool; never build a client per request. Bootstrap calls `dut_http::build(user_agent, timeout, proxy) -> Result<OutboundHttpClient, reqwest::Error>`. `ProxyMode::System` uses environment/system proxies; `Direct` bypasses them for tests.

| Call / type | Contract |
| --- | --- |
| `fetch(UpstreamRequest).await -> Result<UpstreamResponse, UpstreamError>` | GET, read whole body, log start/finish/failure; non-2xx is an error |
| `UpstreamRequest { upstream, url, timeout }` | Stable log name, URL, per-request timeout |
| `json::<T>() -> Result<T, serde_json::Error>` | Decode with diagnostic excerpt on failure |
| `ttl_hint() -> Option<Duration>` | Remaining upstream max-age minus Age |
| `last_modified() -> Option<Timestamp>` | Parsed Last-Modified |
| `body() -> &Bytes` | Original response bytes |
| `UpstreamError` | Transport or Status; wrap in the adapter’s own error |

<a id="upstream-adapters"></a>

## Upstream adapters

`dut-upstream`; constructor arguments below are call shapes. Pass the shared client and bootstrap’s endpoint/timeouts.

| Constructor / type | Role |
| --- | --- |
| `mtr::next_train::MtrNextTrainSource::new(http, endpoint, timeout, CachePolicy)` | Cached NextTrainSource per line/station |
| `mtr::line_status::MtrLineStatusFeed::new(http, endpoint, timeout)` | `Feed<Item = NetworkStatus>` |
| `hko::warnings::HkoWarningFeed::new(http, endpoint, timeout)` | `Feed<Item = WeatherWarnings>` |
| `mtr::open_data::MtrOpenDataFeed::new(http, &base_url, timeout)?` | Feed<Item = ReferenceData>; seven sequential downloads; URL ends with / |
| `connectivity::ConnectivityCheck::new(http, probes)` | One startup probe per upstream |
| `connectivity::Probe::json(request)`, `Probe::csv(request)` | Validate the expected format, rejecting captive-portal HTML |
| `CachePolicy` | default_ttl, ttl_floor, ttl_ceiling, stale_while_revalidate, stale_if_error, failure_backoff |

Adapters expose `probe()` for bootstrap’s connectivity check. `RefreshingCache` remains private to `dut-upstream`.

<a id="simulated-data"></a>

## Simulated data

`dut-mock` performs no I/O and returns the same domain values as real sources. Mock routes can therefore reuse real DTOs and application services. See [the mock design](ARCHITECTURE.md#mock-api).

| Type | Use |
| --- | --- |
| `Scenario` | BoardScenario/StatusScenario implement ALL, USUAL, name(), description(), random_weight() |
| `ScenarioChoice<S>` | Random or Named(S); case-insensitive parsing, UnknownScenario on failure. `resolve(Option<Seed>) -> (S, Seed)` |
| `Seed` | new(u64), DEFAULT (0), fresh(), value(); named defaults to DEFAULT, random without seed generates a new one |
| `SimulatedNextTrains::new(scenario, seed)` | board(line, station).await and station_boards(station).await use NextTrainService; incidents select the requested line or one serving line |
| `SimulatedLineStatus::new(scenario, seed)` | `status() -> Result<Snapshot<NetworkStatus>, SourceUnavailable>` |
| `SimulatedStatusChanges::new(scenario, seed)` | `batches() -> Result<impl Iterator<Item = Vec<LineStatusChange>>, SourceUnavailable>`: an endless cycle of incident then recovery, each batch the domain diff of two simulated feeds; empty when the scenario changes nothing |

```rust
let (scenario, seed) = "peak"
    .parse::<ScenarioChoice<BoardScenario>>()?
    .resolve(None);
let view = SimulatedNextTrains::new(scenario, seed)
    .board(Line::EastRail, "SHT".parse()?)
    .await?;
```

Add scenarios to BoardScenario or StatusScenario with names, descriptions, and weights; implement their conditions in `board::Conditions::of` or `network_status`, then document them in HTTP_API. Update line timetables/platforms against `tests/fixtures/mtr/next_train_network.json`; tests compare the simulation with that capture.

<a id="conventions"></a>

## Errors, telemetry, and tests

- Each layer owns a `thiserror` error type. Keep the source chain in `#[source]`; do not repeat the cause inside the error message. API clients receive `ApiError` mappings, never upstream details.
- Use structured tracing fields, such as `line = %line` and `error = &err as &dyn Error`; `dut_telemetry::millis(duration)` formats durations. Never log secrets.
- Generic wrappers implement Clone/Debug by hand when derives would require unnecessary bounds. Share immutable values through Arc.
- Do not block the runtime or hold a std lock across await. No unwrap, expect, or panic outside tests and compile-time const evaluation.
- Test non-trivial domain/application rules with unit tests. Timing tests use paused Tokio time; upstream tests use wiremock and real captured fixtures. Route tests assert status, content type, and body.
- Run fmt, Clippy with warnings denied, and cargo test before completion; [AGENTS.md](AGENTS.md) is the complete rule set.
