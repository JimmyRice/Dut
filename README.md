# Dut (嘟)

[English](README.md) · [繁體廣東話](docs/zh-HK/README.md) · [简体中文](docs/zh-CN/README.md)

Dut is the Rust backend for MTRGo (Not Yet Released or Open-Sourced). It turns Hong Kong MTR open data into JSON an app can use directly: lines and stations, service status, upcoming trains, fares, Light Rail routes, and accessibility facilities.

## Contents

- [Run locally](#quick-start)
- [HTTP endpoints](#endpoints)
- [Develop with simulated data](#mock-api)
- [Caching and freshness](#freshness)
- [Configuration](#configuration)
- [Containers](#docker)
- [Releases](#releases)
- [Logging and operations](#logging)
- [Development](#development)
- [Data sources](#sources)

- **Useful at the platform.** Directions include the termini shown on platform signs; names come in English and Traditional Chinese. Arrival times are absolute, so the app can update countdowns locally.
- **Shared upstream work.** In-memory caches coalesce requests for the same board. During an outage, usable older data is returned with `stale: true`.
- **Local-first reference data.** Download fares and facilities once, then compare revisions or use conditional requests to sync changes. Original CSV files are available too.
- **Traceable requests.** Responses carry `x-request-id`, which connects a client report to the server logs.

<a id="quick-start"></a>

## Run locally

Use Rust 1.85 or later for edition 2024; the current dependency set may require a newer toolchain. Start the service, then query Tseung Kwan O station:

```bash
cargo run
```

```bash
curl http://127.0.0.1:3000/api/lines/TKL/stations/TKO/next-trains
```

The default listener is `127.0.0.1:3000`. This preserved capture from 2026-09-28 shows a late-night board; the example is shortened, and the last train towards North Point has already left.

```json
{
  "line": { "code": "TKL", "name": { "en": "Tseung Kwan O Line", "tc": "將軍澳綫" } },
  "station": { "code": "TKO", "name": { "en": "Tseung Kwan O", "tc": "將軍澳" } },
  "generated_at": "2026-09-28T01:09:49+08:00",
  "fetched_at": "2026-09-28T01:09:55+08:00",
  "stale": false,
  "delayed": false,
  "alert": null,
  "directions": [
    {
      "direction": "up",
      "towards": [
        { "code": "POA", "name": { "en": "Po Lam", "tc": "寶琳" } },
        { "code": "LHP", "name": { "en": "LOHAS Park", "tc": "康城" } }
      ],
      "trains": [
        {
          "destination": { "code": "POA", "name": { "en": "Po Lam", "tc": "寶琳" } },
          "platforms": [1],
          "arrival_at": "2026-09-28T01:13:49+08:00",
          "time_type": null,
          "via_racecourse": false
        }
      ]
    },
    {
      "direction": "down",
      "towards": [
        { "code": "NOP", "name": { "en": "North Point", "tc": "北角" } }
      ],
      "trains": []
    }
  ]
}
```

<a id="endpoints"></a>

## HTTP endpoints

| Method | URL | Purpose |
| --- | --- | --- |
| `GET` | `/api/lines` | Compiled lines, stations, colours, and directions |
| `GET` | `/api/lines/status` | Network service status, including Light Rail |
| `GET` | `/api/lines/{line}/stations/{station}/next-trains` | Upcoming trains on one line at a station |
| `GET` | `/api/stations/{station}/next-trains` | Upcoming trains on every line at a station |
| `GET` | `/api/health` | Liveness; empty 200 response |
| `GET` | `/api/data` | Dataset and source-file revisions |
| `GET` | `/api/data/sources/{file}` | Original MTR CSV bytes |
| `GET` | `/api/data/stations` | Published stations and routes |
| `GET` | `/api/data/fares` | Heavy rail fares in Hong Kong cents |
| `GET` | `/api/data/airport-express-fares` | Airport Express fares |
| `GET` | `/api/data/light-rail` | Light Rail stops and routes |
| `GET` | `/api/data/light-rail-fares` | Light Rail fares |
| `GET` | `/api/data/accessibility` | Facility catalogue and station facilities |
| `GET` | `/api/mock/scenarios` | Available simulation scenarios; opt-in |
| `GET` | `/api/mock/lines/status` | Simulated service status |
| `GET` | `/api/mock/lines/{line}/stations/{station}/next-trains` | Simulated board for one line |
| `GET` | `/api/mock/stations/{station}/next-trains` | Simulated boards for a station |

See [HTTP_API.md](HTTP_API.md) for parameters, response fields, caching, errors, and examples.

<a id="mock-api"></a>

## Develop with simulated data

Enable the mock API when you need a delay, typhoon, last train, or outage on demand. It uses the same response DTOs as the real endpoints, so the client can switch from `/api` to `/api/mock`.

```bash
cargo run -- --mock-api
```

```bash
curl 'http://127.0.0.1:3000/api/mock/stations/ADM/next-trains?scenario=delayed&seed=7'
```

`scenario` selects the situation; omitting it selects a weighted random scenario. `seed` selects a simulated world. Keep it fixed to watch trains approach and depart as time advances. Responses expose the resolved values in `x-mock-scenario` and `x-mock-seed`. The [scenario catalogue](HTTP_API.md#mock-scenarios) lists every option. Mock routes are disabled by default and return `404` until enabled.

<a id="freshness"></a>

## Caching and freshness

| Data | Freshness | Failure handling |
| --- | --- | --- |
| Next Train | Usually 10 s; clamped to 2–15 s | 90 s after expiry; 5 s retry backoff |
| Line status | Poll every 30 s; fresh for 33 s | 15 min after freshness ends |
| Compiled network | Updated with deployment; max-age=86400 | No upstream dependency |
| Open data | Startup and every 24 h; fresh for 86430 s | Retry after 5 min; stale for up to 30 days after expiry |

Use the remaining `Cache-Control: public, max-age=N` to schedule polling. A stale response uses `no-cache`, which requires revalidation before reuse. Caches live in one process: a restart loses them, and multiple instances multiply upstream traffic. See [the cache design](ARCHITECTURE.md#caching-and-freshness).

<a id="configuration"></a>

## Configuration

| Flag | Environment | Default | Meaning |
| --- | --- | --- | --- |
| `--bind-address <ADDRESS>` | `DUT_BIND_ADDRESS` | `127.0.0.1:3000` | IP and port; hostnames are rejected |
| `--log-level <LEVEL>` | `RUST_LOG` | `info,dut=debug,tower_http=debug` | Log filter |
| `--log-file <PATH>` | `DUT_LOG_FILE` | None | Append a second plain-text log stream |
| `--mock-api` | `DUT_MOCK_API` | Disabled | Enable simulation routes |

Flags override environment variables. `DUT_MOCK_API` accepts `true/false`, `1/0`, `yes/no`, and `on/off`. `dut --help` lists options; `dut --version` prints the version. With Cargo, put flags after `--`. Empty or invalid values stop startup with exit status 2.

```bash
cargo run -- --bind-address 0.0.0.0:3000 --log-level info
```

Endpoints, timeouts, and cache policies are defaults in [config.rs](src/bootstrap/config.rs). `SIGTERM` or Ctrl-C stops accepting requests and drains in-flight work. Outbound HTTP uses environment or macOS system proxies. The macOS bypass list is not applied; set `NO_PROXY` for direct hosts. Tests explicitly connect directly to local fake upstreams.

<a id="docker"></a>

## Containers

The Dockerfile cross-compiles a static musl binary with Alpine and `xx`, then copies it into a distroless static image with CA certificates and a non-root user. There is no shell or curl. The image sets `DUT_BIND_ADDRESS=0.0.0.0:3000`.

```bash
docker build -t dut .
docker run --rm -p 3000:3000 dut
```

```bash
docker buildx build --platform linux/amd64,linux/arm64 -t dut .
```

Use the orchestrator or load balancer to probe `GET /api/health`; the image has no `HEALTHCHECK`.

```bash
docker run --rm -p 3000:3000 ghcr.io/jimmyrice/dut:latest
```

<a id="releases"></a>

## Releases

Set `[workspace.package].version` in `Cargo.toml`, commit it, and push a matching `v<version>` tag. A mismatch fails before compilation. The following is an example version, not the current one:

```bash
git tag v0.6.0
git push origin v0.6.0
```

[release.yml](.github/workflows/release.yml) publishes six binaries plus `SHA256SUMS`. Tags containing `-` are prereleases. [docker.yml](.github/workflows/docker.yml) publishes version and minor tags, plus a major tag from 1.0 onwards; prereleases do not update `latest`. Relevant code or Dockerfile changes on `master` update `edge`. A manual Release workflow run stores build artifacts without creating a release.

| Platform | Architecture | Archive |
| --- | --- | --- |
| Linux | x86-64 | `dut-x86_64-unknown-linux-musl.tar.gz` |
| Linux | arm64 | `dut-aarch64-unknown-linux-musl.tar.gz` |
| macOS | Apple Silicon | `dut-aarch64-apple-darwin.tar.gz` |
| macOS | Intel | `dut-x86_64-apple-darwin.tar.gz` |
| Windows | x86-64 | `dut-x86_64-pc-windows-msvc.zip` |
| Windows | arm64 | `dut-aarch64-pc-windows-msvc.zip` |

Linux builds are statically linked and need no glibc. Custom minimal containers still need `ca-certificates`; use `SSL_CERT_FILE` if the bundle lives elsewhere. Windows builds include the C runtime. Distribution builds use fat LTO and strip symbols; panic aborts the process, so configure automatic restart. Build the same profile locally with `cargo build --profile dist` (`target/dist/dut`). It takes longer than a release build.

The repository is now public, so the release workflows attach GitHub-signed build provenance to new binary archives and container images. Verify an archive with:

```bash
gh attestation verify dut-x86_64-unknown-linux-musl.tar.gz -R JimmyRice/Dut
```

<a id="logging"></a>

## Logging and operations

The default filter includes Dut debug logs. Set `--log-level info` for quieter output, or use target filters such as `dut=debug`; targets match prefixes, including `dut_api` and `dut_upstream`. Invalid filters and empty values fail startup. The first `logging started` entry records the active filter and file path.

Terminal output groups each request into a coloured block; `NO_COLOR=1` disables colour. Pipes and log files use one plain-text line per entry. `--log-file` appends a second stream and requires an existing parent directory. Dut does not rotate files; use `copytruncate` with logrotate or restart after rotation. In containers, collecting stdout is usually enough.

Startup probes report each upstream as reachable or unreachable without blocking the listener. Background polls log their source, cleaned dataset counts, network drift, and health transitions (`failing` or `blind`). Monitor events include routine changes such as service ending for the night. The first successful value is a data baseline; source-health transitions can still emit events at startup.

<a id="development"></a>

## Development

```bash
cargo fmt -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo deny check advisories bans sources
```

Run these at the workspace root before finishing a change. `cargo deny` needs `cargo install cargo-deny --locked`, checks dependencies for advisories, yanked releases, and unknown sources, and skips licences; CI runs it on every push and pull request. Use `-p <crate>` for focused work. Tests are deterministic and offline: paused Tokio time for timing and wiremock with captured fixtures for upstreams. Route tests build the whole app; on macOS they raise the file-descriptor limit to accommodate parallel servers.

```text
src/                 composition root, configuration, startup
crates/dut-core/     domain and application
crates/dut-http/     shared outbound HTTP
crates/dut-upstream/ adapters, CSV cleaning, caches, probes
crates/dut-poll/     scheduled feeds and source health
crates/dut-monitor/  diffs and event delivery
crates/dut-mock/     pure simulation
crates/dut-api/      Axum routes, DTOs, HTTP errors
crates/dut-telemetry/ log output
tests/api/           route tests
tests/fixtures/      captured upstream responses
scripts/             development tools
```

<a id="sync-network"></a>

### Update the compiled network

When a daily poll reports network drift, start the service and run the standard-library-only script:

```bash
python3 scripts/sync-network.py
```

It compares `/api/data/stations` with `/api/lines`. `--write` regenerates the `STATIONS` table in `station.rs`, keeping compiled-only stations such as Racecourse. Review line order, branches, and termini yourself: the script prints `codes![...]` suggestions but never edits `line.rs`. Then format, test, and review the diff. Exit status is 0 for no drift, 1 for drift.

Read [HTTP_API.md](HTTP_API.md) for client contracts, [RUST_API.md](RUST_API.md) for business-facing types, [ARCHITECTURE.md](ARCHITECTURE.md) for design decisions, and [AGENTS.md](AGENTS.md) for contribution rules.

<a id="sources"></a>

## Data sources

- [MTR Next Train API](https://rt.data.gov.hk/v1/transport/mtr/getSchedule.php)
- [MTR line status](https://tnews.mtr.com.hk/alert/ryg_line_status.json)
- [MTR open data portal](https://opendata.mtr.com.hk/)
- [Hong Kong Observatory warning information](https://data.weather.gov.hk/weatherAPI/opendata/weather.php?dataType=warningInfo&lang=en)

Weather warnings are polled every minute for monitor events; there is no public weather endpoint.
