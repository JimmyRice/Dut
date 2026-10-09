# Repository development rules

These rules apply to contributors and coding agents working in this repository.

## Architecture boundaries

- Keep `src/main.rs` limited to runtime startup. Never add routes, business
  logic, configuration parsing, or external calls there.
- Keep `dut-core` (`domain` and `application`) free of Axum, Reqwest, Serde,
  database, filesystem, and network dependencies.
- Put business workflows in `application`, not in route handlers.
- Put all outbound HTTP, MTR API, cache, and persistence code in the
  infrastructure crates: the shared client in `dut-http`, adapters and caches
  in `dut-upstream`.
- Put simulated data for the mock API in `dut-mock`, which performs no I/O.
  A change to a real endpoint's response must keep its mock counterpart's
  response the same shape.
- Put Axum extractors, status codes, headers, and request/response DTOs in
  `dut-api` only.
- Construct concrete dependencies only in `bootstrap` (`src/bootstrap`); share
  them through `AppState`.
- Export from a crate only what another crate uses; keep the rest
  `pub(crate)`.
- Do not expose upstream error details or secrets in API responses. Map errors
  through `ApiError`.

## Change requirements

- New endpoints must include route-level tests covering status, content type,
  and response body.
- Every new or changed endpoint must update `HTTP_API.md` in the same change.
  Cover the URL, HTTP method, path and query parameters, a request example,
  response examples, field descriptions, caching behaviour, and error
  responses, and add an entry to its changelog. Take examples from real
  responses of the running service and keep them valid JSON.
- Every new or changed trait, handle, event, or entry point that business
  logic uses must update `RUST_API.md` in the same change, keeping its
  signatures and examples accurate.
- Non-trivial domain and application behavior must include unit tests.
- Reuse the shared Reqwest client; never create a client per request.
- Keep transport DTOs separate from domain models and implement explicit
  conversions between them.
- Do not create speculative traits or empty layers. Add a port or repository
  only when a concrete use case establishes its contract.

## Code quality standards

This service values reuse, composable traits and structs, readability, and
performance. Hold every change to the highest standard on all four.

### Readability

- Name things for what they mean in the MTR domain, not for how they are
  implemented. Prefer small modules with one clear purpose.
- Keep `lib.rs` and `mod.rs` as indexes: module documentation, `mod`
  declarations, and `pub use` re-exports only. Put code in a file named for
  what it does, such as `client.rs` or `router.rs`.
- Document every public item with `///`, explaining why it exists or behaves as
  it does, not restating the signature.
- Model domain concepts with enums and newtypes (`Line`, `StationCode`)
  instead of raw strings or integers. Parse at the boundary, then trust the
  type.
- Keep functions short and linear. Extract a helper once logic is repeated or
  a block needs a comment to explain what it does.

### Reuse and composition

- Build generic, composable building blocks (`RefreshingCache<K, V>`,
  `Snapshot<T>`, `Localized<T>`, `ByDirection<T>`) instead of duplicating
  logic per data source.
- Introduce a trait only at a real seam: a port with a production
  implementation and a test double. Prefer static dispatch (generics and
  `impl Trait`) over `dyn` unless heterogeneous values are required.
- Declare async methods on public traits as
  `fn name(..) -> impl Future<Output = ..> + Send`, so callers such as Axum
  handlers can rely on `Send` futures.
- Implement `Clone` and `Debug` by hand on generic wrappers when a derive
  would add unnecessary bounds on type parameters.

### Performance

- Never block the async runtime. Do not hold a `std::sync` lock across
  `.await`; keep such critical sections to a few field reads or writes.
- Share immutable data through `Arc` instead of cloning it per request.
  Response DTOs should borrow from cached data.
- Validate every cache key against a bounded set (such as the static network)
  before it reaches a cache. Never key a cache on free-form client input.
- Reuse the shared `OutboundHttpClient`, and prefer zero-allocation parsing and
  lookups on hot paths.

### Errors and logging

- Give each layer its own `thiserror` error type. Do not use `unwrap`,
  `expect`, or `panic!` outside tests and compile-time `const` evaluation.
- Send every outbound request through `OutboundHttpClient::fetch`, which logs
  its start, completion (status, latency, size, caching headers), and failure.
- Log every cache state change: miss, expiry, refresh, coalesced wait,
  stale fallback, background revalidation, and failure backoff.
- Use structured `tracing` fields (`key = %value`) rather than formatting
  values into messages, and record errors as `error = &err as &dyn Error` so
  their source chain is printed.
- Never log secrets, and never return upstream details to API clients.

### Tests

- Keep tests deterministic and offline: use paused Tokio time for timing
  behaviour and `wiremock` for upstream services. Never call real upstream
  APIs from tests.
- Base upstream fixtures on captured real responses, stored under
  `tests/fixtures`.

## Required checks

Run all of these before completing a change:

```bash
cargo fmt -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo deny check advisories bans sources
```

The `cargo-deny` check does not cover licences (see `deny.toml`). The Code quality workflow runs all four commands on every pull request and on every push to `develop`, `staging`, and `master`.

See `ARCHITECTURE.md` for the dependency direction and feature workflow.

## Branches

Merge every change into `develop` first, then promote it to `staging`, then to
`master`. Never merge a feature branch straight into `staging` or `master`.
Each of the three branches must pass the Code quality workflow; only `master`
and `v*` tags build container images and release binaries.
