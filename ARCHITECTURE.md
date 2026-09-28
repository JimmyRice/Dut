# Architecture

This project is a Cargo workspace with a layered architecture and a single
composition root. The dependency direction is inward: HTTP and external
services depend on the application and domain layers, never the reverse.
Layers live in separate crates, so each crate's `Cargo.toml` enforces the
direction: a crate cannot import what it does not depend on.

## Crates

Crates are split where code is reused or deployed separately, not one per
layer.

| Crate | Layer | Contents | Workspace dependencies |
| --- | --- | --- | --- |
| `dut-core` | domain, application | Domain types, use cases, their ports, `Snapshot` and `Freshness` | none |
| `dut-telemetry` | telemetry | Log output and log-field conventions | none |
| `dut-http` | infrastructure | `OutboundHttpClient` and upstream freshness parsing | telemetry |
| `dut-upstream` | infrastructure | MTR adapters, `RefreshingCache`, the connectivity check | core, http, telemetry |
| `dut-api` | api | Axum routes, DTOs, middleware, `ApiError`, `AppState` | core, telemetry |
| `dut` (root package) | bootstrap | Configuration, wiring, process lifecycle | all |

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
  package's `src/`. It holds the configuration and startup errors, creates
  infrastructure and services, assembles `AppState`, builds the application,
  and serves it.
- `telemetry`: Log output and log-field conventions shared by every crate.

## Request flow

```text
HTTP request
  -> dut-api route
  -> dut-core application service
  -> dut-core domain model/rules
  -> dut-upstream adapter when external data is required
  -> dut-api DTO
  -> HTTP response
```

## Workspace map

```text
Cargo.toml        workspace manifest, and the dut binary package
src/              bootstrap/{config, error (StartupError), server}, lib.rs, main.rs
crates/
  dut-core/       domain/{network, next_train, line_status, localized, time},
                  application/{source, next_train, line_status}
  dut-telemetry/  console logging, request_blocks (terminal view, one block per request)
  dut-http/       OutboundHttpClient, freshness
  dut-upstream/   cache, connectivity, mtr/{next_train, line_status}
  dut-api/        routes, dto, error, http_cache, middleware, state
tests/api/        route-level tests of the whole application
tests/fixtures/   captured upstream responses, also read by dut-upstream's unit tests
```

`default-members` lists every crate, so `cargo build`, `cargo clippy`, and
`cargo test` at the root cover the whole workspace; add `-p <crate>` for one.
Dependency versions are pinned once in `[workspace.dependencies]`, and every
crate inherits its version, edition, and lints from the workspace.

## Visibility

Each crate exports only what another crate uses and keeps everything else in
private modules, where the compiler still reports code that nothing uses. It
cannot do so for exported items, so keep exports few. The workspace enables
`unreachable_pub`: `pub` marks exactly what other crates may use, and anything
internal says `pub(crate)`. `dut-core` exports its `domain` and `application`
modules whole, since every other crate builds on them.

The `dut` library exports only what `main` and the route tests use:
`AppConfig`, `build_app`, `run`, and `StartupError`.

## Caching and freshness

Upstream feeds are read through `RefreshingCache` in `dut-upstream`, a
generic in-memory cache with one entry per key. The principle is to **cache
absolute facts and derive relative values at the edge**:

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
  immediately while it refreshes in the background. It is enabled for line
  status and disabled for Next Train, where freshness matters more than the
  roughly 200 ms refresh.
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
layer.

At startup, `dut_upstream::connectivity::ConnectivityCheck` requests one
document from every upstream in the background and logs whether each could be
reached. Each adapter supplies its own probe request, so the check uses the
same URLs and timeouts as real traffic. It only reports: a failed probe does
not stop the server, since the caches already retry and serve stale data.

The cache is in-process and assumes a single instance. Running several
instances multiplies upstream load by the instance count; at that point,
move the cache behind a shared store.

## Adding a feature

1. Add transport-independent entities or value objects under `domain` in
   `dut-core`.
2. Add the use case under `application` in `dut-core`; define a port there
   when the use case requires external data.
3. Implement that port in `dut-upstream`, calling upstream through
   `OutboundHttpClient::fetch` and caching through `RefreshingCache` when the
   data is shared across requests.
4. Register the concrete implementation in `bootstrap` and expose it through
   `AppState`, adding a type parameter for the new source.
5. Add DTOs and handlers in `dut-api`, keeping handlers limited to extraction,
   service invocation, and response mapping.
6. Add route-level integration tests as a module of `tests/api`, which builds
   a single test binary, and unit tests beside non-trivial domain or
   application logic.
7. Document the endpoint in `api.md`, and record the change in its changelog.

Do not introduce repository or service abstractions without a real consumer.
The layer and dependency direction are fixed, while individual abstractions
should be added when their contracts are known.

## Planned crates

The workspace will gain two crates once a background monitor and push
notifications for the MTRGo app are built. This section records the agreed
direction; neither crate exists yet.

| Crate | Contents | Workspace dependencies |
| --- | --- | --- |
| `dut-monitor` | Polling loops, fusion of sources, publishing service alerts | core, telemetry |
| `dut-push` | Push notification policy and the OneSignal client | core, http, telemetry |

- `dut-upstream` gains Hong Kong Observatory and Transport Department
  adapters, and `dut-core` gains the service alert events the monitor
  publishes.
- Event types live in `dut-core`, so `dut-push` does not depend on
  `dut-monitor`. The binary connects the two with a channel.
- `dut-monitor` reads its sources through ports in `dut-core`. The binary
  injects the adapters from `dut-upstream`, and tests inject fakes.
- `dut-push` shares the `OutboundHttpClient` in `dut-http` with the adapters.

Order:

1. Add `dut-monitor`, and let the line status endpoint read the monitor's
   current state.
2. Add `dut-push` when the OneSignal integration starts.

### Monitor

- Each source is polled on its own schedule in its own Tokio task and reports
  to a single fusion task, so a slow source never delays another. Intervals
  use `MissedTickBehavior::Delay`, so a request that timed out is not followed
  by a burst of catch-up requests to a failing upstream.
- It publishes deduplicated state transitions with a severity, such as an
  alert starting, escalating, or ending. It does not publish every poll, and
  it does not filter by notification threshold: thresholds depend on the
  audience and belong to consumers.
- The current state is exposed through a `watch` channel and transitions
  through a `broadcast` channel. A consumer that starts late or lags behind
  resynchronises from the current state.
- The event vocabulary is modelled on GTFS-realtime alerts: cause, effect,
  affected lines and stations, active period, and severity.
- MTR line status and Observatory warnings are structured and may trigger
  alerts. Transport Department special traffic news is free text and is only
  attached as context.
- A failed poll is logged and the loop continues. The time of the last
  successful poll is kept for health reporting.
- The line status endpoint reads the monitor's current state instead of its
  own `RefreshingCache`, so line status is fetched from upstream once rather
  than twice. Next Train keeps request-driven caching.

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
- What was already sent is persisted, so a restart does not repeat a
  notification. This is the first persistent state in the service.
- Only one instance may run push. When the API scales out, the other
  instances run without it.

### Push configuration

Bootstrap reads `DUT_ONESIGNAL_APP_ID` and `DUT_ONESIGNAL_API_KEY` and passes
an `Option<OneSignalConfig>` to `dut-push`; the crate never reads the
environment.

| Variables | Outcome |
| --- | --- |
| Neither set | Push is disabled and this is logged at `info`. The monitor still runs. |
| Both set and non-empty | Push is enabled. |
| Only one set, or either empty | Startup fails. |

- A half-set configuration usually means a mistyped name or a missing secret
  mount. Failing at startup surfaces it before an incident does.
- `AppConfig` is logged at startup, so the API key is held in a type whose
  `Debug` output is redacted, and its header value is marked sensitive.
- Configuration parsing takes a lookup function so that tests supply
  variables from a map. `std::env::set_var` is `unsafe` in edition 2024 and
  races between tests.

### Deployment

The service starts as one binary and one process, with components connected
by in-process channels. Split into several processes only when the API scales
out. Move the event channel to a shared broker such as Redis or NATS only when
a consumer runs in another process, and add serialisable DTOs for events then,
outside `dut-core`.
