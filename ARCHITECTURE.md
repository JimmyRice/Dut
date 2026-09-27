# Architecture

This project uses a layered architecture with a single composition root. The
dependency direction is inward: HTTP and external services depend on the
application and domain layers, never the reverse.

## Layers

- `domain`: Pure business types and rules. It must not import Axum, Reqwest,
  Serde, storage drivers, or transport-specific types.
- `application`: Use cases and orchestration. It may depend on `domain`, but it
  must not contain HTTP handlers or concrete external I/O.
- `infrastructure`: Concrete outbound adapters such as HTTP clients, MTR API
  clients, caches, and repositories.
- `api`: Axum routes, request/response DTOs, validation, request ID and
  tracing middleware, and the mapping from application results to HTTP
  responses.
- `state`: Dependencies shared by handlers. Services and infrastructure are
  constructed once and cloned through Axum state. It sits outside `bootstrap`
  because `api` reads it and `bootstrap` builds it.
- `bootstrap`: The composition root and process lifecycle. It holds the
  configuration and startup errors, creates infrastructure and services,
  assembles `AppState`, builds the application, and serves it.
- `telemetry`: Log output and log-field conventions shared by every layer. It
  depends on no other module in the crate.

## Request flow

```text
HTTP request
  -> api/routes
  -> application service
  -> domain model/rules
  -> infrastructure adapter when external data is required
  -> API DTO
  -> HTTP response
```

## Module map

```text
src/
  domain/          network (lines, stations), next_train, line_status, time
  application/     source (Snapshot, Freshness), next_train, line_status
  infrastructure/  http_client, http_freshness, cache, mtr/{next_train, line_status}
  api/             dto, routes, error, http_cache, middleware
  bootstrap/       config, error (StartupError), server
  telemetry/       console logging, request_blocks (terminal view, one block per request)
  state.rs         AppState
  lib.rs, main.rs  crate root and process entry point
```

The library exports only what `main` and the route tests use: `AppConfig`,
`build_app`, `run`, `StartupError`, and `telemetry::init`. Every layer is a
private module, so the compiler reports code that nothing uses. Keep them
private and export an item only when a caller outside the crate needs it.

## Caching and freshness

Upstream feeds are read through `infrastructure::cache::RefreshingCache`, a
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

The cache is in-process and assumes a single instance. Running several
instances multiplies upstream load by the instance count; at that point,
move the cache behind a shared store.

## Adding a feature

1. Add transport-independent entities or value objects under `domain`.
2. Add the use case under `application`; define an application-owned port when
   the use case requires external data.
3. Implement that port under `infrastructure`, calling upstream through
   `OutboundHttpClient::fetch` and caching through `RefreshingCache` when the
   data is shared across requests.
4. Register the concrete implementation in `bootstrap` and expose it through
   `AppState`.
5. Add DTOs and handlers under `api`, keeping handlers limited to extraction,
   service invocation, and response mapping.
6. Add route-level integration tests as a module of `tests/api`, which builds
   a single test binary, and unit tests beside non-trivial domain or
   application logic.
7. Document the endpoint in `api.md`, and record the change in its changelog.

Do not introduce repository or service abstractions without a real consumer.
The layer and dependency direction are fixed, while individual abstractions
should be added when their contracts are known.
