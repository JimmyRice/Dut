# Architecture

This project is a Cargo workspace with a layered architecture and a single
composition root. The dependency direction is inward: HTTP and external
services depend on the application and domain layers, never the reverse.
Layers live in separate crates, so each crate's `Cargo.toml` enforces the
direction: a crate cannot import what it does not depend on.

This document explains how the pieces fit. [RUST_API.md](RUST_API.md) is the
reference for the types, services, and traits used to write business logic,
and [HTTP_API.md](HTTP_API.md) documents the HTTP endpoints.

## Stateless first

Dut runs as a stateless service. This takes precedence over any convenience
that local state would buy; a design that needs state must say so here and
justify it.

- **Memory only.** Caches, polled values, open data, and the monitor's
  baselines live in the process. A new process rebuilds them from upstreams
  at startup. Nothing is written to local disk and no volume is mounted.
  The one exception is a log file, which an operator may ask for with
  `--log-file`: it is output that the process never reads back, so a
  replacement process loses nothing without it.
- **Disposable instances.** Any instance can be stopped, replaced, or
  duplicated without a migration or a handover. The cost is accepted: a
  process that starts while an upstream is down serves `502` for that
  upstream's data until it recovers, rather than data a previous process
  saved.
- **State that must outlive a process goes outside it**, and only when a
  feature cannot work without it. Prefer, in order: a design that needs no
  memory across restarts; a mechanism the external service already offers,
  such as an idempotency key; a shared store such as Redis. Never local
  disk.

### Open conflicts

Plans written before this principle that contradict it. Each is resolved in
favour of statelessness when its feature is built.

| Plan | Conflict | Direction |
| --- | --- | --- |
| `dut-push` persists what it already sent, so a restart does not repeat a notification ("Planned: push notifications") | It would be the first local state | Not adopted. A restart already replays nothing, since the monitor's first poll is only a baseline; a restart only loses cooldowns and deduplication windows held in memory. Accept that loss, or send event-derived idempotency keys so the provider drops repeats, before considering a shared store |
| Exactly one instance runs the monitor and push ("Deployment") | Replacing that instance can briefly run two, or none, and two would send every notification twice | Choose the instance by configuration, not by coordination state, and rely on the same event-derived idempotency keys to drop duplicates during the overlap |

## Crates

Crates are split where code is reused or deployed separately, not one per
layer.

| Crate | Layer | Contents | Workspace dependencies |
| --- | --- | --- | --- |
| `dut-core` | domain, application | Domain types and their diffs, monitor events, use cases, their ports (including `Feed` and `Subscriber`), `Snapshot` and `Freshness` | none |
| `dut-telemetry` | telemetry | Log output and log-field conventions | none |
| `dut-http` | infrastructure | `OutboundHttpClient` and upstream freshness parsing | telemetry |
| `dut-upstream` | infrastructure | MTR and Observatory adapters, MTR open data cleaning, `RefreshingCache`, the connectivity check | core, http, telemetry |
| `dut-poll` | background | Polls `Feed`s on a schedule, keeps their latest value and source health | core, telemetry |
| `dut-monitor` | background | Watches polled feeds and publishes each change as a `MonitorEvent` | core, poll |
| `dut-api` | api | Axum routes, DTOs, middleware, `ApiError`, `AppState` | core, telemetry |
| `dut` (root package) | bootstrap | Command line, configuration, wiring, process lifecycle | all |

`dut-poll` and `dut-monitor` are split at a deployment boundary. Every API
instance polls, because the line status endpoint serves the polled value;
only one instance should run the monitor, since each would publish the same
events.

- `domain`: Pure business types and rules. `dut-core` has no Axum, Reqwest,
  Serde, or Tokio dependency, so this is a compile error rather than a
  convention.
- `application`: Use cases and orchestration, and the ports through which
  they read external data. It may depend on `domain`, but it must not contain
  HTTP handlers or concrete external I/O.
- `infrastructure`: Concrete outbound adapters. Shared helpers are grouped by
  what they depend on rather than collected in a utilities crate: every
  adapter calls upstream through the one `OutboundHttpClient` in `dut-http`,
  and `RefreshingCache` stays in `dut-upstream` until a second crate needs it.
- `api`: Axum routes, request/response DTOs, validation, request ID and
  tracing middleware, and the mapping from application results to HTTP
  responses. `AppState` holds the services shared by handlers. Its data
  sources are type parameters, so `dut-api` never depends on an adapter.
- `bootstrap`: The composition root and process lifecycle, in the root
  package's `src/`. It reads the command line, holds the configuration and
  startup errors, creates infrastructure and services, assembles `AppState`,
  builds the application, and serves it.
- `background`: Work that runs without a request. `dut-poll` knows nothing
  of what a feed contains; `dut-monitor` interprets changes through the diffs
  that `dut-core` defines on its domain types.
- `telemetry`: Log output and log-field conventions shared by every crate.

## Request flow

```text
HTTP request
  -> dut-api route
  -> dut-core application service
  -> dut-core domain model/rules
  -> dut-upstream adapter when external data is required,
     or the latest polled value from dut-poll
  -> dut-api DTO
  -> HTTP response
```

## Background flow

```text
dut-poll poller, one task per feed, on its own schedule
  -> dut-upstream feed adapter (Feed port)
  -> latest value and source health, in a watch channel
       -> dut-api line status route, through the LineStatusSource port
       -> dut-monitor watcher
            -> dut-core diff of the previous and current value
            -> MonitorEvent, logged and broadcast to subscribers
```

## Workspace map

```text
Cargo.toml        workspace manifest, and the dut binary package
src/              bootstrap/{app (wiring), command_line (CommandLine), config,
                             error (StartupError), server}, lib.rs, main.rs
crates/
  dut-core/       domain/{network, next_train, line_status, weather, source_health,
                         event, localized, time, reference},
                  application/{source, feed, subscriber, next_train, line_status,
                               reference_data}
  dut-telemetry/  output (init, LogConfig), filter (LogFilter), fields (log-field conventions),
                  request_blocks (terminal view, one block per request)
  dut-http/       client (OutboundHttpClient), freshness
  dut-upstream/   cache, connectivity, mtr/{next_train, line_status, open_data}, hko/warnings
  dut-poll/       poller (spawn), schedule, health, state, handle (FeedHandle),
                  line_status, reference_data (the ports, served from polled feeds)
  dut-monitor/    monitor (spawn, MonitorHandle), watcher, delivery (runs a Subscriber),
                  signals, event_log
  dut-api/        router, routes, dto, error, http_cache, middleware, state
tests/api/        route-level tests of the whole application
tests/fixtures/   captured upstream responses, also read by dut-upstream's unit tests
```

Every `lib.rs` and `mod.rs` is an index: module documentation, `mod`
declarations, and `pub use` re-exports. Code lives in files named for what
they do, so a file's name says where to look.

`default-members` lists every crate, so `cargo build`, `cargo clippy`, and
`cargo test` at the root cover the whole workspace; add `-p <crate>` for one.
Dependency versions are pinned once in `[workspace.dependencies]`, and every
crate inherits its version, edition, and lints from the workspace. Release
archives and the container image are built with `[profile.dist]`, whose fat
LTO optimises every crate and dependency as one program, so splitting the
workspace costs no cross-crate inlining in what ships. The same profile
aborts on panic and optimises the outbound HTTPS client for size, since it
runs once per poll rather than once per request, and the command line
parser, which runs once at startup.

## Visibility

Each crate exports only what another crate uses and keeps everything else in
private modules, where the compiler still reports code that nothing uses. It
cannot do so for exported items, so keep exports few. The workspace enables
`unreachable_pub`: `pub` marks exactly what other crates may use, and anything
internal says `pub(crate)`. `dut-core` exports its `domain` and `application`
modules whole, since every other crate builds on them.

The `dut` library exports only what `main` and the route tests use:
`AppConfig`, `build_app`, `CommandLine`, `run`, and `StartupError`.

## Configuration

What the service serves, such as upstream endpoints, timeouts, cache
policies, and poll schedules, is compiled into `AppConfig`'s defaults and
changes with a release. What may differ between two runs of the same build
is an option, which `CommandLine` in `src/bootstrap/command_line.rs` reads
with clap: where the process listens, how it logs, and, as features need
them, credentials and switches.

- **Flag and variable.** Every option is a long flag with an environment
  variable behind it, and the flag wins: containers configure the process
  through its environment, people at a terminal through flags. `RUST_LOG`
  keeps its conventional name; every other variable starts with `DUT_`.
  Deployments depend on these names, so a test pins them.
- **Groups.** Options are grouped by concern, one `clap::Args` struct per
  group under its own `--help` heading. A new concern adds a struct and
  flattens it into `CommandLine`. The doc comments on its fields are the
  `--help` text. Crates never read the command line or the environment;
  bootstrap hands them plain values such as `dut_telemetry::LogConfig`.
- **Parsed at the boundary.** Each option has a type that validates it, such
  as `SocketAddr` or `LogFilter`, so a malformed or empty value stops the
  process before anything starts, with a usage error and exit status 2.
  Rules between options, such as two that must be set together, are
  declared on the arguments with clap's `requires` and `conflicts_with`.
- **Secrets** are passed through their environment variable rather than
  their flag, since other users of a machine can read a process's
  arguments, and their option sets `hide_env_values` so that `--help` does
  not print them.
- **Tests** parse argument lists with every option's environment variable
  removed, so the shell running them cannot change their outcome, and no
  test calls `std::env::set_var`, which is `unsafe` in edition 2024 and
  races between tests.
- **Cost.** Parsing happens once, before logging starts, and nothing on a
  request's path reads options. clap is built without coloured help and
  optimised for size in the `dist` profile, where it adds about 180 KB,
  3.5%, to the binary.

## Caching and freshness

Upstream data reaches the API in one of two ways. Next Train boards are read
per request through `RefreshingCache` in `dut-upstream`, a generic in-memory
cache with one entry per key. Whole-document feeds, such as line status,
weather warnings, and MTR open data, are polled on a schedule by `dut-poll`
(see "Polled feeds"). The principle for both is to **cache absolute facts and derive
relative values at the edge**:

- Next Train arrivals are stored and returned as absolute times
  (`arrival_at`, RFC 3339 with `+08:00`). Upstream's `ttnt` countdown is
  discarded because it goes stale while cached. Clients compute countdowns
  locally.
- On every response, trains whose time is more than 30 seconds in the past
  (`DEPARTED_GRACE`) are filtered out, so a cached board only shows trains that
  can still be caught.

Each cache applies a `CachePolicy`:

- **Freshness period.** Follows upstream's `Cache-Control: max-age` minus
  `Age`, clamped to `[ttl_floor, ttl_ceiling]`. The Next Train CDN caches for
  10 seconds, so fetching more often than it refreshes gains nothing.
- **Single-flight.** Concurrent readers of an expired key wait for one
  refresh instead of each calling upstream.
- **Stale-while-revalidate.** Within this window, an expired value is served
  immediately while it refreshes in the background. Next Train disables it,
  since freshness matters more there than the roughly 200 ms refresh.
- **Stale-if-error.** When a refresh fails, a value that expired within this
  window is served with `"stale": true` instead of an error.
- **Failure backoff.** After a failed refresh, upstream is left alone for a
  short period so an outage does not make every request wait for a timeout.

Cache keys come only from the static network, which is validated before any
upstream call, so the key space is bounded. There are about 120
(line, station) pairs, which caps Next Train load at roughly 12 upstream
requests per second regardless of traffic. Boards are fetched in English
only; names come from the static network. Traditional Chinese is requested
only to localize a special arrangement notice.

API responses carry `Cache-Control: public, max-age=<remaining freshness>`,
or `no-cache` when stale, so HTTP caches downstream can safely add another
layer. Every response is gzipped for clients that accept it.

### Open data

MTR open data (the station list, fares, Light Rail, and barrier-free
facilities) is seven CSV files that change a few times a year, and MTRGo
keeps a local copy. The design follows from that:

- **One poll, one consistent set.** `MtrOpenDataFeed` reads all seven files
  in one poll, once at startup and then daily, retrying a failure after five
  minutes. It reads them one after another over one kept-alive connection:
  nobody waits on a daily poll, so reading them at once would only cost a
  connection per file. A poll succeeds only if every file downloads and cleans, so the
  datasets always agree with each other and with the files, which are kept
  byte for byte and served at `/api/data/sources/{file}`.
- **Clean at the boundary.** Station IDs become station codes, fares become
  whole cents, and encoding defects are fixed once in the adapter. Rows that
  name something unresolvable are counted and logged; malformed values fail
  the poll, keeping the last good set.
- **Revisions, not timestamps.** Each dataset's `Revision` is a hash of its
  cleaned value, and each file's a hash of its bytes, so a revision changes
  exactly when what a client receives changes. Responses carry it as a weak
  `ETag` (datasets also include the service version, since a release may
  change the JSON), answer `If-None-Match` with `304`, and list it in the
  `/api/data` index so the app checks everything in one request.
- **Encoded once per fetch.** The first request for a dataset or file after
  a fetch serializes and gzips it, at the highest level and on the blocking
  pool, and every later request shares those bytes, so a full download of
  the fare table costs about as much CPU as a `304`. A dataset's body also
  carries `fetched_at` and `stale`, so its encodings are keyed by those
  rather than by revision. These bodies bypass `CompressionLayer`, so
  `ContentCoding::negotiate` mirrors the layer's reading of
  `Accept-Encoding`, and a route test holds the two to the same answers.
- **Memory only.** The files and datasets are held by the poller and read
  again from the portal whenever a process starts, per "Stateless first".
  A process that starts during a portal outage answers `502` until the
  portal returns; the 30-day stale window covers only outages that begin
  while a process is running.
- **The compiled network stays authoritative for Next Train.** The station
  list is served as published, and each poll logs where it disagrees with
  the compiled network; `scripts/sync-network.py` turns the published list
  into Rust for review.

At startup, `dut_upstream::connectivity::ConnectivityCheck` requests one
document from every upstream in the background and logs whether each could be
reached. Each adapter supplies its own probe request, so the check uses the
same URLs and timeouts as real traffic. It only reports: a failed probe does
not stop the server, since the caches and pollers already retry and serve
stale data.

The cache and the pollers are in-process and assume a single instance.
Running several instances multiplies upstream load by the instance count; at
that point, move the cache behind a shared store, never onto local disk (see
"Stateless first").

### Polled feeds

`dut_poll::spawn` reads a `Feed` in its own Tokio task on a `Schedule`, so a
slow upstream never delays another. The feed adapter holds no cache; the
poller keeps the last successful value, and a `FeedHandle` reads it.

- **Interval.** Line status is polled every 30 seconds, weather warnings and
  the sampled Next Train boards every 60, and MTR open data once a day. The interval uses
  `MissedTickBehavior::Delay`, so a poll that overruns delays the next one
  rather than being followed by a burst of catch-up requests to a struggling
  upstream.
- **Freshness.** A value is fresh for the interval plus the request timeout,
  so it does not turn stale while the next poll is in flight. The line status
  endpoint's `max-age` is therefore at most 33 seconds, and `"stale": true`
  only appears when polls fail.
- **Stale-if-error.** A failed poll keeps the last value. It is served,
  marked stale, for 15 minutes past its freshness (30 days for open data),
  then the endpoint returns `502`.
- **Retry.** A feed polled rarely retries a failed poll sooner than its
  interval (`retry_after`), so one failure does not leave it without data
  until the next scheduled poll. The real-time feeds are polled often
  enough that they simply wait for the next tick.
- **First poll.** A request that arrives before the first poll finishes waits
  for it, so requests made just after startup do not fail.
- **Source health.** Each poller tracks `starting`, `healthy`, `failing`, and
  `blind` (no success for 2 minutes for line status, 48 hours for open data,
  5 minutes for the others), and logs each transition.

Upstream's `max-age=5` on the line status feed is ignored. The feed is only
rebuilt when a line's status changes, so its `lastBuildDate` is not a
heartbeat.

## Adding a feature

1. Add transport-independent entities or value objects under `domain` in
   `dut-core`.
2. Add the use case under `application` in `dut-core`; define a port there
   when the use case requires external data.
3. Implement that port in `dut-upstream`, calling upstream through
   `OutboundHttpClient::fetch`. Cache through `RefreshingCache` when the data
   is shared across requests and keyed by the request, or implement `Feed`
   and poll it with `dut-poll` when it is one document read in the
   background.
4. Register the concrete implementation in `bootstrap` and expose it through
   `AppState`, adding a type parameter for the new source.
5. Add DTOs and handlers in `dut-api`, keeping handlers limited to extraction,
   service invocation, and response mapping.
6. Add route-level integration tests as a module of `tests/api`, which builds
   a single test binary, and unit tests beside non-trivial domain or
   application logic.
7. Document the endpoint in `HTTP_API.md`, and record the change in its changelog.

Do not introduce repository or service abstractions without a real consumer.
The layer and dependency direction are fixed, while individual abstractions
should be added when their contracts are known.

## Monitor

`dut-monitor` watches the polled feeds and publishes each change as a
`MonitorEvent` from `dut-core`.

- **Facts, not judgements.** Every change is published, including routine
  ones such as a line going from `normal` to `non_service_hours` when service
  ends for the night. Events carry no severity, pass no threshold, and are
  not fused across sources: what matters depends on the audience, so each
  subscriber decides.
- **Events.** Line status (condition or message), weather warnings (issued,
  changed, or cancelled, for every warning the Observatory documents),
  Next Train signals (a delay flag or special arrangement notice at a sampled
  station), and source health.
- **Diffs.** Each domain type defines `changes_since` in `dut-core`, so the
  comparison is unit-tested without tasks or channels. One watcher task per
  feed compares the previous and current successful values after each poll.
- **Baseline.** A feed's first successful poll publishes nothing, so a restart
  never replays the current state as news.
- **Outages.** A failed poll changes no data, so an outage appears only as a
  source health event, never as service resuming or a warning cancelled.
- **Delivery.** Business logic implements the `Subscriber` trait from
  `dut-core` and is attached in bootstrap with `MonitorHandle::attach`. Each
  subscriber runs in its own task and receives events one at a time, in
  order, over a `broadcast` channel, so a slow subscriber delays only itself
  and keeps its state in `&mut self` without locks. Events are not replayed:
  a subscriber that falls more than 256 events behind is told how many it
  missed through `on_lagged` and should resynchronise from the feeds' latest
  values. The event log is a subscriber too, attached by `dut_monitor::spawn`.
  See [RUST_API.md](RUST_API.md) for how to write one.
- **Sampled boards.** One mid-line station per line is read every minute
  through the same cached `NextTrainService` that serves riders, so it adds
  at most ten upstream requests a minute. Whether a board's delay flag leads
  the line status feed is not known yet; these events exist to find out.
- **Transport Department** special traffic news is deferred. It is free text,
  and it needs classifying, possibly by a small language model, before it can
  be published as events.

## Planned: push notifications

`dut-push` will turn monitor events into push notifications for the MTRGo
app. This section records the agreed direction; the crate does not exist yet.

| Crate | Contents | Workspace dependencies |
| --- | --- | --- |
| `dut-push` | Push notification policy and the OneSignal client | core, http, telemetry |

- Event types and the `Subscriber` trait live in `dut-core`, so `dut-push`
  does not depend on `dut-monitor`. The binary attaches it with
  `MonitorHandle::attach`.
- `dut-push` shares the `OutboundHttpClient` in `dut-http` with the adapters.
- Severity, thresholds, and correlating sources (such as a No. 8 signal and
  the MTR's typhoon status) belong to its policy, not to the monitor.

### Push notifications

- The scope is push to the MTRGo app through a push provider, currently
  OneSignal. Chat channels such as Telegram are out of scope; a second
  implementation would be another push provider such as APNs or FCM.
- The policy is a pure function from an event and the record of what was
  already sent to a list of `Notification`s, so thresholds, cooldowns, and
  deduplication are unit-tested without a sender.
- `Notification` uses domain terms only: an audience such as one line or
  everyone, a `Localized` title and body, and collapse and idempotency keys
  derived from the event. Only the `onesignal` module translates these into
  OneSignal filters and fields.
- There is no channel trait until a second provider exists. It is extracted
  then from the two implementations.
- Audiences are targeted through OneSignal tags that the app sets, named after
  this API's line codes, so the service stores no user data. Tag names ship in
  installed apps and are hard to change, so agree them with the app before
  release.
- The record of what was already sent lives in memory, per "Stateless
  first". An earlier plan persisted it so a restart would not repeat a
  notification; that is not adopted (see "Open conflicts"). The monitor's
  baseline already keeps a restart from replaying current state, so a
  restart only forgets cooldowns and deduplication windows. Every
  `Notification` carries an idempotency key derived from its event, to let
  the provider drop repeats; confirm OneSignal's semantics and retention for
  it before relying on it.
- Only one instance may run push. When the API scales out, the other
  instances run without it. A replacement can briefly overlap with the
  instance it replaces; the idempotency keys cover that too.

### Push configuration

`CommandLine` reads `--onesignal-app-id` and `--onesignal-api-key`, set in
production through `DUT_ONESIGNAL_APP_ID` and `DUT_ONESIGNAL_API_KEY` (see
"Configuration"), and bootstrap passes an `Option<OneSignalConfig>` to
`dut-push`; the crate never reads the command line or the environment.

| Options | Outcome |
| --- | --- |
| Neither set | Push is disabled and this is logged at `info`. The monitor still runs. |
| Both set and non-empty | Push is enabled. |
| Only one set, or either empty | Startup fails. |

- A half-set configuration usually means a mistyped name or a missing secret
  mount. Failing at startup surfaces it before an incident does.
- The two options require each other, so clap enforces the table, and an
  empty value fails to parse like any other option's.
- `AppConfig` is logged at startup and `CommandLine` derives `Debug`, so the
  API key is held in a type whose `Debug` output is redacted, and its header
  value is marked sensitive.

### Deployment

The service starts as one binary and one process, with components connected
by in-process channels. Split into several processes only when the API scales
out: every instance then polls, while the monitor and push run in exactly
one, chosen by configuration rather than by leader election, which would
need shared state. Move the event channel to a shared broker such as Redis or NATS only
when a consumer runs in another process, and add serialisable DTOs for events
then, outside `dut-core`.
