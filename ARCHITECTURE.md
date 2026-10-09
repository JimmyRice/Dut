# Architecture

[English](ARCHITECTURE.md) · [繁體粵語](docs/zh-HK/ARCHITECTURE.md) · [简体中文](docs/zh-CN/ARCHITECTURE.md)

Dut is a Cargo workspace with one composition root. Dependencies point towards the domain and application: HTTP, adapters, and background tasks use business contracts, while the business code stays independent of transport and I/O. Each crate manifest enforces that boundary.

## Contents

- [Process state is disposable](#stateless-first)
- [Crates and boundaries](#crates)
- [Request and background flows](#flows)
- [Configuration at startup](#configuration)
- [Caching and freshness](#caching-and-freshness)
- [Mock API](#mock-api)
- [Monitor and delivery](#monitor)
- [Adding a feature](#adding-a-feature)
- [Planned: push notifications](#planned-push)
- [Deployment direction](#deployment)

Read [HTTP_API.md](HTTP_API.md) for client contracts, [RUST_API.md](RUST_API.md) for business-facing types, [ARCHITECTURE.md](ARCHITECTURE.md) for design decisions, and [AGENTS.md](AGENTS.md) for contribution rules.

<a id="stateless-first"></a>

## Process state is disposable

Caches, feed snapshots, open data, and monitor baselines live in memory. Restarting rebuilds them from upstreams; there is no database, local persistence, or required volume. An optional log file is output only and is never read back. If a process starts during an upstream outage, affected endpoints return `502` until the first usable fetch.

For future features, first try a design that needs no state across restarts, then use an external service mechanism such as idempotency keys. Add a shared store only when a concrete feature needs it. Local disk is not an option. Earlier ideas about persisting notification history are superseded by this rule; cooldowns and deduplication windows may be lost on restart.

<a id="crates"></a>

## Crates and boundaries

| Crate | Responsibility | Workspace dependencies |
| --- | --- | --- |
| `dut-core` | Pure domain types, diffs, application services, ports, snapshots | None |
| `dut-telemetry` | Log filters, output, structured fields, terminal request blocks | None |
| `dut-http` | Shared outbound HTTP and freshness parsing | telemetry |
| `dut-upstream` | MTR/HKO adapters, CSV cleaning, cache, startup probes | core, http, telemetry |
| `dut-mock` | Pure simulation for the mock API | core |
| `dut-poll` | Scheduled feeds, latest values, source health | core, telemetry |
| `dut-monitor` | Feed watchers and event delivery | core, poll |
| `dut-api` | Axum routes, DTOs, middleware, errors, AppState | core, mock, telemetry |
| `dut` | Configuration, dependency wiring, process lifecycle | All crates |

`dut-core` has no Axum, Reqwest, Serde, Tokio, database, filesystem, or network dependency. Workflows belong in `application`; domain rules belong in `domain`. `dut-api` receives generic sources through `AppState`, so it does not import upstream adapters. Concrete production dependencies are built in `src/bootstrap`; `main.rs` only starts the runtime.

Simulation stays outside the rider-facing core because its timetables are approximations. Polling and monitoring are separate crates so they can eventually run in different deployment roles. Today bootstrap starts both in the same process; selecting an API-only role is a future deployment change, not a current CLI switch.

<a id="flows"></a>

## Request and background flows

```text
HTTP request
  -> dut-api: parse and validate
  -> dut-core: application service and domain rules
  -> dut-upstream cache/adapter OR dut-poll snapshot
  -> dut-api: DTO and HTTP response

Scheduled poll
  -> Feed adapter -> watch channel: latest value + health
  -> API source port reads the snapshot
  -> monitor compares successful values
  -> MonitorEvent -> broadcast -> Subscriber
```

Every `lib.rs` and `mod.rs` is an index: documentation, module declarations, and re-exports only. Implementation files are named for their purpose. Export only what another crate uses; keep the rest private or `pub(crate)`. `unreachable_pub` helps enforce that. `dut-core` exposes its domain and application modules; the root library exposes only startup and route-test entry points.

Root Cargo commands cover all default workspace members. Versions and lints are inherited from the workspace. The `dist` profile uses fat LTO, one codegen unit, stripped symbols, and abort-on-panic; selected startup and HTTPS dependencies optimise for size while request handling keeps its speed-oriented defaults. The `min` profile repeats those settings but optimises every crate for size, for the smallest binary at some cost in speed.

<a id="configuration"></a>

## Configuration at startup

Release defaults define upstream URLs, timeouts, cache policies, and polling schedules. `CommandLine` uses clap to parse per-run options into typed values once, before logging starts. Each option has a long flag and an environment variable; flags win. Variables use `DUT_`, except the conventional `RUST_LOG`.

Group related options in `clap::Args` structs and flatten them into `CommandLine`. Validate formats at parsing, and express relationships with `requires` or `conflicts_with`. Invalid or empty values exit with status 2. Crates receive plain configuration values and never read arguments or the environment themselves. Tests remove option variables from their parsing environment without mutating the process environment.

When credentials are introduced, pass them through environment variables, hide their values in help, redact `Debug`, and mark sensitive headers. Never log a secret. No current option adds push credentials; see the planned section below.

<a id="caching-and-freshness"></a>

## Caching and freshness

Cache absolute facts and derive relative values at the edge. Store `arrival_at` and discard upstream countdowns. `BoardView` filters trains more than 30 seconds past arrival (`DEPARTED_GRACE`) on every read. Immutable values are shared through `Arc`, and DTOs borrow where possible.

Next Train uses the private generic `RefreshingCache<K, V>` in `dut-upstream`. Keys are validated line/station pairs from the compiled network, so client input cannot grow the cache without bounds. Concurrent readers share one refresh. Freshness follows upstream `max-age - Age`, clamped to 2–15 seconds; the default is 10. Stale-while-revalidate is disabled, stale-if-error is 90 seconds after expiry, and failures back off for 5 seconds.

Boards are fetched in English; station names come from the compiled network. A Traditional Chinese request is added only for special-arrangement notices. Every outbound request goes through the shared `OutboundHttpClient::fetch`, with structured start, finish, and failure logs. Cache logs cover misses, expiry, refresh, coalesced waits, stale fallback, background revalidation, and backoff.

<a id="polled-feeds"></a>

### Polled feeds

| Feed | Interval / first poll | Fresh / stale window | Blind after |
| --- | --- | --- | --- |
| Line status | 30 s / 0 s | 33 s / 15 min | 2 min |
| HKO warnings | 60 s / 0 s | 65 s / 15 min | 5 min |
| Sampled train signals | 60 s / 60 s | 63 s / 15 min | 5 min |
| MTR open data | 24 h / 0 s | 86430 s / 30 d | 48 h |

`dut_poll::spawn` runs one Tokio task per feed. `MissedTickBehavior::Delay` avoids catch-up bursts after slow polls. Open data retries after 5 minutes; other feeds use their normal interval. Readers wait for the first poll within the schedule’s startup wait. Failed polls retain the last successful value; once its freshness and stale window are exhausted, snapshots become unavailable. Source health tracks `Starting`, `Healthy`, `Failing`, and `Blind`.

Line status ignores upstream’s five-second TTL: its publication time changes only when status changes and is not a heartbeat. HTTP freshness comes from the poll schedule. Fresh responses use `public, max-age=N`; stale ones use `no-cache`. gzip is negotiated for supported bodies.

<a id="open-data"></a>

### Open data as one snapshot

`MtrOpenDataFeed` reads seven CSV files sequentially over a reusable connection. A poll publishes only when every file downloads and cleans successfully; otherwise the previous complete set remains. The adapter resolves station IDs, parses fares as integer cents, normalises known text defects, logs skipped unresolved rows, and rejects malformed values. Raw files keep their original bytes.

Cleaned values and raw bytes each have a deterministic revision. Dataset ETags also include the service version; raw-file ETags do not. `/api/data` lists revisions, while datasets and files support `If-None-Match`. Their JSON/CSV and maximum-level gzip encodings are built on the blocking pool once per fetched state, then shared. Encoding keys include `fetched_at` and `stale`, since those fields can change without a revision change. Responses include `Content-Length` and `Vary: Accept-Encoding`.

The compiled network remains authoritative for Next Train. Polls report published-network drift, and `scripts/sync-network.py` produces changes for review. Startup probes independently check real adapter URLs and document formats; probe failures report trouble but never stop the listener.

<a id="mock-api"></a>

## Mock API

The opt-in `/api/mock` routes reuse real DTOs and application validation. `dut-mock` implements a pure simulated source, with no polling, cache, or I/O. A scenario and seed define independent deterministic choices; absolute time advances the timetable without retaining request state. Captured platforms and short workings inform the simulation, while headways remain approximations. A request simulates at most 64 trains per direction per line.

`/api/mock/events` streams line status changes as Server-Sent Events. The changes are the domain diff between a simulated calm feed and a simulated incident, so they match what polling the status route would show; the route paces them with a timer, and a connection holds no state beyond its position in the cycle. Real monitor events are not streamed yet.

<a id="monitor"></a>

## Monitor and delivery

The monitor compares successive successful values using domain `changes_since` functions. It publishes every observed change: line condition or message, HKO warning lifecycle, sampled board delay/notice, and source health. Events describe facts; severity, thresholds, cross-source correlation, and notification policy belong to subscribers. Failed polls never imply service recovery or warning cancellation.

The first successful data value establishes a baseline and emits no data-change event. Health transitions may still be published. Bootstrap attaches `Subscriber`s immediately; each runs serially in its own task with mutable state and no lock. The broadcast buffer retains 256 events and does not replay history. A lagging subscriber receives `on_lagged(missed)` and should resynchronise if missed changes matter. The event logger is attached before watchers begin.

Next Train monitoring samples one mid-line station per line every minute through the same cached service used by riders, adding at most ten board fetches a minute before any extra localisation. It is not full-network incident detection. Transport Department free-text traffic news remains deferred pending classification.

<a id="adding-a-feature"></a>

## Adding a feature

1. Add domain concepts and tested rules in `dut-core/domain`.
2. Put the workflow in `application`; add a port only when a concrete use case needs external data.
3. Implement adapters in `dut-upstream`, using the shared HTTP client. Choose a keyed cache for request-driven boards or `Feed` for scheduled whole documents.
4. Wire concrete dependencies in bootstrap and pass services through `AppState`.
5. Add DTOs and thin handlers in `dut-api`; mirror response changes in mock routes.
6. Test domain/application decisions and route status, content type, and body using offline fixtures.
7. Update HTTP contracts and changelog in `HTTP_API.md`, business interfaces in `RUST_API.md`, and all translations. Run the three required checks in [AGENTS.md](AGENTS.md).

<a id="planned-push"></a>

## Planned: push notifications

`dut-push` does not exist yet. The intended crate would depend on core, http, and telemetry, implement `Subscriber`, and translate domain notifications through a provider adapter. OneSignal is the current candidate; APNs or FCM could establish a second-provider seam later. Chat channels are outside this plan.

Keep policy pure and test thresholds, cooldowns, and deduplication independently of sending. Notifications should use domain audiences, `Localized` titles and bodies, and event-derived collapse/idempotency keys. Provider tags would target line-code audiences without storing user data; agree tag names with the app before release. Confirm provider idempotency semantics and retention before relying on them during restarts or overlapping deployments.

Proposed options are `--onesignal-app-id` / `DUT_ONESIGNAL_APP_ID` and `--onesignal-api-key` / `DUT_ONESIGNAL_API_KEY`. Neither would disable push, both non-empty would enable it, and partial or empty configuration would fail startup. These names are design notes and are not accepted by today’s CLI.

<a id="deployment"></a>

## Deployment direction

Today the service is one binary connected by in-process channels. Multiple instances each rebuild caches and poll upstreams, so upstream load scales with instance count and monitors duplicate events. Before scaling out, introduce configuration to run monitor and push in one role, with idempotency for replacement overlap. Move events to Redis or NATS only when a consumer runs in another process; serialisable event DTOs belong outside `dut-core`.
