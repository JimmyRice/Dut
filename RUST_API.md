# Rust API

This is the reference for writing business logic inside Dut: the domain
types to work with, the services to call, and the traits to implement. It is
written for people and coding agents alike, so every item names the crate and
module it lives in.

- [ARCHITECTURE.md](ARCHITECTURE.md) explains how the layers fit and why.
- [AGENTS.md](AGENTS.md) lists the rules every change must follow.
- [HTTP_API.md](HTTP_API.md) documents the HTTP endpoints for app clients.

When a public trait, handle, or entry point described here changes, update
this file in the same change.

The snippets in this file are not compiled. The doc comments on
`Subscriber`, `Feed`, `dut_poll::spawn`, `Line::towards`, `StationCode`,
`Fare`, and `HONG_KONG` carry examples that `cargo test` compiles and runs,
and `cargo doc --open` shows. When the two disagree, the doc comment is
right and this file needs updating.

## Where to start

| I want to… | Use | Crate |
| --- | --- | --- |
| React to a change (a line disrupted, a warning issued, service ending) | Implement [`Subscriber`](#subscriber), attach with `MonitorHandle::attach` | `dut-core`, `dut-monitor` |
| Read the current line status, weather warnings, or another polled feed | [`FeedHandle::snapshot`](#feedhandle) | `dut-poll` |
| Look up Next Train boards | [`NextTrainService`](#nexttrainservice) | `dut-core` |
| Read MTR open data: stations, fares, Light Rail, barrier-free facilities | [`ReferenceDataService`](#referencedataservice) | `dut-core` |
| Read a new upstream document in the background | Implement [`Feed`](#feed), start it with `dut_poll::spawn` | `dut-core`, `dut-upstream`, `dut-poll` |
| Call an upstream over HTTP | [`OutboundHttpClient::fetch`](#outbound-http) | `dut-http` |
| Work with lines, stations, directions, bilingual names | [Domain vocabulary](#domain-vocabulary) | `dut-core` |
| Let an operator set something at startup, such as a credential | Add an option to a group in `CommandLine`, see ARCHITECTURE.md "Configuration" | `dut` (bootstrap) |

Everything is constructed once, in `src/bootstrap/app.rs`, and nowhere else.
Business logic receives what it needs from there.

## Traits you implement

| Trait | Implement it to | Existing implementations | Wired in |
| --- | --- | --- | --- |
| [`Subscriber`](#subscriber) | React to monitor events | `EventLog` (dut-monitor) | `monitor.attach(..)` in bootstrap |
| [`Feed`](#feed) | Read a whole upstream document on a schedule | `MtrLineStatusFeed`, `HkoWarningFeed`, `NextTrainSignalFeed` | `dut_poll::spawn(..)` in bootstrap |
| `NextTrainSource` | Supply Next Train boards to `NextTrainService` | `MtrNextTrainSource` | `NextTrainService::new(..)` |
| `LineStatusSource` | Supply line status to `LineStatusService` | `FeedHandle<NetworkStatus>` | `LineStatusService::new(..)` |
| `ReferenceDataSource` | Supply MTR open data to `ReferenceDataService` | `FeedHandle<ReferenceData>` | `ReferenceDataService::new(..)` |

Async trait methods are declared as
`fn name(..) -> impl Future<Output = ..> + Send`, so callers can rely on
`Send` futures. Implementations may still write `async fn name(..)`; the
compiler checks that the future is `Send`.

### Subscriber

`dut_core::application::subscriber::Subscriber`

```rust
pub trait Subscriber: Send + 'static {
    const NAME: &'static str;
    fn on_event(&mut self, event: &MonitorEvent) -> impl Future<Output = ()> + Send;
    fn on_lagged(&mut self, missed: u64) -> impl Future<Output = ()> + Send; // default: does nothing
}
```

- Each attached subscriber runs in **its own Tokio task** and receives every
  event **in the order it was published**, one at a time. A slow subscriber
  delays only itself.
- State lives in `&mut self`. No `Mutex` is needed, and a `std::sync` lock
  must never be held across `.await`.
- `NAME` appears as the `subscriber` field of every log line from its task.
- Events are **not replayed**. A subscriber attached late misses what came
  before, so attach every subscriber in bootstrap, right after
  `dut_monitor::spawn`.
- A subscriber that falls more than 256 events behind loses the oldest ones.
  `on_lagged(missed)` is then called before delivery resumes; override it to
  resynchronise from the feeds' latest values if a missed change matters.
- The trait lives in `dut-core`, so an implementation depends on `dut-core`
  only, never on `dut-monitor`.

Example: notice when a line ends service for the night.

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

Wire it in `src/bootstrap/app.rs`:

```rust
let monitor = dut_monitor::spawn(&line_status, &weather_warnings, &next_train_signals);
monitor.attach(ServiceEnds::default());
```

Test the decision logic by building an event by hand:

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

Keep the decision (what to do about an event) in pure functions like
`service_ended`, and the side effects (sending, storing) in `on_event`. The
planned push notifications follow the same split.

### Feed

`dut_core::application::feed::Feed`

```rust
pub trait Feed: Send + Sync + 'static {
    type Item: Send + Sync + 'static;
    const SOURCE: SourceId;
    fn fetch(&self) -> impl Future<Output = Result<Self::Item, SourceUnavailable>> + Send;
}
```

- `fetch` reads the **whole document afresh** every call. The feed holds no
  cache: `dut-poll` decides how often to call it and keeps the latest value.
- `SOURCE` names the feed in logs and in `SourceHealth` events. A new feed
  adds a variant to `SourceId` in `dut_core::domain::source_health`.
- Return domain types, not wire DTOs. Decode into a DTO inside the adapter
  and convert explicitly.
- Map adapter errors with `SourceUnavailable::new(error)`. The cause is
  logged, never shown to API clients.

Sketch of a new adapter in `dut-upstream`, following `hko/warnings/feed.rs`:

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

Wire it in bootstrap with a `Schedule` constant from `src/bootstrap/config.rs`:

```rust
let example = dut_poll::spawn(example_feed, polling.example);
```

To also publish its changes as events:

1. Add `ExampleDocument::changes_since(&self, previous: &Self) -> Vec<..>` in
   `dut-core`, with unit tests.
2. Add a `Change` variant in `dut_core::domain::event`.
3. Pass the new `FeedHandle` to `dut_monitor::spawn` and add one
   `spawn_watch` call for it in `dut-monitor/src/monitor.rs`.
4. Add a log arm in `dut-monitor/src/event_log.rs`.

## Reading data

### Snapshot and freshness

`dut_core::application::source`

| Item | Meaning |
| --- | --- |
| `Snapshot<T>` | A shared (`Arc`) value with `value()`, `fetched_at()` (when this service received it), and `freshness()` |
| `Freshness::Fresh { expires_in }` | Current; may be reused for `expires_in` |
| `Freshness::Stale` | Past its freshness, served because upstream failed. `is_stale()` checks it; `combine` takes the stalest of two |
| `SourceUnavailable` | No usable data, fresh or stale. Its cause is for logs only |

### FeedHandle

`dut_poll::FeedHandle<T>`, returned by `dut_poll::spawn`. Cloning is cheap.

| Method | Use it to |
| --- | --- |
| `snapshot().await -> Result<Snapshot<T>, SourceUnavailable>` | Read the current value with its freshness. Waits for the first poll if it has not finished |
| `subscribe() -> watch::Receiver<FeedState<T>>` | Be woken after every poll, successful or not. Keeps only the latest state, not history |
| `source() -> SourceId` | Name the feed |

`FeedState<T>` exposes `latest() -> Option<&Polled<T>>`, `health()`, and
`attempts()`. `Polled<T>` exposes `value() -> &Arc<T>` and `fetched_at()`.
Borrow a `watch` value only briefly: a poll cannot publish while a borrow is
held.

`FeedHandle<NetworkStatus>` implements `LineStatusSource`, which is how the
line status endpoint serves the polled value. `FeedHandle<ReferenceData>`
implements `ReferenceDataSource` the same way for the open data endpoints.

`dut_poll::Schedule` sets `interval`, `first_poll_after`, `retry_after`,
`fresh_for`, `stale_if_error`, and `blind_after`; see "Polled feeds" in
ARCHITECTURE.md. A `retry_after` shorter than `interval` polls again that soon
after a failure; set it equal to `interval` to keep the regular schedule.

### ReferenceDataService

`dut_core::application::reference_data::ReferenceDataService<S>`, cheap to
clone.

```rust
let snapshot = service.reference_data().await?; // Snapshot<ReferenceData>
let data = snapshot.value();
let fare = data.fares.value().get(from, to);     // Option<&RailFares>
let csv = data.files.get(SourceFile::LinesFares); // &PublishedFile
```

Every dataset and file in one `ReferenceData` comes from the same poll, so
they agree with each other. See [reference](#reference) for the types.

### NextTrainService

`dut_core::application::next_train::NextTrainService<S>`, cheap to clone.

| Method | Returns |
| --- | --- |
| `board(line, station).await` | `Result<BoardView, NextTrainError>` for one line at one station |
| `station_boards(station).await` | `Result<StationBoards, NextTrainError>` for every line at a station; one failing line does not hide the others |

- The line and station are validated against the static network before any
  upstream call, so a cache key never comes from free-form input.
- `BoardView` hides trains that departed more than 30 seconds ago
  (`DEPARTED_GRACE`): use `board()`, `upcoming(direction)`, and
  `directions()`.
- `NextTrainError` is `UnknownStation`, `StationNotOnLine`, or
  `Unavailable(SourceUnavailable)`.
- Boards are cached per line and station, so asking for a board a rider has
  just requested costs no upstream call.

`LineStatusService::status().await` returns `Snapshot<NetworkStatus>` the
same way.

## Monitor

`dut-monitor`

| Item | Purpose |
| --- | --- |
| `dut_monitor::spawn(&line_status, &weather_warnings, &next_train_signals) -> MonitorHandle` | Starts one watcher task per feed and attaches the event log |
| `MonitorHandle::attach(subscriber)` | Runs a [`Subscriber`](#subscriber) in its own task. The usual way in |
| `MonitorHandle::subscribe() -> broadcast::Receiver<MonitorEvent>` | A raw receiver for a caller that runs its own loop and handles `RecvError::Lagged` and `Closed` itself |
| `NextTrainSignalFeed::new(next_trains)` | The `Feed` that samples one mid-line station per line |

What subscribers can rely on:

- **Facts, not judgements.** Every change is published, routine ones included.
  Severity, thresholds, and filtering are the subscriber's job.
- **Baseline.** A feed's first successful poll publishes no data events, so a
  restart never replays the current state as news. Read `FeedHandle::snapshot`
  for the state at startup.
- **Outages.** A failed poll changes no data. It shows only as a
  `SourceHealth` event, never as service resuming or a warning cancelled.
- **Order.** Events from one feed arrive in the order they were observed.
- **One instance.** Only one process should run the monitor and its
  subscribers.

### Events

`dut_core::domain::event::MonitorEvent { observed_at: Timestamp, change: Change }`

| `Change` variant | Payload | Published when |
| --- | --- | --- |
| `LineStatus(LineStatusChange)` | `previous` and `current` `LineStatus` | A line's condition or message changes, including `Normal` ↔ `NonServiceHours` |
| `WeatherWarning(WarningChange)` | `Issued(ActiveWarning)`, `Changed { previous, current }`, or `Cancelled(ActiveWarning)` | The Observatory issues, changes (level or update time), or cancels a warning |
| `NextTrainSignal(SignalChange)` | `line`, `station`, `previous` and `current` `NextTrainSignal` | A sampled board's delay flag or special arrangement notice changes |
| `SourceHealth(HealthChange)` | `source`, `previous` and `current` `HealthState` | A source turns `Healthy`, `Failing`, or `Blind` |

## Domain vocabulary

All in `dut_core::domain`. Parse raw input once at the boundary, then pass
these types around.

### network

| Item | Notes |
| --- | --- |
| `Line` | `AirportExpress`, `TungChung`, … `LightRail`. `"tkl".parse::<Line>()` is case-insensitive; `code()`, `name()`, `color()`, `stations()`, `termini()`, `towards(station, direction)`, `serves(station)`, `Line::serving(station)`, `Line::with_next_train()`, `Line::ALL` |
| `StationCode` | Three uppercase letters, `Copy`. `"tko".parse::<StationCode>()` at runtime; `StationCode::from_static("TKO")` only in `const` items, where a bad literal fails the build |
| `Station` | `Station::find(code)` returns the known station with its bilingual `name`; `Station::all()` lists them by code |
| `Direction`, `ByDirection<T>` | `Up`/`Down` as the Next Train API defines them per line; `ByDirection::get(direction)` |

A valid `StationCode` is not necessarily a known station: check with
`Station::find` or `Line::serves`.

The `STATIONS` table in `network/station.rs` sits between `GENERATED`
markers: `scripts/sync-network.py --write` regenerates it from MTR open data
through a running service. Line layouts and termini in `network/line.rs` are
maintained by hand.

### reference

`dut_core::domain::reference`: MTR open data, read by `MtrOpenDataFeed`.

| Item | Notes |
| --- | --- |
| `ReferenceData` | One poll's worth: `files: BySourceFile<PublishedFile>` and one `Dataset` per cleaned dataset (`stations`, `fares`, `airport_express_fares`, `light_rail`, `light_rail_fares`, `accessibility`) |
| `Dataset<T>` | `value()`, `revision()`, `updated_at()` (upstream `Last-Modified`). `Dataset::new(value, updated_at)` derives the revision from the value's `Hash` |
| `Revision` | Deterministic FNV-1a hash; `Revision::of(&value)`, `Revision::of_bytes(bytes)`; `Display` is 16 hex digits |
| `SourceFile`, `BySourceFile<T>` | The seven portal files; `file_name()`, `FromStr` from the file name, `SourceFile::ALL`. `BySourceFile::get(file)`, `from_fn`, `map` |
| `PublishedFile` | A file byte for byte: `body() -> &Arc<[u8]>`, `updated_at()`, `revision()` |
| `Fare` | Whole Hong Kong cents; `"4.90".parse::<Fare>()` gives 490, `cents()` |
| `FareTable<K, F>`, `Trip<K, F>` | Fares keyed by origin and destination, `StationCode` or `StopId`; `get(from, to)`, `trips()`. Drops same-stop trips |
| `RailFares` | `octopus: OctopusFares` and `single_journey: SingleJourneyFares`, shared by MTR and Light Rail |
| `AirportExpressFares` | `octopus` and `single_journey`, each `AdultAndChildFares` |
| `PublishedNetwork` | Stations and `Route`s (`line`, `direction`, `stations`) as open data lists them; `drift()` compares them with the compiled network, returning `NetworkDrift`s |
| `LightRailNetwork` | `stops: Vec<Stop>` (`StopId`, `StopCode`, name) and `routes: Vec<LightRailRoute>` (`RouteNumber`, stops per direction); `stop(id)`, `routes_serving(id)` |
| `Accessibility` | `categories: Vec<FacilityGroup>` (`FacilityCategory`, name, `Facility`s) and `stations: Vec<StationAccessibility>` (provided `StationFacility`s with an optional `location`) |

`StopCode` and `StationCode` are separate types on purpose: Light Rail stop
codes are assigned independently of station codes.

### Everything else

| Module | Items |
| --- | --- |
| `localized` | `Localized<T> { en, tc }` for anything published in English and Traditional Chinese |
| `time` | `HONG_KONG`, the fixed UTC+8 offset. Times are `jiff::Timestamp`; show them with `display_with_offset(HONG_KONG)` |
| `line_status` | `LineCondition` (`Normal`, `Delayed`, `Disrupted`, `DelayedOrDisrupted`, `NonServiceHours`, `TyphoonSignal`, `Unknown(String)`) with `display_color()`; `LineStatus`; `NetworkStatus::changes_since` |
| `next_train` | `NextTrainBoard` with `signal()`; `TrainArrival` (absolute `arrival_at`, never a countdown); `AlertNotice`; `NextTrainSignal`; `NextTrainSignals::changes_since` |
| `weather` | `WeatherWarning` (every Observatory warning, including `TropicalCyclone(CycloneSignal)`, `PreNo8Announcement`, `Rainstorm(RainstormLevel)`, and `Unrecognised(String)`); `ActiveWarning`; `WeatherWarnings::changes_since` |
| `source_health` | `SourceId` (`MtrLineStatus`, `MtrNextTrain`, `HkoWarnings`, `MtrOpenData`), `HealthState`, `HealthChange` |

An unrecognised upstream value is kept verbatim (`Unknown`, `Unrecognised`)
and logged, rather than failing the whole document.

## Outbound HTTP

`dut-http`. The process shares one client; never build a `reqwest::Client`
yourself.

| Item | Purpose |
| --- | --- |
| `OutboundHttpClient::fetch(UpstreamRequest).await` | Sends a `GET`, reads the body, and logs start, finish (status, latency, size, caching headers), or failure. Non-2xx is an error |
| `UpstreamRequest { upstream, url, timeout }` | `upstream` is a short stable log name such as `hko.warnings` |
| `UpstreamResponse::json::<T>()` | Decodes the body, logging an excerpt if it is not the expected JSON |
| `UpstreamResponse::ttl_hint()` | Upstream's remaining freshness from `Cache-Control` and `Age` |
| `UpstreamResponse::last_modified()` | When upstream says the document last changed, from `Last-Modified` |
| `UpstreamResponse::body()` | The raw body, for documents that are not JSON, such as CSV |
| `UpstreamError` | `Transport` or `Status`; wrap it in the adapter's own error type |

Bootstrap builds the client with `dut_http::build(user_agent, timeout, proxy)`
and hands clones to adapters.

## Upstream adapters

`dut-upstream`

| Item | Kind |
| --- | --- |
| `mtr::next_train::MtrNextTrainSource::new(http, endpoint, timeout, CachePolicy)` | `NextTrainSource`, cached per line and station |
| `mtr::line_status::MtrLineStatusFeed::new(http, endpoint, timeout)` | `Feed<Item = NetworkStatus>` |
| `hko::warnings::HkoWarningFeed::new(http, endpoint, timeout)` | `Feed<Item = WeatherWarnings>` |
| `mtr::open_data::MtrOpenDataFeed::new(http, &base_url, timeout)?` | `Feed<Item = ReferenceData>`: reads the seven portal files in one poll, one after another over one connection, and cleans them. `base_url` ends in `/` |
| `connectivity::ConnectivityCheck::new(http, probes)` | Probes every upstream once at startup and logs the outcome |
| `connectivity::Probe::json(request)`, `Probe::csv(request)` | A probe and the document a real answer is, so a captive portal's HTML page counts as unreachable |
| `CachePolicy` | Freshness, stale-while-revalidate, stale-if-error, and backoff for request-driven caches |

Every adapter offers `probe()`, the `Probe` the connectivity check sends.
`RefreshingCache` is private to `dut-upstream`.

## Telemetry and conventions

- `dut_telemetry::millis(duration)` formats durations for log fields.
- Log with structured fields (`line = %line`), and record errors as
  `error = &err as &dyn Error` so their source chain is printed.
- Each layer has its own `thiserror` error type. No `unwrap`, `expect`, or
  `panic!` outside tests and `const` evaluation.
- An error's message leaves out its cause, which `#[source]` returns, so a
  logged error or a failed start prints each cause once. Write
  `#[error("failed to bind the HTTP server to {address}")]`, not
  `"…: {source}"`.
- Implement `Clone` and `Debug` by hand on generic wrappers, so they do not
  demand the same of their type parameters.
- Tests are offline and deterministic: paused Tokio time for timing, `wiremock`
  for upstreams, fixtures from real responses in `tests/fixtures`.

See [AGENTS.md](AGENTS.md) for the complete rules and required checks.
