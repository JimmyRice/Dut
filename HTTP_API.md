# HTTP API

[English](HTTP_API.md) · [繁體廣東話](docs/zh-HK/HTTP_API.md) · [简体中文](docs/zh-CN/HTTP_API.md)

This is the client contract for Dut. Update endpoint URLs, methods, parameters, examples, fields, caching, errors, and the changelog in the same change as the implementation. Keep all language versions in sync. The response captures below are preserved from the previous documentation; they are historical examples, not live data. Excerpts remain valid JSON. Simulated and illustrative examples are labelled separately.

## Contents

- [Overview](#overview)
- [Shared conventions](#conventions)
- [1. Lines and stations](#lines)
- [2. Network service status](#line-status)
- [3. Next trains on one line](#next-trains)
- [4. Next trains across a station](#station-next-trains)
- [5. Liveness](#health)
- [Open data: shared behaviour](#open-data)
- [6. Open-data index](#data-index)
- [7. Original source files](#source-files)
- [8. Published stations and routes](#published-stations)
- [9. Heavy rail fares](#fares)
- [10. Airport Express fares](#airport-express-fares)
- [11. Light Rail stops and routes](#light-rail)
- [12. Light Rail fares](#light-rail-fares)
- [13. Accessibility facilities](#accessibility)
- [Mock API: shared behaviour](#mock-api)
- [14. Scenario catalogue](#mock-scenarios)
- [15. Simulated network status](#mock-line-status)
- [16. Simulated board on one line](#mock-next-trains)
- [17. Simulated boards across a station](#mock-station-next-trains)
- [18. Simulated event stream](#mock-events)
- [Error codes](#errors)
- [Appendix A: line codes](#line-codes)
- [Appendix B: station codes](#station-codes)
- [Changelog](#changelog)

<a id="overview"></a>

## Overview

The local base URL is `http://127.0.0.1:3000`. All routes live under `/api` and use `GET`; health also supports `HEAD`. There is no authentication. JSON responses use `application/json` with UTF-8; source files use `text/csv; charset=utf-8`; the simulated event stream uses `text/event-stream`. Health has no body or content type. gzip is negotiated through `Accept-Encoding`. Mock routes require `--mock-api` or `DUT_MOCK_API`.

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
| `GET` | `/api/mock/health` | Mock availability check; opt-in |
| `GET` | `/api/mock/scenarios` | Available simulation scenarios; opt-in |
| `GET` | `/api/mock/lines/status` | Simulated service status |
| `GET` | `/api/mock/lines/{line}/stations/{station}/next-trains` | Simulated board for one line |
| `GET` | `/api/mock/stations/{station}/next-trains` | Simulated boards for a station |
| `GET` | `/api/mock/events` | Simulated line status events (Server-Sent Events); opt-in |

<a id="conventions"></a>

## Shared conventions

- **Time:** RFC 3339, Hong Kong offset `+08:00`, whole seconds. Compute countdowns from `arrival_at - device time`, never from `generated_at` or `fetched_at`. Upstream `ttnt` is discarded.
- **Codes:** line and station path codes are case-insensitive; response codes are uppercase.
- **Names:** `{ "en": "...", "tc": "..." }` supplies English and Traditional Chinese in one request. Documentation language does not change the wire schema.
- **Money:** integer Hong Kong cents. `490` means HK$4.90; divide by 100 for display.
- **Directions:** `up` and `down` are stable MTR identifiers, not compass directions. Use `towards` for a platform-style heading and a train’s `destination` for its actual terminus. Short workings remain in the same direction. Disneyland Resort’s `up` runs towards Sunny Bay; branch layouts also prevent inferring direction from a flat station list.

| Header / field | Meaning |
| --- | --- |
| `Cache-Control: public, max-age=N` | Remaining whole seconds of freshness; usable as a polling hint |
| `Cache-Control: no-cache` | Revalidate before reuse; the response can still be stored |
| `stale` | The value has expired and is being served within the fallback window |
| `generated_at` / `updated_at` | Upstream generation/publication time; an old publication time alone does not mean stale data |
| `fetched_at` | Time of the last successful fetch by Dut |
| `ETag` | Dataset/file validator; send it back in `If-None-Match` |
| `x-request-id` | Generated UUID unless a caller supplies a header; returned for traced routes, absent on health |

Application errors use the JSON envelope below, with a stable `code` and an English `message`. They carry no `Cache-Control`. Upstream details stay in server logs. Lookup errors do not echo submitted codes; the `not_found` message does include the unmatched route path.

```json
{
  "error": {
    "code": "unknown_station",
    "message": "No station matches the requested station code"
  }
}
```

<a id="lines"></a>

## 1. Lines and stations

```http
GET /api/lines
```

No path parameters.

No query parameters are read.

Returns the ten Next Train lines in compiled-network order: AEL, TCL, TML, TKL, EAL, SIL, TWL, ISL, KTL, DRL. Download once for navigation. Data changes only with deployment. `Cache-Control: public, max-age=86400`; no business errors.

```bash
curl http://127.0.0.1:3000/api/lines
```

`200 OK`, `application/json`. Preserved historical response excerpt:

```json
{
  "lines": [
    {
      "code": "DRL",
      "name": { "en": "Disneyland Resort Line", "tc": "迪士尼綫" },
      "color": "#F550A6",
      "stations": [
        { "code": "SUN", "name": { "en": "Sunny Bay", "tc": "欣澳" } },
        { "code": "DIS", "name": { "en": "Disneyland Resort", "tc": "迪士尼" } }
      ],
      "directions": [
        {
          "direction": "up",
          "towards": [
            { "code": "SUN", "name": { "en": "Sunny Bay", "tc": "欣澳" } }
          ]
        },
        {
          "direction": "down",
          "towards": [
            { "code": "DIS", "name": { "en": "Disneyland Resort", "tc": "迪士尼" } }
          ]
        }
      ]
    }
  ]
}
```

| Field | Type | Meaning |
| --- | --- | --- |
| `lines` | array | Ordered lines |
| `lines[].code`, `lines[].name` | string, object | Line code and bilingual name |
| `lines[].color` | string | Brand colour as `#RRGGBB` |
| `lines[].stations` | array | Station references in line order |
| `lines[].directions` | array | `up` then `down`, each with `direction` and station-reference array `towards` |

<a id="station-reference"></a>

### Station reference

| Field | Type | Meaning |
| --- | --- | --- |
| `code` | string | Three uppercase letters |
| `name` | object \| null | `{ en, tc }`; null only for an upstream station unknown to the compiled network |

<a id="line-status"></a>

## 2. Network service status

```http
GET /api/lines/status
```

No path parameters.

No query parameters are read.

```bash
curl http://127.0.0.1:3000/api/lines/status
```

Includes eleven lines, with Light Rail last in the upstream order. `condition` is for client logic; `display` follows the MTR website’s presentation.

`200 OK`, `application/json`. Preserved historical response excerpt:

```json
{
  "updated_at": "2026-09-28T06:15:01+08:00",
  "fetched_at": "2026-09-28T23:30:07+08:00",
  "stale": false,
  "lines": [
    {
      "line": { "code": "TWL", "name": { "en": "Tsuen Wan Line", "tc": "荃灣綫" } },
      "color": "#FF0000",
      "condition": "normal",
      "display": "green",
      "message": null
    },
    {
      "line": { "code": "KTL", "name": { "en": "Kwun Tong Line", "tc": "觀塘綫" } },
      "color": "#1A9431",
      "condition": "normal",
      "display": "green",
      "message": null
    },
    {
      "line": { "code": "ISL", "name": { "en": "Island Line", "tc": "港島綫" } },
      "color": "#0860A8",
      "condition": "normal",
      "display": "green",
      "message": null
    }
  ]
}
```

| Field | Type | Meaning |
| --- | --- | --- |
| `updated_at`, `fetched_at`, `stale` | string, string, boolean | Publication time, fetch time, and freshness flag |
| `lines` | array | Upstream line order |
| `lines[].line`, `lines[].color` | object, string | Line `{ code, name }` and brand colour |
| `lines[].condition`, `lines[].display` | string | Semantic condition and display value below |
| `lines[].message` | string \| null | Upstream explanation, if present |

| Upstream | condition | display | Meaning |
| --- | --- | --- | --- |
| green | `normal` | green | Normal service |
| yellow | `delayed` | yellow | Delay or gradual recovery |
| red | `disrupted` | red | Disruption; consider other transport |
| pink | `delayed_or_disrupted` | yellow | Delayed or disrupted |
| grey | `non_service_hours` | grey | Outside service hours |
| typhoon | `typhoon_signal` | typhoon | Tropical cyclone signal |
| Other | `unknown` | grey | Unrecognised status; logged as a warning |

Illustrative delayed-line element, not a live capture:

```json
{
  "line": { "code": "KTL", "name": { "en": "Kwun Tong Line", "tc": "觀塘綫" } },
  "color": "#1A9431",
  "condition": "delayed_or_disrupted",
  "display": "yellow",
  "message": "Trains are delayed"
}
```

Polling is every 30 seconds; freshness lasts 33 seconds from a successful fetch, so `max-age` is at most 33 and decreases with age. Requests read the poller and wait for the first poll at startup. Expired values may be served for another 15 minutes with `stale: true` and `no-cache`. No usable snapshot yields `502 upstream_unavailable`.

<a id="next-trains"></a>

## 3. Next trains on one line

```http
GET /api/lines/{line}/stations/{station}/next-trains
```

| Path parameter | Meaning |
| --- | --- |
| `line` | Required line code; Light Rail `LR` has no boards |
| `station` | Required known station on the requested line |

No query parameters are read.

```bash
curl http://127.0.0.1:3000/api/lines/TKL/stations/TKO/next-trains
```

Returns up to four trains per direction. Departed trains beyond the 30-second grace are filtered. A terminus direction is omitted unless upstream still reports a train there. A valid direction can contain no trains outside service hours. `towards` lists major reachable termini; short workings use each train’s `destination`.

`200 OK`, `application/json`. Preserved historical response excerpt:

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
        },
        {
          "destination": { "code": "LHP", "name": { "en": "LOHAS Park", "tc": "康城" } },
          "platforms": [1],
          "arrival_at": "2026-09-28T01:16:49+08:00",
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

| Field | Type | Meaning |
| --- | --- | --- |
| `line`, `station` | object | Line and station references |
| `generated_at`, `fetched_at` | string | Upstream generation and Dut fetch times |
| `stale`, `delayed` | boolean | Expired fallback data; MTR delay flag (independent values) |
| `alert` | object \| null | `{ en: notice, tc: notice }`; each notice has `message` and nullable `url` |
| `directions` | array | Boarding directions, `up` first |
| `directions[].direction` | string | `up` or `down` |
| `directions[].towards` | array | Major terminus references; may be empty if upstream reports a train beyond the final major terminus |
| `directions[].trains` | array | Up to four arrivals in time order |
| `directions[].trains[].destination` | object | Actual train terminus reference |
| `directions[].trains[].platforms` | array of integer | Ordered platforms, usually `[1]`; Airport uses `[1, 3]` towards AsiaWorld-Expo or `[2, 4]` towards Hong Kong. Unreadable platforms become `[]`, keeping the train |
| `directions[].trains[].arrival_at` | string | Expected arrival, or departure when `time_type=departure`; use for countdowns |
| `directions[].trains[].time_type` | string \| null | East Rail: `arrival` or `departure`; other lines: null |
| `directions[].trains[].via_racecourse` | boolean | East Rail via Racecourse rather than Fo Tan; false elsewhere |

Additional preserved excerpts: Po Lam’s departure direction, one East Rail train, and an Airport Express train at Airport on 2026-10-02. The first object contains only the `directions` field, not the complete response.

```json
{
  "directions": [
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

```json
{
  "destination": { "code": "SHS", "name": { "en": "Sheung Shui", "tc": "上水" } },
  "platforms": [2],
  "arrival_at": "2026-09-28T00:19:34+08:00",
  "time_type": "arrival",
  "via_racecourse": false
}
```

```json
{
  "destination": { "code": "AWE", "name": { "en": "AsiaWorld-Expo", "tc": "博覽館" } },
  "platforms": [1, 3],
  "arrival_at": "2026-10-02T16:33:00+08:00",
  "time_type": null,
  "via_racecourse": false
}
```

Illustrative special-arrangement alert, not a live capture:

```json
{
  "alert": {
    "en": { "message": "Special train service arrangement", "url": "https://example.com/notice" },
    "tc": { "message": "特別列車服務安排", "url": "https://example.com/notice" }
  }
}
```

Freshness follows the CDN’s remaining TTL, normally 10 seconds and clamped to 2–15. An expired board waits for one coalesced refresh. Failed refreshes may serve data up to 90 seconds after expiry, with `stale: true`; retries back off for 5 seconds. Errors: `404 unknown_line`, `404 unknown_station`, `404 station_not_on_line` (including `LR`), or `502 upstream_unavailable` when no usable board remains.

```bash
curl http://127.0.0.1:3000/api/lines/TKL/stations/ADM/next-trains
```

```json
{
  "error": {
    "code": "station_not_on_line",
    "message": "The requested line does not serve the requested station"
  }
}
```

<a id="station-next-trains"></a>

## 4. Next trains across a station

```http
GET /api/stations/{station}/next-trains
```

| Path parameter | Meaning |
| --- | --- |
| `station` | Required known station code; no line parameter |

No query parameters are read.

```bash
curl http://127.0.0.1:3000/api/stations/ADM/next-trains
```

Fetches each serving line concurrently through its own cache. One successful line is enough for `200`; failures stay visible as `board: null` plus an error. Admiralty covers EAL, SIL, TWL, and ISL. The excerpt below shows EAL and TWL after the last trains have left.

`200 OK`, `application/json`. Preserved historical response excerpt:

```json
{
  "station": { "code": "ADM", "name": { "en": "Admiralty", "tc": "金鐘" } },
  "lines": [
    {
      "line": { "code": "EAL", "name": { "en": "East Rail Line", "tc": "東鐵綫" } },
      "board": {
        "generated_at": "2026-09-28T01:09:49+08:00",
        "fetched_at": "2026-09-28T01:09:57+08:00",
        "stale": false,
        "delayed": false,
        "alert": null,
        "directions": [
          {
            "direction": "up",
            "towards": [
              { "code": "LOW", "name": { "en": "Lo Wu", "tc": "羅湖" } },
              { "code": "LMC", "name": { "en": "Lok Ma Chau", "tc": "落馬洲" } }
            ],
            "trains": []
          }
        ]
      },
      "error": null
    },
    {
      "line": { "code": "TWL", "name": { "en": "Tsuen Wan Line", "tc": "荃灣綫" } },
      "board": {
        "generated_at": "2026-09-28T01:09:46+08:00",
        "fetched_at": "2026-09-28T01:09:57+08:00",
        "stale": false,
        "delayed": false,
        "alert": null,
        "directions": [
          {
            "direction": "up",
            "towards": [
              { "code": "TSW", "name": { "en": "Tsuen Wan", "tc": "荃灣" } }
            ],
            "trains": []
          },
          {
            "direction": "down",
            "towards": [
              { "code": "CEN", "name": { "en": "Central", "tc": "中環" } }
            ],
            "trains": []
          }
        ]
      },
      "error": null
    }
  ]
}
```

| Field | Type | Meaning |
| --- | --- | --- |
| `station` | object | Station reference |
| `lines` | array | Serving lines in `/api/lines` order |
| `lines[].line` | object | `{ code, name }` |
| `lines[].board` | object \| null | Section 3 fields without `line` and `station`; null on failure |
| `lines[].error` | object \| null | `{ code, message }`; null on success |

Illustrative failing-line element:

```json
{
  "line": { "code": "SIL", "name": { "en": "South Island Line", "tc": "南港島綫" } },
  "board": null,
  "error": {
    "code": "upstream_unavailable",
    "message": "An upstream service is unavailable"
  }
}
```

The response takes the shortest remaining freshness of successful boards. Any stale successful board makes it `no-cache`. Errors: `404 unknown_station`; `502 upstream_unavailable` only if every line fails.

<a id="health"></a>

## 5. Liveness

```http
GET /api/health
```

No path parameters.

No query parameters are read.

```bash
curl -i http://127.0.0.1:3000/api/health
```

`GET` and `HEAD` return `200`, an empty body, no content type, and `Cache-Control: no-store`. There is no upstream call, request log, or generated request ID. This checks the HTTP process, not upstream availability. There are no business errors; an unreachable service fails at connection level.

```http
HTTP/1.1 200 OK
cache-control: no-store
content-length: 0
```

<a id="open-data"></a>

## Open data: shared behaviour

Sections 6–13 read one in-memory snapshot of seven MTR CSV files. The first poll starts immediately, followed by a poll every 24 hours and a 5-minute retry on failure. A publish is all-or-nothing. Readers may wait for the first poll; requests never trigger a new portal fetch. Freshness is 86430 seconds from fetch, then a 30-day stale window, after which `502 upstream_unavailable` is returned. A new process has no prior snapshot.

For local-first sync, request `/api/data`, compare opaque revisions with local copies, and download only changed datasets. Datasets use weak ETags containing service version plus content hash, such as `W/"0.5.5-5987f5c9680ce780"`; raw files use only the byte hash. Send `If-None-Match` to receive an empty `304` with ETag and caching headers when unchanged. The index itself has no ETag. Treat revisions as equality tokens, not timestamps or sortable versions.

```bash
curl -i -H 'If-None-Match: W/"0.5.5-5987f5c9680ce780"' http://127.0.0.1:3000/api/data/fares
```

```http
HTTP/1.1 304 Not Modified
etag: W/"0.5.5-5987f5c9680ce780"
cache-control: public, max-age=86000
```

The preceding 304 headers illustrate the conditional contract; remaining freshness depends on request time. Fresh responses use `public, max-age=N`; stale ones use `no-cache`. Datasets/files share identity and maximum-level gzip encodings per fetched state, with `Content-Length` and `Vary: Accept-Encoding`. The first request after a fetch pays encoding cost; later requests reuse bytes.

| Field | Type | Meaning |
| --- | --- | --- |
| `updated_at` | string \| null | Dataset source `Last-Modified`; may be months old or absent without making the snapshot stale |
| `fetched_at` | string | Last successful full poll |
| `stale` | boolean | Snapshot past its freshness window |

<a id="data-index"></a>

## 6. Open-data index

```http
GET /api/data
```

No path parameters.

No query parameters are read.

```bash
curl http://127.0.0.1:3000/api/data
```

`200 OK`, `application/json`. Preserved historical response excerpt:

```json
{
  "fetched_at": "2026-09-29T03:08:55+08:00",
  "stale": false,
  "datasets": [
    {
      "name": "stations",
      "path": "/api/data/stations",
      "revision": "0.5.5-0d1201f5804cd4cd",
      "updated_at": "2023-11-21T18:09:07+08:00"
    },
    {
      "name": "fares",
      "path": "/api/data/fares",
      "revision": "0.5.5-5987f5c9680ce780",
      "updated_at": "2026-04-03T01:02:50+08:00"
    },
    {
      "name": "airport-express-fares",
      "path": "/api/data/airport-express-fares",
      "revision": "0.5.5-545393302ce4c18f",
      "updated_at": "2025-06-22T01:05:16+08:00"
    },
    {
      "name": "light-rail",
      "path": "/api/data/light-rail",
      "revision": "0.5.5-97a676332b659216",
      "updated_at": "2026-07-05T00:58:02+08:00"
    },
    {
      "name": "light-rail-fares",
      "path": "/api/data/light-rail-fares",
      "revision": "0.5.5-e9deb534253c3bde",
      "updated_at": "2024-06-30T01:39:03+08:00"
    },
    {
      "name": "accessibility",
      "path": "/api/data/accessibility",
      "revision": "0.5.5-b199444f08da98bf",
      "updated_at": "2023-06-25T02:28:09+08:00"
    }
  ],
  "sources": [
    {
      "file": "mtr_lines_and_stations.csv",
      "path": "/api/data/sources/mtr_lines_and_stations.csv",
      "revision": "55f8eab5c379633b",
      "updated_at": "2023-11-21T18:09:07+08:00",
      "bytes": 14161
    },
    {
      "file": "mtr_lines_fares.csv",
      "path": "/api/data/sources/mtr_lines_fares.csv",
      "revision": "570e3401233f8a52",
      "updated_at": "2026-04-03T01:02:50+08:00",
      "bytes": 743938
    }
  ]
}
```

| Field | Type | Meaning |
| --- | --- | --- |
| `fetched_at`, `stale` | string, boolean | Snapshot metadata; no top-level `updated_at` |
| `datasets`, `sources` | array | Fixed-order cleaned datasets and seven original files |
| `datasets[].name`, `datasets[].path` | string | Dataset name and download path; names match the six dataset URL suffixes |
| `datasets[].revision`, `sources[].revision` | string | ETag value without `W/` or quotes |
| `datasets[].updated_at`, `sources[].updated_at` | string \| null | Upstream modification time |
| `sources[].file`, `sources[].path` | string | Original filename and download path |
| `sources[].bytes` | integer | Uncompressed byte count |

The index has no ETag and does not return `304`; use its revisions to validate individual downloads.

Caching and conditional requests follow [shared open-data behaviour](#open-data). Error: `502 upstream_unavailable` when no usable complete snapshot exists.

<a id="source-files"></a>

## 7. Original source files

```http
GET /api/data/sources/{file}
```

| Path parameter | Meaning |
| --- | --- |
| `file` | Required case-insensitive filename from the list below |

No query parameters are read.

- `mtr_lines_and_stations.csv`
- `mtr_lines_fares.csv`
- `airport_express_fares.csv`
- `light_rail_routes_and_stops.csv`
- `light_rail_fares.csv`
- `barrier_free_facility_category.csv`
- `barrier_free_facilities.csv`

```bash
curl -i http://127.0.0.1:3000/api/data/sources/airport_express_fares.csv
```

`200 OK`, `text/csv; charset=utf-8`. The decoded body preserves every upstream byte, including BOMs, CRLF, and source typos. The following historical excerpt shows the header and first four rows; the full identity body was 691 bytes.

```csv
ST_FROM,ST_FROM_ID,ST_TO,ST_TO_ID,OCT_ADT_FARE,OCT_CHD_FARE,SINGLE_ADT_FARE,SINGLE_CHD_FARE
HongKong,44,Airport,47,120,60,130,65
HongKong,44,AsiaWorld-Expo,56,120,60,130,65
Kowloon,45,Airport,47,105,52.5,115,57.5
Kowloon,45,AsiaWorld-Expo,56,105,52.5,115,57.5
```

Column definitions belong to the MTR portal’s source-file documentation. For normalised field contracts, use the JSON datasets below. Raw-file ETags change only with bytes, not service upgrades. Unknown names return `404 unknown_source`.

Caching and conditional requests follow [shared open-data behaviour](#open-data). Error: `502 upstream_unavailable` when no usable complete snapshot exists.

<a id="published-stations"></a>

## 8. Published stations and routes

```http
GET /api/data/stations
```

No path parameters.

No query parameters are read.

```bash
curl http://127.0.0.1:3000/api/data/stations
```

From `mtr_lines_and_stations.csv`. Unlike `/api/lines`, this reflects published data rather than the compiled network. The preserved capture had 97 stations and ten lines; Racecourse was absent. IDs become station codes, names are trimmed, `茘` is normalised to `荔`, blank rows are removed, and branch routes use `from`/`towards` instead of internal labels such as `LMC-UT`.

`200 OK`, `application/json`. Preserved historical response excerpt:

```json
{
  "updated_at": "2023-11-21T18:09:07+08:00",
  "fetched_at": "2026-09-29T03:08:55+08:00",
  "stale": false,
  "stations": [
    {
      "code": "ADM",
      "name": { "en": "Admiralty", "tc": "金鐘" },
      "lines": ["EAL", "SIL", "TWL", "ISL"]
    },
    {
      "code": "LAK",
      "name": { "en": "Lai King", "tc": "荔景" },
      "lines": ["TCL", "TWL"]
    }
  ],
  "lines": [
    {
      "code": "TKL",
      "name": { "en": "Tseung Kwan O Line", "tc": "將軍澳綫" },
      "routes": [
        {
          "direction": "up",
          "from": { "code": "NOP", "name": { "en": "North Point", "tc": "北角" } },
          "towards": { "code": "POA", "name": { "en": "Po Lam", "tc": "寶琳" } },
          "stations": ["NOP", "QUB", "YAT", "TIK", "TKO", "HAH", "POA"]
        },
        {
          "direction": "up",
          "from": { "code": "TIK", "name": { "en": "Tiu Keng Leng", "tc": "調景嶺" } },
          "towards": { "code": "LHP", "name": { "en": "LOHAS Park", "tc": "康城" } },
          "stations": ["TIK", "TKO", "LHP"]
        },
        {
          "direction": "down",
          "from": { "code": "POA", "name": { "en": "Po Lam", "tc": "寶琳" } },
          "towards": { "code": "NOP", "name": { "en": "North Point", "tc": "北角" } },
          "stations": ["POA", "HAH", "TKO", "TIK", "YAT", "QUB", "NOP"]
        },
        {
          "direction": "down",
          "from": { "code": "LHP", "name": { "en": "LOHAS Park", "tc": "康城" } },
          "towards": { "code": "TIK", "name": { "en": "Tiu Keng Leng", "tc": "調景嶺" } },
          "stations": ["LHP", "TKO", "TIK"]
        }
      ]
    }
  ]
}
```

| Field | Type | Meaning |
| --- | --- | --- |
| `updated_at`, `fetched_at`, `stale` | metadata | Shared dataset metadata |
| `stations` | array | Sorted by station code |
| `stations[].code`, `stations[].name`, `stations[].lines` | string, object, array of string | Code, published bilingual name, serving lines in compiled line order |
| `lines[].code`, `lines[].name` | string, object | Line code and bilingual name; lines with no published routes are omitted |
| `lines[].routes` | array | `up` before `down`, main route before branches |
| `lines[].routes[].direction` | string | `up` / `down`; Disneyland `up` is Disney to Sunny Bay |
| `lines[].routes[].from`, `lines[].routes[].towards` | object | Start and terminus station references |
| `lines[].routes[].stations` | array of string | Ordered stops; a branch may list only the branch section, such as Tiu Keng Leng to LOHAS Park |

Caching and conditional requests follow [shared open-data behaviour](#open-data). Error: `502 upstream_unavailable` when no usable complete snapshot exists.

<a id="fares"></a>

## 9. Heavy rail fares

```http
GET /api/data/fares
```

No path parameters.

No query parameters are read.

```bash
curl --compressed http://127.0.0.1:3000/api/data/fares
```

From `mtr_lines_fares.csv`, excluding Airport Express. IDs become station codes; Racecourse resolves to `RAC` by name. The historical full response held 9120 trips, about 1.7 MB JSON or 71 KB gzip. Use compression for full downloads.

`200 OK`, `application/json`. Preserved historical response excerpt:

```json
{
  "updated_at": "2026-04-03T01:02:50+08:00",
  "fetched_at": "2026-09-29T03:08:55+08:00",
  "stale": false,
  "fares": [
    {
      "from": "CEN",
      "to": "ADM",
      "octopus": {
        "adult": 490,
        "student": 320,
        "joyyou_sixty": 200,
        "child": 320,
        "elderly": 200,
        "disability": 200
      },
      "single_journey": { "adult": 500, "child": 350, "elderly": 350 }
    },
    {
      "from": "RAC",
      "to": "FOT",
      "octopus": {
        "adult": 770,
        "student": 380,
        "joyyou_sixty": 770,
        "child": 380,
        "elderly": 200,
        "disability": 200
      },
      "single_journey": { "adult": 800, "child": 400, "elderly": 400 }
    }
  ]
}
```

| Field | Type | Meaning |
| --- | --- | --- |
| `updated_at`, `fetched_at`, `stale` | metadata | Shared dataset metadata |
| `fares[].from`, `fares[].to` | string | Origin and destination station codes |
| `fares` | array | Trips sorted by origin then destination; A→B and B→A are separate; same-stop trips are removed |
| `fares[].octopus.adult` | integer | Adult Octopus, cents |
| `fares[].octopus.student` | integer | Student concession Octopus |
| `fares[].octopus.joyyou_sixty` | integer | JoyYou, ages 60–64 |
| `fares[].octopus.child` | integer | Child Octopus |
| `fares[].octopus.elderly` | integer | Elderly Octopus, age 65+ |
| `fares[].octopus.disability` | integer | Disability concession |
| `fares[].single_journey.adult` | integer | Adult single journey |
| `fares[].single_journey.child` | integer | Child single journey |
| `fares[].single_journey.elderly` | integer | Elderly single journey |

Caching and conditional requests follow [shared open-data behaviour](#open-data). Error: `502 upstream_unavailable` when no usable complete snapshot exists.

<a id="airport-express-fares"></a>

## 10. Airport Express fares

```http
GET /api/data/airport-express-fares
```

No path parameters.

No query parameters are read.

```bash
curl http://127.0.0.1:3000/api/data/airport-express-fares
```

From `airport_express_fares.csv`; all amounts are cents. Separate upstream IDs 44–46 resolve to HOK, KOW, and TSY. The historical full capture contained 14 trips.

`200 OK`, `application/json`. Preserved historical response excerpt:

```json
{
  "updated_at": "2025-06-22T01:05:16+08:00",
  "fetched_at": "2026-09-29T03:08:55+08:00",
  "stale": false,
  "fares": [
    {
      "from": "HOK",
      "to": "AIR",
      "octopus": { "adult": 12000, "child": 6000 },
      "single_journey": { "adult": 13000, "child": 6500 }
    }
  ]
}
```

| Field | Type | Meaning |
| --- | --- | --- |
| `updated_at`, `fetched_at`, `stale` | metadata | Shared dataset metadata |
| `fares` | array | Sorted by origin then destination |
| `fares[].from`, `fares[].to` | string | Origin and destination station codes |
| `fares[].octopus.adult`, `fares[].octopus.child` | integer | Adult and child Octopus cents |
| `fares[].single_journey.adult`, `fares[].single_journey.child` | integer | Adult and child single-journey cents |

Caching and conditional requests follow [shared open-data behaviour](#open-data). Error: `502 upstream_unavailable` when no usable complete snapshot exists.

<a id="light-rail"></a>

## 11. Light Rail stops and routes

```http
GET /api/data/light-rail
```

No path parameters.

No query parameters are read.

```bash
curl http://127.0.0.1:3000/api/data/light-rail
```

From `light_rail_routes_and_stops.csv`. Stop IDs are numeric and match the MTR Light Rail arrival API; three-letter stop codes are a separate namespace from heavy rail station codes. Routes are sorted numerically with suffixes (`614` before `614P`); consecutive duplicate stops at turnarounds are collapsed. The preserved capture had 68 stops and eleven routes.

`200 OK`, `application/json`. Preserved historical response excerpt:

```json
{
  "updated_at": "2026-07-05T00:58:02+08:00",
  "fetched_at": "2026-09-29T03:08:55+08:00",
  "stale": false,
  "stops": [
    {
      "id": 1,
      "code": "FEP",
      "name": { "en": "Tuen Mun Ferry Pier", "tc": "屯門碼頭" },
      "routes": ["507", "610", "614", "614P", "615", "615P"]
    }
  ],
  "routes": [
    {
      "route": "505",
      "directions": [
        {
          "from": { "id": 920, "name": { "en": "Sam Shing", "tc": "三聖" } },
          "towards": { "id": 100, "name": { "en": "Siu Hong", "tc": "兆康" } },
          "stops": [920, 265, 270, 280, 295, 60, 190, 180, 170, 160, 150, 140, 130, 120, 110, 100]
        },
        {
          "from": { "id": 100, "name": { "en": "Siu Hong", "tc": "兆康" } },
          "towards": { "id": 920, "name": { "en": "Sam Shing", "tc": "三聖" } },
          "stops": [100, 120, 130, 140, 150, 160, 170, 200, 60, 295, 280, 270, 265, 920]
        }
      ]
    }
  ]
}
```

| Field | Type | Meaning |
| --- | --- | --- |
| `updated_at`, `fetched_at`, `stale` | metadata | Shared dataset metadata |
| `stops` | array | Sorted by stop ID |
| `stops[].id`, `stops[].code`, `stops[].name` | integer, string, object | Numeric ID, stop code, bilingual name |
| `stops[].routes` | array of string | Serving route numbers, sorted |
| `routes` | array | Sorted routes |
| `routes[].route` | string | Route number, such as `505` or `614P` |
| `routes[].directions` | array | Upstream direction 1 then 2 |
| `routes[].directions[].from`, `routes[].directions[].towards` | object | Start/end `{ id, name }` |
| `routes[].directions[].stops` | array of integer | Ordered IDs; circular routes 705/706 retain the source’s two joining sections |

Caching and conditional requests follow [shared open-data behaviour](#open-data). Error: `502 upstream_unavailable` when no usable complete snapshot exists.

<a id="light-rail-fares"></a>

## 12. Light Rail fares

```http
GET /api/data/light-rail-fares
```

No path parameters.

No query parameters are read.

```bash
curl --compressed http://127.0.0.1:3000/api/data/light-rail-fares
```

From `light_rail_fares.csv`, using the same ticket categories and integer-cent amounts as heavy rail. `from` and `to` are numeric stop IDs from section 11. Trips are sorted by origin/destination and omit same-stop journeys. The historical capture had 4556 trips, roughly 810 KB JSON or 17 KB gzip.

`200 OK`, `application/json`. Preserved historical response excerpt:

```json
{
  "updated_at": "2024-06-30T01:39:03+08:00",
  "fetched_at": "2026-09-29T03:08:55+08:00",
  "stale": false,
  "fares": [
    {
      "from": 1,
      "to": 10,
      "octopus": {
        "adult": 510,
        "student": 220,
        "joyyou_sixty": 200,
        "child": 220,
        "elderly": 200,
        "disability": 200
      },
      "single_journey": { "adult": 550, "child": 300, "elderly": 300 }
    }
  ]
}
```

| Field | Type | Meaning |
| --- | --- | --- |
| `updated_at`, `fetched_at`, `stale` | metadata | Shared dataset metadata |
| `fares[].from`, `fares[].to` | integer | Origin and destination stop IDs |
| `fares` | array | Trips sorted by origin then destination; A→B and B→A are separate; same-stop trips are removed |
| `fares[].octopus.adult` | integer | Adult Octopus, cents |
| `fares[].octopus.student` | integer | Student concession Octopus |
| `fares[].octopus.joyyou_sixty` | integer | JoyYou, ages 60–64 |
| `fares[].octopus.child` | integer | Child Octopus |
| `fares[].octopus.elderly` | integer | Elderly Octopus, age 65+ |
| `fares[].octopus.disability` | integer | Disability concession |
| `fares[].single_journey.adult` | integer | Adult single journey |
| `fares[].single_journey.child` | integer | Child single journey |
| `fares[].single_journey.elderly` | integer | Elderly single journey |

Caching and conditional requests follow [shared open-data behaviour](#open-data). Error: `502 upstream_unavailable` when no usable complete snapshot exists.

<a id="accessibility"></a>

## 13. Accessibility facilities

```http
GET /api/data/accessibility
```

No path parameters.

No query parameters are read.

```bash
curl --compressed http://127.0.0.1:3000/api/data/accessibility
```

Combines the two `barrier_free_*.csv` files. Resolves station IDs, decodes HTML entities in names, retains only provided (`Y`) facilities, and trims locations while keeping embedded newlines. Rows with no provided facilities are omitted. The historical capture held four categories, 36 facilities, and 98 stations.

`200 OK`, `application/json`. Preserved historical response excerpt:

```json
{
  "updated_at": "2023-06-25T02:28:09+08:00",
  "fetched_at": "2026-09-29T03:08:55+08:00",
  "stale": false,
  "categories": [
    {
      "category": "station_access",
      "name": { "en": "System Accessibility", "tc": "出入口設施" },
      "facilities": [
        { "code": "AJ1", "name": { "en": "Same Level", "tc": "同一層" } },
        { "code": "AJ2", "name": { "en": "Ramp", "tc": "斜道" } }
      ]
    },
    {
      "category": "visually_impaired",
      "name": { "en": "Facilities for Visually Impaired", "tc": "視覺受損人士設施" },
      "facilities": [
        { "code": "VJ1", "name": { "en": "Tactile Guide Paths", "tc": "失明人士引導徑" } },
        { "code": "VJ2", "name": { "en": "Escalator Audible Warning Signals", "tc": "扶手電梯發聲提示器" } }
      ]
    }
  ],
  "stations": [
    {
      "station": { "code": "ADM", "name": { "en": "Admiralty", "tc": "金鐘" } },
      "facilities": [
        { "code": "AJ3", "location": { "en": "Exit E", "tc": "E 出口" } },
        { "code": "AJ5", "location": { "en": "Exits A & D", "tc": "A 和 D 出口" } },
        { "code": "AJ8", "location": null },
        { "code": "VJ1", "location": null }
      ]
    }
  ]
}
```

| Field | Type | Meaning |
| --- | --- | --- |
| `updated_at`, `fetched_at`, `stale` | metadata | Shared metadata; `updated_at` is the later source-file modification time |
| `categories` | array | Fixed order: `station_access`, `visually_impaired`, `hearing_impaired`, `mobility_impaired` |
| `categories[].category`, `categories[].name` | string, object | Category ID and published bilingual name; station access covers level access, ramps, lifts, and wheelchair aids |
| `categories[].facilities` | array | Catalogue order |
| `categories[].facilities[].code`, `categories[].facilities[].name` | string, object | Case-preserved code (such as `VIn1`) and bilingual facility name |
| `stations` | array | Sorted by station code |
| `stations[].station` | object | Station reference with compiled-network name |
| `stations[].facilities` | array | Provided facilities in catalogue order |
| `stations[].facilities[].code` | string | Matches the catalogue code |
| `stations[].facilities[].location` | object \| null | Bilingual free text, or null; display it without parsing structure |

Caching and conditional requests follow [shared open-data behaviour](#open-data). Error: `502 upstream_unavailable` when no usable complete snapshot exists.

<a id="mock-api"></a>

## Mock API: shared behaviour

Enable with `--mock-api` or `DUT_MOCK_API=true`. Disabled paths return `404 not_found`. Simulations perform no upstream I/O and reuse real response contracts, gzip, request IDs, and error mapping. Real routes ignore mock query parameters. Both simulated values and error responses are suitable for testing client behaviour, not for showing live departures to riders.

**Availability check.** `GET /api/mock/health` tells a client whether the mock API is enabled. It takes no parameters and returns the same empty `200` as [liveness](#health): no content type, `Cache-Control: no-store`. It adds a `x-request-id` and does no simulation. When the mock API is disabled the path is unknown, so the response is `404 not_found` (JSON); a client treats any other result as "mock unavailable".

```bash
curl -i http://127.0.0.1:3000/api/mock/health
```

```http
HTTP/1.1 200 OK
cache-control: no-store
x-request-id: fbce04e4-cfce-4880-9725-ad7fd10a5714
content-length: 0
```

| Query | Meaning |
| --- | --- |
| `scenario` | Optional, case-insensitive. A named scenario or `random`; omitted means weighted random |
| `seed` | Optional u64, 0–18446744073709551615. Named scenario defaults to 0; random without seed chooses a fresh seed each request |

A fixed seed selects the same simulated world and, with `random`, the same weighted scenario. Time still advances: repeated requests show trains approach and depart. To reproduce, use the resolved `x-mock-scenario` and `x-mock-seed` headers. Simulation responses, including simulated `502`, have these headers; parameter/path errors and the catalogue do not.

Platforms and short workings are based on network captures from 2026-10-02, including dual platforms at Airport and alternating terminus platforms. Simulation includes LOHAS Park branches, Kwun Tong short workings to Ho Man Tin, East Rail branches/short workings, and Tung Chung peak trains to Tsing Yi. Times are whole-minute offsets from `generated_at`, which lags `fetched_at` by 2–8 seconds. Delays stretch headways by 1.7, add jitter, and cancel roughly one in eight trains. These are approximations, independent of the real date.

| Line | Peak min | Off-peak min | Late-night min |
| --- | --- | --- | --- |
| AEL | 10 | 10 | 12 |
| TCL | 5 | 7.5 | 10 |
| TML | 2.8 | 4.5 | 7.5 |
| TKL | 2.3 | 4 | 6 |
| EAL | 2.8 | 5 | 7.5 |
| SIL | 3.3 | 4.5 | 7 |
| TWL | 2.1 | 3.5 | 6 |
| ISL | 2.5 | 3.5 | 6 |
| KTL | 2.1 | 3.5 | 6 |
| DRL | 6 | 8 | 10 |

<a id="mock-scenarios"></a>

## 14. Scenario catalogue

```http
GET /api/mock/scenarios
```

No path parameters.

No query parameters are read.

```bash
curl http://127.0.0.1:3000/api/mock/scenarios
```

`200 OK`, `application/json`. Example captured from the running mock service; arrays are shortened.

```json
{
  "next_trains": [
    {
      "scenario": "peak",
      "random_weight": 3,
      "description": {
        "en": "Rush hour: trains every two to three minutes on most lines, some of them turning back short of the terminus.",
        "tc": "繁忙時間：大部分綫路兩至三分鐘一班，部分班次在中途站折返。"
      }
    }
  ],
  "line_status": [
    {
      "scenario": "normal",
      "random_weight": 8,
      "description": {
        "en": "Good service on every line.",
        "tc": "所有綫路服務正常。"
      }
    }
  ]
}
```

| Field | Type | Meaning |
| --- | --- | --- |
| `next_trains`, `line_status` | array | Fixed-order lists: eleven board scenarios and nine status scenarios |
| `*[].scenario` | string | Query name |
| `*[].random_weight` | integer | Relative selection weight; zero means explicit only |
| `*[].description` | object | Bilingual `{ en, tc }` description for a developer menu |

Cache-Control is `public, max-age=86400`. No business errors when enabled; otherwise `404 not_found`.

<a id="mock-line-status"></a>

## 15. Simulated network status

```http
GET /api/mock/lines/status
```

No path parameters.

Uses optional `scenario` and `seed`; see [shared mock behaviour](#mock-api).

| scenario | Weight | Behaviour |
| --- | --- | --- |
| normal | 8 | All lines normal |
| delayed | 3 | One line delayed with a notice |
| disrupted | 1 | One line disrupted with partial suspension |
| delayed_or_disrupted | 1 | One line delayed/disrupted, displayed yellow |
| typhoon_signal | 1 | All lines under a typhoon signal, no message |
| non_service_hours | 2 | All lines outside service hours |
| unknown_condition | 0 | One unknown raw `blue` condition; display grey |
| stale | 1 | Normal data from 2–12 min ago, no-cache |
| upstream_unavailable | 1 | 502 upstream_unavailable |

```bash
curl -i 'http://127.0.0.1:3000/api/mock/lines/status?scenario=disrupted&seed=8'
```

`200 OK`, `application/json`. Example captured from the running mock service; arrays are shortened.

```json
{
  "updated_at": "2026-10-02T07:04:40+08:00",
  "fetched_at": "2026-10-02T07:50:00+08:00",
  "stale": false,
  "lines": [
    {
      "line": { "code": "TWL", "name": { "en": "Tsuen Wan Line", "tc": "荃灣綫" } },
      "color": "#FF0000",
      "condition": "normal",
      "display": "green",
      "message": null
    },
    {
      "line": { "code": "KTL", "name": { "en": "Kwun Tong Line", "tc": "觀塘綫" } },
      "color": "#1A9431",
      "condition": "disrupted",
      "display": "red",
      "message": "Due to a power supply fault, Kwun Tong Line train service between Kwun Tong and Yau Tong stations is suspended. Free shuttle buses are being arranged. Passengers are advised to use other means of transport."
    },
    {
      "line": { "code": "LR", "name": { "en": "Light Rail", "tc": "輕鐵" } },
      "color": "#9F7A00",
      "condition": "normal",
      "display": "green",
      "message": null
    }
  ]
}
```

Fields match [network status](#line-status). The seed selects an incident line, including Light Rail. `updated_at` uses 06:15 for normal/stale, 01:20 for non-service hours, or a time within the last hour otherwise. Freshness simulates 30-second polls and a maximum `max-age` of 33; stale uses `no-cache`. Errors are `400 unknown_scenario`, `400 invalid_query`, disabled-route `404 not_found`, or simulated `502 upstream_unavailable`.

```bash
curl 'http://127.0.0.1:3000/api/mock/lines/status?scenario=typhoon'
```

```json
{
  "error": {
    "code": "unknown_scenario",
    "message": "No mock scenario matches the requested name"
  }
}
```

<a id="mock-next-trains"></a>

## 16. Simulated board on one line

```http
GET /api/mock/lines/{line}/stations/{station}/next-trains
```

| Path parameter | Meaning |
| --- | --- |
| `line` | Required line code; Light Rail `LR` has no boards |
| `station` | Required known station on the requested line |

Uses optional `scenario` and `seed`; see [shared mock behaviour](#mock-api).

| scenario | Weight | Behaviour |
| --- | --- | --- |
| peak | 3 | Rush-hour service with short workings |
| off_peak | 4 | Off-peak service |
| late_night | 2 | Sparse service; LOHAS Park shuttles to Tiu Keng Leng |
| last_train | 1 | Last trains disappear as time advances; repeats every 20 min |
| non_service_hours | 1 | Directions with empty trains |
| delayed | 2 | Peak incident line with irregular, cancelled, clustered trains |
| special_arrangement | 1 | Off-peak service with a bilingual alert |
| race_day | 1 | Some East Rail trains via Racecourse instead of Fo Tan; Racecourse has no trains in other scenarios |
| stale | 1 | Off-peak data aged about 40–90 s, no-cache |
| partial_outage | 1 | One incident line unavailable |
| upstream_unavailable | 1 | All boards unavailable |

```bash
curl -i 'http://127.0.0.1:3000/api/mock/lines/EAL/stations/SHT/next-trains?scenario=peak&seed=2'
```

`200 OK`, `application/json`. Example captured from the running mock service; arrays are shortened.

```json
{
  "line": { "code": "EAL", "name": { "en": "East Rail Line", "tc": "東鐵綫" } },
  "station": { "code": "SHT", "name": { "en": "Sha Tin", "tc": "沙田" } },
  "generated_at": "2026-10-02T07:50:04+08:00",
  "fetched_at": "2026-10-02T07:50:10+08:00",
  "stale": false,
  "delayed": false,
  "alert": null,
  "directions": [
    {
      "direction": "up",
      "towards": [
        { "code": "LOW", "name": { "en": "Lo Wu", "tc": "羅湖" } },
        { "code": "LMC", "name": { "en": "Lok Ma Chau", "tc": "落馬洲" } }
      ],
      "trains": [
        {
          "destination": { "code": "LMC", "name": { "en": "Lok Ma Chau", "tc": "落馬洲" } },
          "platforms": [2],
          "arrival_at": "2026-10-02T07:52:04+08:00",
          "time_type": "arrival",
          "via_racecourse": false
        },
        {
          "destination": { "code": "LOW", "name": { "en": "Lo Wu", "tc": "羅湖" } },
          "platforms": [2],
          "arrival_at": "2026-10-02T07:55:04+08:00",
          "time_type": "arrival",
          "via_racecourse": false
        },
        {
          "destination": { "code": "LOW", "name": { "en": "Lo Wu", "tc": "羅湖" } },
          "platforms": [2],
          "arrival_at": "2026-10-02T07:58:04+08:00",
          "time_type": "arrival",
          "via_racecourse": false
        },
        {
          "destination": { "code": "TAP", "name": { "en": "Tai Po Market", "tc": "大埔墟" } },
          "platforms": [2],
          "arrival_at": "2026-10-02T08:00:04+08:00",
          "time_type": "arrival",
          "via_racecourse": false
        }
      ]
    },
    {
      "direction": "down",
      "towards": [
        { "code": "ADM", "name": { "en": "Admiralty", "tc": "金鐘" } }
      ],
      "trains": [
        {
          "destination": { "code": "ADM", "name": { "en": "Admiralty", "tc": "金鐘" } },
          "platforms": [3],
          "arrival_at": "2026-10-02T07:50:04+08:00",
          "time_type": "arrival",
          "via_racecourse": false
        }
      ]
    }
  ]
}
```

Fields match [one-line boards](#next-trains); incidents affect the requested line. East Rail uses departure at ADM, LOW, and LMC, arrival elsewhere. Freshness simulates ten-second updates, with `max-age` 2–10; stale uses `no-cache`. Invalid paths are checked before query parameters. Errors add `400 unknown_scenario` and `400 invalid_query` to the real endpoint’s errors. `partial_outage` and `upstream_unavailable` produce `502` with mock headers.

Simulated bilingual alert content:

```json
{
  "en": {
    "message": "Special train service arrangements are now in place on this line. Please click here for more information.",
    "url": "https://www.mtr.com.hk/alert/alert_title_wap.html"
  },
  "tc": {
    "message": "此綫路現正實施特別列車服務安排，詳情請按此。",
    "url": "https://www.mtr.com.hk/alert/alert_title_wap.html"
  }
}
```

```bash
curl -i 'http://127.0.0.1:3000/api/mock/lines/TKL/stations/TKO/next-trains?scenario=upstream_unavailable'
```

```json
{
  "error": {
    "code": "upstream_unavailable",
    "message": "An upstream service is unavailable"
  }
}
```

<a id="mock-station-next-trains"></a>

## 17. Simulated boards across a station

```http
GET /api/mock/stations/{station}/next-trains
```

| Path parameter | Meaning |
| --- | --- |
| `station` | Required known station code |

Uses optional `scenario` and `seed`; see [shared mock behaviour](#mock-api).

```bash
curl -i 'http://127.0.0.1:3000/api/mock/stations/ADM/next-trains?scenario=partial_outage'
```

`200 OK`, `application/json`. Example captured from the running mock service; arrays are shortened.

```json
{
  "station": { "code": "ADM", "name": { "en": "Admiralty", "tc": "金鐘" } },
  "lines": [
    {
      "line": { "code": "EAL", "name": { "en": "East Rail Line", "tc": "東鐵綫" } },
      "board": {
        "generated_at": "2026-10-02T07:49:57+08:00",
        "fetched_at": "2026-10-02T07:50:00+08:00",
        "stale": false,
        "delayed": false,
        "alert": null,
        "directions": [
          {
            "direction": "up",
            "towards": [
              { "code": "LOW", "name": { "en": "Lo Wu", "tc": "羅湖" } },
              { "code": "LMC", "name": { "en": "Lok Ma Chau", "tc": "落馬洲" } }
            ],
            "trains": [
              {
                "destination": { "code": "LOW", "name": { "en": "Lo Wu", "tc": "羅湖" } },
                "platforms": [7],
                "arrival_at": "2026-10-02T07:53:57+08:00",
                "time_type": "departure",
                "via_racecourse": false
              }
            ]
          }
        ]
      },
      "error": null
    },
    {
      "line": { "code": "TWL", "name": { "en": "Tsuen Wan Line", "tc": "荃灣綫" } },
      "board": null,
      "error": {
        "code": "upstream_unavailable",
        "message": "An upstream service is unavailable"
      }
    }
  ]
}
```

Fields, partial success, and aggregate freshness match [station boards](#station-next-trains). Scenarios are the same as section 16. The seed selects one serving line for delay, special arrangement, or partial outage. A partial outage returns `200` when another line succeeds, but `502` at a single-line station. Invalid station paths are checked before query parameters. Errors: `400 unknown_scenario`, `400 invalid_query`, `404 unknown_station`, disabled-route `404 not_found`, or `502 upstream_unavailable` when all boards fail.

<a id="mock-events"></a>

## 18. Simulated event stream

```http
GET /api/mock/events
```

No path parameters.

| Query | Meaning |
| --- | --- |
| `scenario`, `seed` | See [shared mock behaviour](#mock-api). Scenarios are the `line_status` ones from section 14 |
| `interval` | Optional whole seconds between changes, 1–60; default 5 |

```bash
curl -N 'http://127.0.0.1:3000/api/mock/events?scenario=delayed&seed=7&interval=2'
```

`200 OK`, `text/event-stream`. The stream stays open until the client disconnects; use `curl -N` or an `EventSource`. Each event has an `event:` name and one line of JSON in `data:`. Example captured from the running mock service; the two `line_status` events are one incident and its recovery, two seconds apart.

```
event: hello
data: {"server_time":"2026-10-08T01:25:49+08:00"}

event: line_status
data: {"observed_at":"2026-10-08T01:25:51+08:00","line":{"code":"DRL","name":{"en":"Disneyland Resort Line","tc":"迪士尼綫"}},"color":"#F550A6","previous":{"condition":"normal","display":"green","message":null},"current":{"condition":"delayed","display":"yellow","message":"Due to a signalling fault at Disneyland Resort Station, Disneyland Resort Line train service is delayed. Passengers please allow extra travelling time."}}

event: line_status
data: {"observed_at":"2026-10-08T01:25:53+08:00","line":{"code":"DRL","name":{"en":"Disneyland Resort Line","tc":"迪士尼綫"}},"color":"#F550A6","previous":{"condition":"delayed","display":"yellow","message":"Due to a signalling fault at Disneyland Resort Station, Disneyland Resort Line train service is delayed. Passengers please allow extra travelling time."},"current":{"condition":"normal","display":"green","message":null}}
```

The stream sends `hello` at once, then one batch of changes after each `interval`, in a cycle: the scenario's incident is reported, service recovers, and the cycle starts again. The seed picks the affected line and wording exactly as it does for the status route, so each event is the difference between two readings of section 15. A network-wide scenario (`typhoon_signal`, `non_service_hours`) sends one event per line, all in one batch. `normal`, `stale`, and `upstream_unavailable` change nothing, so the stream says `hello` and then stays silent apart from keep-alive comments.

| Event | Data field | Type | Meaning |
| --- | --- | --- | --- |
| `hello` | `server_time` | string | RFC 3339 time, sent once when the stream opens |
| `line_status` | `observed_at` | string | When the change was noticed |
| | `line` | object | `{ code, name }` of the line |
| | `color` | string | The line's colour, as in [lines](#lines) |
| | `previous`, `current` | object | The line's `condition`, `display`, and `message` before and after, in the terms of [section 2](#line-status) |

Events are hints, not a log: nothing is replayed, there is no `id`, and a client that reconnects misses what happened meanwhile. After connecting or reconnecting, fetch the current state from the status route. The real service has no event stream yet; this route fixes the shape the app can build against.

Headers: `Cache-Control: no-cache`, `X-Accel-Buffering: no` so that nginx does not hold events back, and the `x-mock-scenario` and `x-mock-seed` of the simulation. The response is never gzipped. A comment line (`:`) every 15 seconds keeps idle connections open. Errors: `400 invalid_query` for a malformed or out-of-range `interval`, `400 unknown_scenario`, or disabled-route `404 not_found`. A simulated outage never ends the stream with `502`.

<a id="errors"></a>

## Error codes

| HTTP | code | Cause |
| --- | --- | --- |
| 400 | `invalid_query` | Malformed or repeated mock query parameter, including invalid u64 seed or out-of-range `interval` |
| 400 | `unknown_scenario` | Unknown scenario for this endpoint |
| 404 | `not_found` | Unknown route or disabled mock API |
| 404 | `unknown_line` | Unknown line code |
| 404 | `unknown_station` | Malformed or unknown station code |
| 404 | `station_not_on_line` | Line does not serve station or has no Next Train support |
| 404 | `unknown_source` | Unknown source filename |
| 502 | `upstream_unavailable` | No usable fresh or stale value |

<a id="line-codes"></a>

## Appendix A: line codes

| Code | Traditional Chinese name | English | Colour | Next Train |
| --- | --- | --- | --- | --- |
| `AEL` | 機場快綫 | Airport Express | `#1C7670` | Yes |
| `TCL` | 東涌綫 | Tung Chung Line | `#FE7F1D` | Yes |
| `TML` | 屯馬綫 | Tuen Ma Line | `#9A3B26` | Yes |
| `TKL` | 將軍澳綫 | Tseung Kwan O Line | `#6B208B` | Yes |
| `EAL` | 東鐵綫 | East Rail Line | `#5EB6E4` | Yes |
| `SIL` | 南港島綫 | South Island Line | `#99CF16` | Yes |
| `TWL` | 荃灣綫 | Tsuen Wan Line | `#FF0000` | Yes |
| `ISL` | 港島綫 | Island Line | `#0860A8` | Yes |
| `KTL` | 觀塘綫 | Kwun Tong Line | `#1A9431` | Yes |
| `DRL` | 迪士尼綫 | Disneyland Resort Line | `#F550A6` | Yes |
| `LR` | 輕鐵 | Light Rail | `#9F7A00` | No; status only |

<a id="station-codes"></a>

## Appendix B: station codes

Stations are grouped in compiled line order. A shared code is the same interchange station on every line. Branch entries do not describe a single linear route; use `towards` or published route segments to navigate. `/api/lines` provides bilingual names for these codes.

| Line | Station codes and official names |
| --- | --- |
| `AEL` | `HOK` Hong Kong, `KOW` Kowloon, `TSY` Tsing Yi, `AIR` Airport, `AWE` AsiaWorld-Expo |
| `TCL` | `HOK` Hong Kong, `KOW` Kowloon, `OLY` Olympic, `NAC` Nam Cheong, `LAK` Lai King, `TSY` Tsing Yi, `SUN` Sunny Bay, `TUC` Tung Chung |
| `TML` | `WKS` Wu Kai Sha, `MOS` Ma On Shan, `HEO` Heng On, `TSH` Tai Shui Hang, `SHM` Shek Mun, `CIO` City One, `STW` Sha Tin Wai, `CKT` Che Kung Temple, `TAW` Tai Wai, `HIK` Hin Keng, `DIH` Diamond Hill, `KAT` Kai Tak, `SUW` Sung Wong Toi, `TKW` To Kwa Wan, `HOM` Ho Man Tin, `HUH` Hung Hom, `ETS` East Tsim Sha Tsui, `AUS` Austin, `NAC` Nam Cheong, `MEF` Mei Foo, `TWW` Tsuen Wan West, `KSR` Kam Sheung Road, `YUL` Yuen Long, `LOP` Long Ping, `TIS` Tin Shui Wai, `SIH` Siu Hong, `TUM` Tuen Mun |
| `TKL` | `NOP` North Point, `QUB` Quarry Bay, `YAT` Yau Tong, `TIK` Tiu Keng Leng, `TKO` Tseung Kwan O, `LHP` LOHAS Park, `HAH` Hang Hau, `POA` Po Lam |
| `EAL` | `ADM` Admiralty, `EXC` Exhibition Centre, `HUH` Hung Hom, `MKK` Mong Kok East, `KOT` Kowloon Tong, `TAW` Tai Wai, `SHT` Sha Tin, `FOT` Fo Tan, `RAC` Racecourse, `UNI` University, `TAP` Tai Po Market, `TWO` Tai Wo, `FAN` Fanling, `SHS` Sheung Shui, `LOW` Lo Wu, `LMC` Lok Ma Chau |
| `SIL` | `ADM` Admiralty, `OCP` Ocean Park, `WCH` Wong Chuk Hang, `LET` Lei Tung, `SOH` South Horizons |
| `TWL` | `CEN` Central, `ADM` Admiralty, `TST` Tsim Sha Tsui, `JOR` Jordan, `YMT` Yau Ma Tei, `MOK` Mong Kok, `PRE` Prince Edward, `SSP` Sham Shui Po, `CSW` Cheung Sha Wan, `LCK` Lai Chi Kok, `MEF` Mei Foo, `LAK` Lai King, `KWF` Kwai Fong, `KWH` Kwai Hing, `TWH` Tai Wo Hau, `TSW` Tsuen Wan |
| `ISL` | `KET` Kennedy Town, `HKU` HKU, `SYP` Sai Ying Pun, `SHW` Sheung Wan, `CEN` Central, `ADM` Admiralty, `WAC` Wan Chai, `CAB` Causeway Bay, `TIH` Tin Hau, `FOH` Fortress Hill, `NOP` North Point, `QUB` Quarry Bay, `TAK` Tai Koo, `SWH` Sai Wan Ho, `SKW` Shau Kei Wan, `HFC` Heng Fa Chuen, `CHW` Chai Wan |
| `KTL` | `WHA` Whampoa, `HOM` Ho Man Tin, `YMT` Yau Ma Tei, `MOK` Mong Kok, `PRE` Prince Edward, `SKM` Shek Kip Mei, `KOT` Kowloon Tong, `LOF` Lok Fu, `WTS` Wong Tai Sin, `DIH` Diamond Hill, `CHH` Choi Hung, `KOB` Kowloon Bay, `NTK` Ngau Tau Kok, `KWT` Kwun Tong, `LAT` Lam Tin, `YAT` Yau Tong, `TIK` Tiu Keng Leng |
| `DRL` | `SUN` Sunny Bay, `DIS` Disneyland Resort |

<a id="changelog"></a>

## Changelog

| Date | Change |
| --- | --- |
| 2026-10-10 | Added the opt-in mock route `GET /api/mock/health`: an empty `200` when the mock API is enabled, `404 not_found` otherwise. Real contracts unchanged. |
| 2026-10-08 | Added the opt-in mock route `GET /api/mock/events`: a Server-Sent Events stream of simulated `line_status` changes, with `scenario`, `seed`, and `interval`. Real contracts unchanged. |
| 2026-10-07 | Documentation rewritten in English with Cantonese and Mandarin editions. Clarified no-cache, restart behaviour, health events, and planned push configuration. No HTTP contract change. Existing captures retained. |
| 2026-10-02 | Breaking: train `platform` integer became `platforms` integer array in real and mock endpoints. Airport supports `[1, 3]` and `[2, 4]`; unreadable values use `[]` without dropping trains. Fixes Airport boards previously failing on `1/3`. |
| 2026-10-02 | Added four opt-in mock routes with scenario/seed, resolution headers, and `invalid_query` / `unknown_scenario` errors. Real contracts unchanged. |
| 2026-10-01 | Dataset/file encoding shared per fetch; maximum gzip reduces fare download from about 75 KB to 71 KB. Added Content-Length and Vary; decoded content, ETags, and caching unchanged. |
| 2026-09-29 | Added eight open-data routes, daily polls, ETags/304, integer-cent fares, gzip, and `unknown_source`. |
| 2026-09-29 | Added empty 200 health response with no-store and no request tracing. |
| 2026-09-28 | Line status switched to 30-second polling and up to 33-second freshness; fields unchanged. |
| 2026-09-28 | Removed undocumented/unimplemented 500 internal_error from the error table. |
| 2026-09-28 | Breaking: up/down arrays became directions with direction/towards/trains; terminus directions omitted, train sequence removed. Lines destinations became directions; towards lists main termini. |
| 2026-09-28 | Initial lines, status, and two Next Train routes; removed /api/hello and /api/hello.json. |
