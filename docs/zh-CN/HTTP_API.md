# HTTP API 文档

[English](../../HTTP_API.md) · [繁體粵語](../zh-HK/HTTP_API.md) · [简体中文](HTTP_API.md)

本文档是 Dut 的客户端契约。修改接口实现时，需要在同一次改动中更新 URL、方法、参数、示例、字段、缓存、错误及变更记录，并同步全部语言版本。下方实际响应保留自原有文档，属于历史示例，并非实时数据；节选仍是有效 JSON。模拟和示意示例会单独标明。

## 目录

- [概览](#overview)
- [通用约定](#conventions)
- [1. 线路与车站](#lines)
- [2. 全网服务状态](#line-status)
- [3. 单线列车到站](#next-trains)
- [4. 全站列车到站](#station-next-trains)
- [5. 存活检查](#health)
- [开放数据：共同行为](#open-data)
- [6. 开放数据索引](#data-index)
- [7. 原始文件](#source-files)
- [8. 公开车站与路线](#published-stations)
- [9. 重铁车费](#fares)
- [10. 机场快线车费](#airport-express-fares)
- [11. 轻铁车站与路线](#light-rail)
- [12. 轻铁车费](#light-rail-fares)
- [13. 无障碍设施](#accessibility)
- [Mock API：共同行为](#mock-api)
- [14. 场景目录](#mock-scenarios)
- [15. 模拟全网状态](#mock-line-status)
- [16. 模拟单线到站板](#mock-next-trains)
- [17. 模拟全站到站板](#mock-station-next-trains)
- [错误码](#errors)
- [附录 A：线路代码](#line-codes)
- [附录 B：车站代码](#station-codes)
- [变更记录](#changelog)

<a id="overview"></a>

## 概览

本地 base URL 为 `http://127.0.0.1:3000`。全部路由位于 `/api` 下，使用 `GET`；健康检查也支持 `HEAD`。目前没有鉴权。JSON 响应使用 UTF-8 `application/json`；原始文件使用 `text/csv; charset=utf-8`。健康检查没有 body 或 content type。gzip 通过 `Accept-Encoding` 协商。Mock 路由需要通过 `--mock-api` 或 `DUT_MOCK_API` 启用。

| 方法 | URL | 用途 |
| --- | --- | --- |
| `GET` | `/api/lines` | 编译内置的线路、车站、颜色及方向 |
| `GET` | `/api/lines/status` | 全网服务状态，包括轻铁 |
| `GET` | `/api/lines/{line}/stations/{station}/next-trains` | 某站某条线的列车到站 |
| `GET` | `/api/stations/{station}/next-trains` | 某站所有线路的列车到站 |
| `GET` | `/api/health` | 存活检查；空的 200 响应 |
| `GET` | `/api/data` | 数据集和原始文件版本 |
| `GET` | `/api/data/sources/{file}` | 港铁原始 CSV 字节 |
| `GET` | `/api/data/stations` | 公开车站和路线资料 |
| `GET` | `/api/data/fares` | 重铁车费，单位为港仙 |
| `GET` | `/api/data/airport-express-fares` | 机场快线车费 |
| `GET` | `/api/data/light-rail` | 轻铁车站和路线 |
| `GET` | `/api/data/light-rail-fares` | 轻铁车费 |
| `GET` | `/api/data/accessibility` | 设施目录和各站设施 |
| `GET` | `/api/mock/scenarios` | 可用模拟场景；需要先启用 |
| `GET` | `/api/mock/lines/status` | 模拟服务状态 |
| `GET` | `/api/mock/lines/{line}/stations/{station}/next-trains` | 模拟单线到站板 |
| `GET` | `/api/mock/stations/{station}/next-trains` | 模拟全站到站板 |

<a id="conventions"></a>

## 通用约定

- **时间：** RFC 3339，香港时差 `+08:00`，精确到秒。倒计时使用 `arrival_at - 设备时间`，不要使用 `generated_at` 或 `fetched_at`；上游 `ttnt` 已丢弃。
- **代码：** 路径中的线路和车站代码不区分大小写；响应统一大写。
- **名称：** `{ "en": "...", "tc": "..." }` 一次提供英文和繁体中文。文档语言不会改变 JSON 契约。
- **金额：** 整数港仙，`490` 即 HK$4.90；显示时除以 100。
- **方向：** `up` 和 `down` 是港铁稳定 ID，不是罗盘方向。月台标题使用 `towards`，每班实际终点使用 `destination`；中途折返仍属于同一方向。迪士尼线 `up` 为往欣澳，支线也不能仅靠一份平面车站列表推断方向。

| Header／字段 | 含义 |
| --- | --- |
| `Cache-Control: public, max-age=N` | 剩余新鲜期秒数，可作为轮询提示 |
| `Cache-Control: no-cache` | 复用前必须验证；仍可存储 |
| `stale` | 数据已过新鲜期，在回退窗口内返回 |
| `generated_at` / `updated_at` | 上游生成／发布时间；发布时间早不代表数据过时 |
| `fetched_at` | Dut 最近成功获取时间 |
| `ETag` | 数据集／文件验证值，下次放入 `If-None-Match` |
| `x-request-id` | 被追踪的路由生成 UUID 或沿用客户端 header；健康检查没有 |

应用错误使用下面的 JSON，`code` 稳定，`message` 为英文。错误不带 `Cache-Control`。上游细节仅记录在服务端日志中。查询错误不回显提交的代码；`not_found` 消息会包含未匹配路径。

```json
{
  "error": {
    "code": "unknown_station",
    "message": "No station matches the requested station code"
  }
}
```

<a id="lines"></a>

## 1. 线路与车站

```http
GET /api/lines
```

无路径参数。

不读取查询参数。

按内置顺序返回十条 Next Train 线路：AEL、TCL、TML、TKL、EAL、SIL、TWL、ISL、KTL、DRL。导航数据可下载一次使用，仅随部署更新。`Cache-Control: public, max-age=86400`；无业务错误。

```bash
curl http://127.0.0.1:3000/api/lines
```

`200 OK`，`application/json`。保留的历史响应节选：

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

| 字段 | 类型 | 含义 |
| --- | --- | --- |
| `lines` | array | 有序线路 |
| `lines[].code`, `lines[].name` | string, object | 线路代码及双语名称 |
| `lines[].color` | string | 品牌色 `#RRGGBB` |
| `lines[].stations` | array | 按线路顺序排列的车站引用 |
| `lines[].directions` | array | 先 `up` 后 `down`，各含 `direction` 和车站引用数组 `towards` |

<a id="station-reference"></a>

### 车站引用

| 字段 | 类型 | 含义 |
| --- | --- | --- |
| `code` | string | 三个大写字母 |
| `name` | object \| null | `{ en, tc }`；仅当上游返回内置路网未知车站时为 null |

<a id="line-status"></a>

## 2. 全网服务状态

```http
GET /api/lines/status
```

无路径参数。

不读取查询参数。

```bash
curl http://127.0.0.1:3000/api/lines/status
```

包含十一条线，轻铁位于上游顺序最后。`condition` 用于客户端逻辑，`display` 遵循港铁网站展示。

`200 OK`，`application/json`。保留的历史响应节选：

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

| 字段 | 类型 | 含义 |
| --- | --- | --- |
| `updated_at`, `fetched_at`, `stale` | string, string, boolean | 发布时间、获取时间及新鲜度标记 |
| `lines` | array | 上游线路顺序 |
| `lines[].line`, `lines[].color` | object, string | 线路 `{ code, name }` 及品牌色 |
| `lines[].condition`, `lines[].display` | string | 下表的语义状态及展示值 |
| `lines[].message` | string \| null | 上游提供的说明，可为空 |

| 上游 | condition | display | 含义 |
| --- | --- | --- | --- |
| green | `normal` | green | 服务正常 |
| yellow | `delayed` | yellow | 延误或逐步恢复 |
| red | `disrupted` | red | 服务受阻，考虑其他交通 |
| pink | `delayed_or_disrupted` | yellow | 延误或受阻 |
| grey | `non_service_hours` | grey | 非服务时间 |
| typhoon | `typhoon_signal` | typhoon | 热带气旋信号 |
| 其他 | `unknown` | grey | 未知状态，记录 warn 日志 |

延误线路元素示意，并非实际抓取：

```json
{
  "line": { "code": "KTL", "name": { "en": "Kwun Tong Line", "tc": "觀塘綫" } },
  "color": "#1A9431",
  "condition": "delayed_or_disrupted",
  "display": "yellow",
  "message": "Trains are delayed"
}
```

每 30 秒轮询；成功获取起算新鲜期 33 秒，`max-age` 最多为 33，随数据年龄减少。请求读取轮询结果，启动时等待首次轮询。过期值可再顶替 15 分钟，使用 `stale: true` 和 `no-cache`。无可用快照则返回 `502 upstream_unavailable`。

<a id="next-trains"></a>

## 3. 单线列车到站

```http
GET /api/lines/{line}/stations/{station}/next-trains
```

| 路径参数 | 含义 |
| --- | --- |
| `line` | 必填线路代码；轻铁 `LR` 没有到站板 |
| `station` | 必填，必须是指定线路上的已知车站 |

不读取查询参数。

```bash
curl http://127.0.0.1:3000/api/lines/TKL/stations/TKO/next-trains
```

每个方向最多四班车，超过 30 秒离站宽限期的列车会被过滤。终点方向不列出，除非上游仍报告列车。非服务时间，有效方向可以没有列车。`towards` 列出主要可达终点，中途折返查看每班的 `destination`。

`200 OK`，`application/json`。保留的历史响应节选：

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

| 字段 | 类型 | 含义 |
| --- | --- | --- |
| `line`, `station` | object | 线路及车站引用 |
| `generated_at`, `fetched_at` | string | 上游生成及 Dut 获取时间 |
| `stale`, `delayed` | boolean | 旧数据回退；港铁延误标记，两者独立 |
| `alert` | object \| null | `{ en: notice, tc: notice }`，各通告含 `message` 和可空 `url` |
| `directions` | array | 可乘车方向，`up` 在前 |
| `directions[].direction` | string | `up` 或 `down` |
| `directions[].towards` | array | 主要终点引用；已过最后主要终点但上游仍报告列车时可以为空 |
| `directions[].trains` | array | 按到站时间排序，最多四班 |
| `directions[].trains[].destination` | object | 列车实际终点引用 |
| `directions[].trains[].platforms` | array of integer | 按上游顺序的月台，通常 `[1]`；机场往博览馆为 `[1, 3]`，往香港为 `[2, 4]`。无法识别则为 `[]`，保留列车 |
| `directions[].trains[].arrival_at` | string | 预计到站，`time_type=departure` 时为开出时间；用于倒计时 |
| `directions[].trains[].time_type` | string \| null | 东铁：`arrival` 或 `departure`；其他线路 null |
| `directions[].trains[].via_racecourse` | boolean | 东铁经马场而非火炭；其他线路为 false |

另保留宝琳开出方向、一班东铁列车及 2026-10-02 机场站一班机场快线的节选。第一个对象仅含 `directions`，并非完整响应。

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

特别安排通告示意，并非实际抓取：

```json
{
  "alert": {
    "en": { "message": "Special train service arrangement", "url": "https://example.com/notice" },
    "tc": { "message": "特別列車服務安排", "url": "https://example.com/notice" }
  }
}
```

新鲜期遵循 CDN 剩余 TTL，通常 10 秒，限制在 2–15 秒。到站板过期后等待一次合并刷新。刷新失败可在过期后 90 秒内返回旧数据，标记 `stale: true`；重试退避 5 秒。错误包括 `404 unknown_line`、`404 unknown_station`、`404 station_not_on_line`（含 `LR`）；无可用板则为 `502 upstream_unavailable`。

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

## 4. 全站列车到站

```http
GET /api/stations/{station}/next-trains
```

| 路径参数 | 含义 |
| --- | --- |
| `station` | 必填已知车站代码，无需线路参数 |

不读取查询参数。

```bash
curl http://127.0.0.1:3000/api/stations/ADM/next-trains
```

并发查询各途经线路，使用各自缓存。一条线路成功即返回 `200`；失败线路保留为 `board: null` 并附错误。金钟包含 EAL、SIL、TWL 和 ISL。下面节选 EAL 及 TWL 末班车开出后的数据。

`200 OK`，`application/json`。保留的历史响应节选：

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

| 字段 | 类型 | 含义 |
| --- | --- | --- |
| `station` | object | 车站引用 |
| `lines` | array | 按 `/api/lines` 顺序的途经线路 |
| `lines[].line` | object | `{ code, name }` |
| `lines[].board` | object \| null | 第 3 节字段，但不含 `line` 和 `station`；失败为 null |
| `lines[].error` | object \| null | `{ code, message }`；成功为 null |

失败线路元素示意：

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

响应采用成功到站板中最短的剩余新鲜期。任何成功板为旧数据就使用 `no-cache`。错误：`404 unknown_station`；仅当全部线路失败时返回 `502 upstream_unavailable`。

<a id="health"></a>

## 5. 存活检查

```http
GET /api/health
```

无路径参数。

不读取查询参数。

```bash
curl -i http://127.0.0.1:3000/api/health
```

`GET` 和 `HEAD` 返回 `200`、空 body，无 content type，并带 `Cache-Control: no-store`。不访问上游，不记录请求日志，也不生成请求 ID。此接口检查 HTTP 进程，并非上游可用性。无业务错误；无法连接服务时为连接层失败。

```http
HTTP/1.1 200 OK
cache-control: no-store
content-length: 0
```

<a id="open-data"></a>

## 开放数据：共同行为

第 6–13 节读取同一份内存快照，来自七个港铁 CSV。启动后立即首次轮询，之后每 24 小时一次，失败在 5 分钟后重试。全部成功才发布。读取可能等待首次轮询；请求不会触发平台获取。获取起算新鲜期 86430 秒，之后有 30 天旧数据窗口，再之后返回 `502 upstream_unavailable`。新进程没有旧快照。

Local First 同步可请求 `/api/data`，与本地版本比较，仅下载变化的数据集。数据集弱 ETag 包含服务版本和内容 hash，例如 `W/"0.5.1-5987f5c9680ce780"`；原始文件仅包含字节 hash。下次发送 `If-None-Match`，未变化则返回空的 `304`，带 ETag 及缓存 header。索引本身没有 ETag。版本仅用于比较相等，不应视为时间戳或可排序版本。

```bash
curl -i -H 'If-None-Match: W/"0.5.1-5987f5c9680ce780"' http://127.0.0.1:3000/api/data/fares
```

```http
HTTP/1.1 304 Not Modified
etag: W/"0.5.1-5987f5c9680ce780"
cache-control: public, max-age=86000
```

上方 304 header 为条件请求契约示意，剩余新鲜期取决于请求时间。新鲜响应使用 `public, max-age=N`，旧数据使用 `no-cache`。每份获取状态的数据集／文件共用未压缩及最高级别 gzip 编码，带 `Content-Length` 和 `Vary: Accept-Encoding`。首次请求承担编码成本，之后复用字节。

| 字段 | 类型 | 含义 |
| --- | --- | --- |
| `updated_at` | string \| null | 数据集来源 `Last-Modified`；可能是数月前或未提供，不代表快照过时 |
| `fetched_at` | string | 最近成功完整轮询 |
| `stale` | boolean | 快照已过新鲜期 |

<a id="data-index"></a>

## 6. 开放数据索引

```http
GET /api/data
```

无路径参数。

不读取查询参数。

```bash
curl http://127.0.0.1:3000/api/data
```

`200 OK`，`application/json`。保留的历史响应节选：

```json
{
  "fetched_at": "2026-09-29T03:08:55+08:00",
  "stale": false,
  "datasets": [
    {
      "name": "stations",
      "path": "/api/data/stations",
      "revision": "0.5.1-0d1201f5804cd4cd",
      "updated_at": "2023-11-21T18:09:07+08:00"
    },
    {
      "name": "fares",
      "path": "/api/data/fares",
      "revision": "0.5.1-5987f5c9680ce780",
      "updated_at": "2026-04-03T01:02:50+08:00"
    },
    {
      "name": "airport-express-fares",
      "path": "/api/data/airport-express-fares",
      "revision": "0.5.1-545393302ce4c18f",
      "updated_at": "2025-06-22T01:05:16+08:00"
    },
    {
      "name": "light-rail",
      "path": "/api/data/light-rail",
      "revision": "0.5.1-97a676332b659216",
      "updated_at": "2026-07-05T00:58:02+08:00"
    },
    {
      "name": "light-rail-fares",
      "path": "/api/data/light-rail-fares",
      "revision": "0.5.1-e9deb534253c3bde",
      "updated_at": "2024-06-30T01:39:03+08:00"
    },
    {
      "name": "accessibility",
      "path": "/api/data/accessibility",
      "revision": "0.5.1-b199444f08da98bf",
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

| 字段 | 类型 | 含义 |
| --- | --- | --- |
| `fetched_at`, `stale` | string, boolean | 快照元信息；无顶层 `updated_at` |
| `datasets`, `sources` | array | 固定顺序的清洗数据集及七个原始文件 |
| `datasets[].name`, `datasets[].path` | string | 数据集名称及下载路径；名称对应六个 URL 后缀 |
| `datasets[].revision`, `sources[].revision` | string | ETag 去除 `W/` 及引号的值 |
| `datasets[].updated_at`, `sources[].updated_at` | string \| null | 上游修改时间 |
| `sources[].file`, `sources[].path` | string | 原始文件名及下载路径 |
| `sources[].bytes` | integer | 未压缩字节数 |

索引没有 ETag，不返回 `304`；使用其版本验证各项下载。

缓存及条件请求遵循[开放数据共同行为](#open-data)。无可用完整快照时返回 `502 upstream_unavailable`。

<a id="source-files"></a>

## 7. 原始文件

```http
GET /api/data/sources/{file}
```

| 路径参数 | 含义 |
| --- | --- |
| `file` | 必填，仅接受下列文件名，不区分大小写 |

不读取查询参数。

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

`200 OK`，`text/csv; charset=utf-8`。解码后的 body 保留全部上游字节，包括 BOM、CRLF 和原有错字。下方历史节选显示标题及前四行，完整未压缩 body 当时为 691 bytes。

```csv
ST_FROM,ST_FROM_ID,ST_TO,ST_TO_ID,OCT_ADT_FARE,OCT_CHD_FARE,SINGLE_ADT_FARE,SINGLE_CHD_FARE
HongKong,44,Airport,47,120,60,130,65
HongKong,44,AsiaWorld-Expo,56,120,60,130,65
Kowloon,45,Airport,47,105,52.5,115,57.5
Kowloon,45,AsiaWorld-Expo,56,105,52.5,115,57.5
```

列定义见港铁平台的来源文件说明。需要标准化字段契约时使用下面的 JSON。原始文件 ETag 仅随字节变化，不受服务升级影响。未知文件名返回 `404 unknown_source`。

缓存及条件请求遵循[开放数据共同行为](#open-data)。无可用完整快照时返回 `502 upstream_unavailable`。

<a id="published-stations"></a>

## 8. 公开车站与路线

```http
GET /api/data/stations
```

无路径参数。

不读取查询参数。

```bash
curl http://127.0.0.1:3000/api/data/stations
```

来自 `mtr_lines_and_stations.csv`，反映公开数据，区别于 `/api/lines` 的内置路网。保留样本有 97 站、十条线，没有马场。ID 转为车站代码，站名去多余空白，`茘` 统一为 `荔`，移除空行，支线使用 `from`／`towards` 而非 `LMC-UT` 等内部名称。

`200 OK`，`application/json`。保留的历史响应节选：

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

| 字段 | 类型 | 含义 |
| --- | --- | --- |
| `updated_at`, `fetched_at`, `stale` | metadata | 共用数据集元信息 |
| `stations` | array | 按车站代码排序 |
| `stations[].code`, `stations[].name`, `stations[].lines` | string, object, array of string | 代码、公开双语名称，途经线路按内置线序 |
| `lines[].code`, `lines[].name` | string, object | 线路代码及双语名称，无公开路线数据的线路不列出 |
| `lines[].routes` | array | 先 `up` 后 `down`，先主线后支线 |
| `lines[].routes[].direction` | string | `up`／`down`；迪士尼 `up` 为迪士尼往欣澳 |
| `lines[].routes[].from`, `lines[].routes[].towards` | object | 起点及终点车站引用 |
| `lines[].routes[].stations` | array of string | 依序停站；支线可能仅列支线段，如调景岭往康城 |

缓存及条件请求遵循[开放数据共同行为](#open-data)。无可用完整快照时返回 `502 upstream_unavailable`。

<a id="fares"></a>

## 9. 重铁车费

```http
GET /api/data/fares
```

无路径参数。

不读取查询参数。

```bash
curl --compressed http://127.0.0.1:3000/api/data/fares
```

来自 `mtr_lines_fares.csv`，不含机场快线。ID 转为车站代码，马场按名称对应 `RAC`。历史完整响应有 9120 个行程，约 1.7 MB JSON／71 KB gzip；完整下载建议启用压缩。

`200 OK`，`application/json`。保留的历史响应节选：

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

| 字段 | 类型 | 含义 |
| --- | --- | --- |
| `updated_at`, `fetched_at`, `stale` | metadata | 共用数据集元信息 |
| `fares[].from`, `fares[].to` | string | 起点及终点车站代码 |
| `fares` | array | 按起点再终点排序；A→B 和 B→A 分开，同站行程移除 |
| `fares[].octopus.adult` | integer | 成人八达通，港仙 |
| `fares[].octopus.student` | integer | 学生乘车优惠八达通 |
| `fares[].octopus.joyyou_sixty` | integer | 乐悠咭，60–64 岁 |
| `fares[].octopus.child` | integer | 小童八达通 |
| `fares[].octopus.elderly` | integer | 长者八达通，65 岁及以上 |
| `fares[].octopus.disability` | integer | 残疾人士优惠 |
| `fares[].single_journey.adult` | integer | 成人单程票 |
| `fares[].single_journey.child` | integer | 小童单程票 |
| `fares[].single_journey.elderly` | integer | 长者单程票 |

缓存及条件请求遵循[开放数据共同行为](#open-data)。无可用完整快照时返回 `502 upstream_unavailable`。

<a id="airport-express-fares"></a>

## 10. 机场快线车费

```http
GET /api/data/airport-express-fares
```

无路径参数。

不读取查询参数。

```bash
curl http://127.0.0.1:3000/api/data/airport-express-fares
```

来自 `airport_express_fares.csv`，金额均为港仙。上游独立 ID 44–46 对应 HOK、KOW 和 TSY。历史完整样本包含 14 个行程。

`200 OK`，`application/json`。保留的历史响应节选：

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

| 字段 | 类型 | 含义 |
| --- | --- | --- |
| `updated_at`, `fetched_at`, `stale` | metadata | 共用数据集元信息 |
| `fares` | array | 按起点再终点排序 |
| `fares[].from`, `fares[].to` | string | 起点及终点代码 |
| `fares[].octopus.adult`, `fares[].octopus.child` | integer | 成人及小童八达通港仙 |
| `fares[].single_journey.adult`, `fares[].single_journey.child` | integer | 成人及小童单程票港仙 |

缓存及条件请求遵循[开放数据共同行为](#open-data)。无可用完整快照时返回 `502 upstream_unavailable`。

<a id="light-rail"></a>

## 11. 轻铁车站与路线

```http
GET /api/data/light-rail
```

无路径参数。

不读取查询参数。

```bash
curl http://127.0.0.1:3000/api/data/light-rail
```

来自 `light_rail_routes_and_stops.csv`。车站 ID 为数字，对应港铁轻铁到站 API；三字母代码与重铁属于不同编号空间。路线按数字及后缀排序（`614` 在 `614P` 前），合并折返处连续重复的车站。保留样本包含 68 站及十一条路线。

`200 OK`，`application/json`。保留的历史响应节选：

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

| 字段 | 类型 | 含义 |
| --- | --- | --- |
| `updated_at`, `fetched_at`, `stale` | metadata | 共用数据集元信息 |
| `stops` | array | 按车站 ID 排序 |
| `stops[].id`, `stops[].code`, `stops[].name` | integer, string, object | 数字 ID、车站代码及双语名称 |
| `stops[].routes` | array of string | 途经路线号，已排序 |
| `routes` | array | 已排序路线 |
| `routes[].route` | string | 路线号，如 `505` 或 `614P` |
| `routes[].directions` | array | 上游方向 1 再 2 |
| `routes[].directions[].from`, `routes[].directions[].towards` | object | 起点／终点 `{ id, name }` |
| `routes[].directions[].stops` | array of integer | 有序 ID；循环线 705／706 保留来源中两段相接的形式 |

缓存及条件请求遵循[开放数据共同行为](#open-data)。无可用完整快照时返回 `502 upstream_unavailable`。

<a id="light-rail-fares"></a>

## 12. 轻铁车费

```http
GET /api/data/light-rail-fares
```

无路径参数。

不读取查询参数。

```bash
curl --compressed http://127.0.0.1:3000/api/data/light-rail-fares
```

来自 `light_rail_fares.csv`，票种及整数港仙与重铁一致。`from` 和 `to` 是第 11 节的数字车站 ID。按起点／终点排序，移除同站行程。历史样本有 4556 个行程，约 810 KB JSON／17 KB gzip。

`200 OK`，`application/json`。保留的历史响应节选：

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

| 字段 | 类型 | 含义 |
| --- | --- | --- |
| `updated_at`, `fetched_at`, `stale` | metadata | 共用数据集元信息 |
| `fares[].from`, `fares[].to` | integer | 起点及终点 ID |
| `fares` | array | 按起点再终点排序；A→B 和 B→A 分开，同站行程移除 |
| `fares[].octopus.adult` | integer | 成人八达通，港仙 |
| `fares[].octopus.student` | integer | 学生乘车优惠八达通 |
| `fares[].octopus.joyyou_sixty` | integer | 乐悠咭，60–64 岁 |
| `fares[].octopus.child` | integer | 小童八达通 |
| `fares[].octopus.elderly` | integer | 长者八达通，65 岁及以上 |
| `fares[].octopus.disability` | integer | 残疾人士优惠 |
| `fares[].single_journey.adult` | integer | 成人单程票 |
| `fares[].single_journey.child` | integer | 小童单程票 |
| `fares[].single_journey.elderly` | integer | 长者单程票 |

缓存及条件请求遵循[开放数据共同行为](#open-data)。无可用完整快照时返回 `502 upstream_unavailable`。

<a id="accessibility"></a>

## 13. 无障碍设施

```http
GET /api/data/accessibility
```

无路径参数。

不读取查询参数。

```bash
curl --compressed http://127.0.0.1:3000/api/data/accessibility
```

合并两个 `barrier_free_*.csv`，转换车站 ID，解码名称中的 HTML entity，仅保留已提供（`Y`）的设施，位置去多余空白但保留换行。未提供任何设施的行被省略。历史样本有四类、36 种设施及 98 站。

`200 OK`，`application/json`。保留的历史响应节选：

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

| 字段 | 类型 | 含义 |
| --- | --- | --- |
| `updated_at`, `fetched_at`, `stale` | metadata | 共用元信息；`updated_at` 取两个文件中较晚的修改时间 |
| `categories` | array | 固定顺序：`station_access`、`visually_impaired`、`hearing_impaired`、`mobility_impaired` |
| `categories[].category`, `categories[].name` | string, object | 类别 ID 及公开双语名称；出入口类包含同层、坡道、升降机及轮椅辅助设施 |
| `categories[].facilities` | array | 目录顺序 |
| `categories[].facilities[].code`, `categories[].facilities[].name` | string, object | 保留大小写的代码（如 `VIn1`）及双语设施名称 |
| `stations` | array | 按车站代码排序 |
| `stations[].station` | object | 车站引用，名称来自内置路网 |
| `stations[].facilities` | array | 按目录顺序列出已提供设施 |
| `stations[].facilities[].code` | string | 对应目录代码 |
| `stations[].facilities[].location` | object \| null | 双语自由文本或 null；直接显示，不解析结构 |

缓存及条件请求遵循[开放数据共同行为](#open-data)。无可用完整快照时返回 `502 upstream_unavailable`。

<a id="mock-api"></a>

## Mock API：共同行为

通过 `--mock-api` 或 `DUT_MOCK_API=true` 启用，未启用的路径返回 `404 not_found`。模拟不访问上游，共用正式响应契约、gzip、请求 ID 及错误映射。正式路由忽略 Mock 查询参数。模拟数据和错误适合测试客户端行为，并非乘客实时班次。

| 查询 | 含义 |
| --- | --- |
| `scenario` | 可选，不区分大小写，场景名或 `random`；不传则按权重随机 |
| `seed` | 可选 u64，0–18446744073709551615。指定场景默认 0；random 无 seed 时每次生成新 seed |

固定 seed 对应同一个模拟世界，配合 `random` 也会选择同一场景。时间仍会推进，重复请求可看到列车到站和开出。复现时使用响应中的 `x-mock-scenario` 与 `x-mock-seed`。模拟响应（含模拟 `502`）均带这些 header；参数／路径错误及目录不带。

月台及中途折返参考 2026-10-02 全网样本，包括机场双月台及终点交替月台。模拟含康城支线、观塘线往何文田折返、东铁支线／折返及东涌线繁忙时段往青衣班次。时间为 `generated_at` 加整数分钟，它比 `fetched_at` 早 2–8 秒。延误使班距拉长 1.7 倍、增加偏差，约每八班取消一班。这些是近似值，不依赖真实日期。

| 线路 | 繁忙分钟 | 非繁忙分钟 | 深夜分钟 |
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

## 14. 场景目录

```http
GET /api/mock/scenarios
```

无路径参数。

不读取查询参数。

```bash
curl http://127.0.0.1:3000/api/mock/scenarios
```

`200 OK`，`application/json`。示例取自运行中的 Mock 服务，数组已节选。

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

| 字段 | 类型 | 含义 |
| --- | --- | --- |
| `next_trains`, `line_status` | array | 固定顺序：十一个到站场景、九个状态场景 |
| `*[].scenario` | string | 查询名称 |
| `*[].random_weight` | integer | 相对抽选权重，零表示仅可指定 |
| `*[].description` | object | 开发菜单使用的双语 `{ en, tc }` 说明 |

Cache-Control 为 `public, max-age=86400`。启用后无业务错误，否则为 `404 not_found`。

<a id="mock-line-status"></a>

## 15. 模拟全网状态

```http
GET /api/mock/lines/status
```

无路径参数。

接受可选 `scenario` 和 `seed`，见 [Mock 共同行为](#mock-api)。

| scenario | 权重 | 行为 |
| --- | --- | --- |
| normal | 8 | 全线正常 |
| delayed | 3 | 一线延误，附通告 |
| disrupted | 1 | 一线受阻，部分区间暂停 |
| delayed_or_disrupted | 1 | 一线延误／受阻，显示黄色 |
| typhoon_signal | 1 | 全线台风信号状态，无消息 |
| non_service_hours | 2 | 全线非服务时间 |
| unknown_condition | 0 | 一线未知原值 `blue`，显示灰色 |
| stale | 1 | 正常数据，2–12 分钟前，no-cache |
| upstream_unavailable | 1 | 502 upstream_unavailable |

```bash
curl -i 'http://127.0.0.1:3000/api/mock/lines/status?scenario=disrupted&seed=8'
```

`200 OK`，`application/json`。示例取自运行中的 Mock 服务，数组已节选。

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

字段与[全网状态](#line-status)一致。Seed 选择事故线，包含轻铁。`updated_at` 正常／stale 使用 06:15，非服务时间使用 01:20，其余为最近一小时。新鲜度模拟 30 秒轮询，`max-age` 最多 33，stale 使用 `no-cache`。错误为 `400 unknown_scenario`、`400 invalid_query`、未启用时的 `404 not_found` 或模拟 `502 upstream_unavailable`。

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

## 16. 模拟单线到站板

```http
GET /api/mock/lines/{line}/stations/{station}/next-trains
```

| 路径参数 | 含义 |
| --- | --- |
| `line` | 必填线路代码；轻铁 `LR` 没有到站板 |
| `station` | 必填，必须是指定线路上的已知车站 |

接受可选 `scenario` 和 `seed`，见 [Mock 共同行为](#mock-api)。

| scenario | 权重 | 行为 |
| --- | --- | --- |
| peak | 3 | 繁忙班次，含中途折返 |
| off_peak | 4 | 非繁忙班次 |
| late_night | 2 | 深夜疏班；康城仅往返调景岭 |
| last_train | 1 | 末班车逐班开出，每 20 分钟重演 |
| non_service_hours | 1 | 方向保留，列车数组为空 |
| delayed | 2 | 繁忙事故线有脱班、取消及集中到站 |
| special_arrangement | 1 | 非繁忙班次，附双语通告 |
| race_day | 1 | 部分东铁经马场而非火炭；其他场景马场无车 |
| stale | 1 | 非繁忙数据约 40–90 秒前，no-cache |
| partial_outage | 1 | 一条事故线不可用 |
| upstream_unavailable | 1 | 全部到站板不可用 |

```bash
curl -i 'http://127.0.0.1:3000/api/mock/lines/EAL/stations/SHT/next-trains?scenario=peak&seed=2'
```

`200 OK`，`application/json`。示例取自运行中的 Mock 服务，数组已节选。

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

字段与[单线板](#next-trains)一致，事故影响指定线路。东铁在 ADM、LOW 和 LMC 使用 departure，其余使用 arrival。新鲜度模拟十秒更新，`max-age` 为 2–10；stale 使用 `no-cache`。先验证路径，再验证查询参数。正式错误之外增加 `400 unknown_scenario` 和 `400 invalid_query`。`partial_outage` 及 `upstream_unavailable` 返回 `502`，带 Mock header。

模拟双语通告内容：

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

## 17. 模拟全站到站板

```http
GET /api/mock/stations/{station}/next-trains
```

| 路径参数 | 含义 |
| --- | --- |
| `station` | 必填已知车站代码 |

接受可选 `scenario` 和 `seed`，见 [Mock 共同行为](#mock-api)。

```bash
curl -i 'http://127.0.0.1:3000/api/mock/stations/ADM/next-trains?scenario=partial_outage'
```

`200 OK`，`application/json`。示例取自运行中的 Mock 服务，数组已节选。

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

字段、部分成功及聚合新鲜度遵循[全站板](#station-next-trains)。场景与第 16 节一致。Seed 选择一条途经线进行延误、特别安排或部分故障。部分故障有其他线成功则为 `200`，单线站则为 `502`。先验证车站路径，再验证查询参数。错误：`400 unknown_scenario`、`400 invalid_query`、`404 unknown_station`、未启用时的 `404 not_found`，全部失败时为 `502 upstream_unavailable`。

<a id="errors"></a>

## 错误码

| HTTP | code | 原因 |
| --- | --- | --- |
| 400 | `invalid_query` | Mock 参数格式错误或重复，含无效 u64 seed |
| 400 | `unknown_scenario` | 该接口未知的场景 |
| 404 | `not_found` | 未知路由或 Mock 未启用 |
| 404 | `unknown_line` | 未知线路代码 |
| 404 | `unknown_station` | 格式错误或未知车站代码 |
| 404 | `station_not_on_line` | 线路不经过本站或不支持 Next Train |
| 404 | `unknown_source` | 未知来源文件名 |
| 502 | `upstream_unavailable` | 无可用新鲜或旧数据 |

<a id="line-codes"></a>

## 附录 A：线路代码

| 代码 | 繁体中文名 | English | 颜色 | 列车到站 |
| --- | --- | --- | --- | --- |
| `AEL` | 機場快綫 | Airport Express | `#1C7670` | 有 |
| `TCL` | 東涌綫 | Tung Chung Line | `#FE7F1D` | 有 |
| `TML` | 屯馬綫 | Tuen Ma Line | `#9A3B26` | 有 |
| `TKL` | 將軍澳綫 | Tseung Kwan O Line | `#6B208B` | 有 |
| `EAL` | 東鐵綫 | East Rail Line | `#5EB6E4` | 有 |
| `SIL` | 南港島綫 | South Island Line | `#99CF16` | 有 |
| `TWL` | 荃灣綫 | Tsuen Wan Line | `#FF0000` | 有 |
| `ISL` | 港島綫 | Island Line | `#0860A8` | 有 |
| `KTL` | 觀塘綫 | Kwun Tong Line | `#1A9431` | 有 |
| `DRL` | 迪士尼綫 | Disneyland Resort Line | `#F550A6` | 有 |
| `LR` | 輕鐵 | Light Rail | `#9F7A00` | 无；仅状态 |

<a id="station-codes"></a>

## 附录 B：车站代码

车站按内置线路顺序分组，共用代码在各线均表示同一换乘站。支线条目不代表单一线性路径；导航应使用 `towards` 或公开路线段。`/api/lines` 提供这些代码的双语名称。

| 线路 | 车站代码及官方名称 |
| --- | --- |
| `AEL` | `HOK` 香港、`KOW` 九龍、`TSY` 青衣、`AIR` 機場、`AWE` 博覽館 |
| `TCL` | `HOK` 香港、`KOW` 九龍、`OLY` 奧運、`NAC` 南昌、`LAK` 荔景、`TSY` 青衣、`SUN` 欣澳、`TUC` 東涌 |
| `TML` | `WKS` 烏溪沙、`MOS` 馬鞍山、`HEO` 恆安、`TSH` 大水坑、`SHM` 石門、`CIO` 第一城、`STW` 沙田圍、`CKT` 車公廟、`TAW` 大圍、`HIK` 顯徑、`DIH` 鑽石山、`KAT` 啟德、`SUW` 宋皇臺、`TKW` 土瓜灣、`HOM` 何文田、`HUH` 紅磡、`ETS` 尖東、`AUS` 柯士甸、`NAC` 南昌、`MEF` 美孚、`TWW` 荃灣西、`KSR` 錦上路、`YUL` 元朗、`LOP` 朗屏、`TIS` 天水圍、`SIH` 兆康、`TUM` 屯門 |
| `TKL` | `NOP` 北角、`QUB` 鰂魚涌、`YAT` 油塘、`TIK` 調景嶺、`TKO` 將軍澳、`LHP` 康城、`HAH` 坑口、`POA` 寶琳 |
| `EAL` | `ADM` 金鐘、`EXC` 會展、`HUH` 紅磡、`MKK` 旺角東、`KOT` 九龍塘、`TAW` 大圍、`SHT` 沙田、`FOT` 火炭、`RAC` 馬場、`UNI` 大學、`TAP` 大埔墟、`TWO` 太和、`FAN` 粉嶺、`SHS` 上水、`LOW` 羅湖、`LMC` 落馬洲 |
| `SIL` | `ADM` 金鐘、`OCP` 海洋公園、`WCH` 黃竹坑、`LET` 利東、`SOH` 海怡半島 |
| `TWL` | `CEN` 中環、`ADM` 金鐘、`TST` 尖沙咀、`JOR` 佐敦、`YMT` 油麻地、`MOK` 旺角、`PRE` 太子、`SSP` 深水埗、`CSW` 長沙灣、`LCK` 荔枝角、`MEF` 美孚、`LAK` 荔景、`KWF` 葵芳、`KWH` 葵興、`TWH` 大窩口、`TSW` 荃灣 |
| `ISL` | `KET` 堅尼地城、`HKU` 香港大學、`SYP` 西營盤、`SHW` 上環、`CEN` 中環、`ADM` 金鐘、`WAC` 灣仔、`CAB` 銅鑼灣、`TIH` 天后、`FOH` 炮台山、`NOP` 北角、`QUB` 鰂魚涌、`TAK` 太古、`SWH` 西灣河、`SKW` 筲箕灣、`HFC` 杏花邨、`CHW` 柴灣 |
| `KTL` | `WHA` 黃埔、`HOM` 何文田、`YMT` 油麻地、`MOK` 旺角、`PRE` 太子、`SKM` 石硤尾、`KOT` 九龍塘、`LOF` 樂富、`WTS` 黃大仙、`DIH` 鑽石山、`CHH` 彩虹、`KOB` 九龍灣、`NTK` 牛頭角、`KWT` 觀塘、`LAT` 藍田、`YAT` 油塘、`TIK` 調景嶺 |
| `DRL` | `SUN` 欣澳、`DIS` 迪士尼 |

<a id="changelog"></a>

## 变更记录

| 日期 | 变更 |
| --- | --- |
| 2026-10-07 | 文档重写为英文，新增粤语及国语版本。明确 no-cache、重启行为、健康事件及推送规划。HTTP 契约不变，保留原有样本。 |
| 2026-10-02 | 不兼容：正式和 Mock 列车的 `platform` 整数改为 `platforms` 整数数组。机场支持 `[1, 3]`／`[2, 4]`，未知值用 `[]` 并保留列车，修复此前 `1/3` 导致机场板失败。 |
| 2026-10-02 | 新增四个需启用的 Mock 路由，支持场景／seed、实际值 header 及 `invalid_query`／`unknown_scenario`。正式契约不变。 |
| 2026-10-01 | 数据集／文件每次获取共享编码，最高 gzip 将车费下载从约 75 KB 减至 71 KB。增加 Content-Length 和 Vary；解压内容、ETag 及缓存不变。 |
| 2026-09-29 | 新增八个开放数据路由、每日轮询、ETag／304、整数港仙、gzip 及 `unknown_source`。 |
| 2026-09-29 | 新增健康检查，空 200、no-store，不追踪请求。 |
| 2026-09-28 | 线路状态改为 30 秒轮询，新鲜期最多 33 秒；字段不变。 |
| 2026-09-28 | 错误表移除未实现的 500 internal_error。 |
| 2026-09-28 | 不兼容：up／down 数组改为 directions，含 direction／towards／trains；省略终点方向，移除 sequence。线路 destinations 改为 directions，towards 仅列主要终点。 |
| 2026-09-28 | 首版线路、状态及两个 Next Train 路由；移除 /api/hello 和 /api/hello.json。 |
