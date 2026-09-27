# Dut API 文档

Dut 是 MTRGo App 的数据整合后端。本文档记录全部对外 HTTP 接口。

> **维护规则**：新增或修改任何接口时，必须在同一次改动里更新本文档，包括 URL、方法、参数、请求示例、响应示例、字段说明、错误和变更记录。详见 `AGENTS.md`。

## 目录

- [概览](#概览)
- [通用约定](#通用约定)
- [接口一览](#接口一览)
- [1. 获取线路与车站资料](#1-获取线路与车站资料)
- [2. 获取全线路服务状态](#2-获取全线路服务状态)
- [3. 获取单线单站列车到站](#3-获取单线单站列车到站)
- [4. 获取车站所有线路的列车到站](#4-获取车站所有线路的列车到站)
- [错误码](#错误码)
- [附录 A：线路代码](#附录-a线路代码)
- [附录 B：车站代码](#附录-b车站代码)
- [变更记录](#变更记录)

---

## 概览

| 项目 | 说明 |
|---|---|
| Base URL | `http://127.0.0.1:3000`（本地默认值，见 `src/bootstrap/config.rs`） |
| 路径前缀 | 所有接口都在 `/api` 下 |
| HTTP 方法 | 目前只有 `GET` |
| 鉴权 | 暂无 |
| 响应格式 | `application/json`，UTF-8 |
| 数据来源 | [MTR Next Train API](https://rt.data.gov.hk/v1/transport/mtr/getSchedule.php)、[MTR 线路状态 JSON](https://tnews.mtr.com.hk/alert/ryg_line_status.json) |

---

## 通用约定

### 时间格式

所有时间都是 **RFC 3339 字符串，香港时间（`+08:00`），精确到秒**，例如 `2026-09-28T00:20:27+08:00`。iOS 端可以直接用 `ISO8601DateFormatter` 解析，不需要开启小数秒选项。

### 倒计时由客户端计算

列车到站接口只返回**绝对时间** `arrival_at`，不返回"还有几分钟"。

- **计算方法**：倒计时 = `arrival_at − 设备当前时间`，客户端每秒在本地更新一次即可。
- **原因**：绝对时间放在缓存里不会过期；相对的分钟数一放进缓存就会变错。港铁原始数据里的 `ttnt` 字段因此已被丢弃。
- **不要**用 `generated_at` 或 `fetched_at` 推算倒计时。

### 代码大小写不敏感

路径里的线路代码和车站代码不区分大小写：`/api/lines/tkl/stations/tko/next-trains` 与 `/api/lines/TKL/stations/TKO/next-trains` 等价。响应里的代码一律为大写。

### 双语名称

线路名和站名都以 `{ "en": "...", "tc": "..." }` 返回，`en` 是英文，`tc` 是繁体中文。客户端按用户语言取用即可，不需要为不同语言重复请求。

### 行车方向

每条线路有两个行车方向，用 `direction` 标识，取值为 `up` 或 `down`，与港铁数据源一致。`up` / `down` 只是稳定的 ID，本身不表示"往哪走"：大多数线路的 `up` 朝 `stations` 列表的末端，迪士尼綫却正好相反；将军澳綫和东铁綫还有支线。所以：

- **展示给用户时用 `towards`。** 它列出这个方向驶往的终点站，写法与月台指示牌一致，例如将军澳站的上行是"往 寶琳／康城"。客户端直接拼成标题即可，不需要自己推算。
- **需要记住方向时用 `direction`。** 例如保存用户常坐的方向。
- **`towards` 只列主要终点。** 中途折返的班次（例如只到調景嶺的将军澳綫列车、只到上水的东铁綫列车）仍归在同一个方向下，实际终点看每班车的 `destination`。

### 缓存与数据新鲜度

服务端会缓存上游数据（策略见 `ARCHITECTURE.md`），并通过以下方式告诉客户端数据有多新：

| 位置 | 含义 |
|---|---|
| 响应头 `Cache-Control: public, max-age=N` | 数据还能被视为最新的剩余秒数。客户端和 CDN 可以在 N 秒内复用这个响应，也可以把 N 当作下次轮询的间隔。 |
| 响应头 `Cache-Control: no-cache` | 数据已经过期（见下面的 `stale`），不要复用这个响应。 |
| 响应体 `stale: true` | 这份数据已超过新鲜期，服务端暂时拿不到更新的数据，先返回旧数据。App 可以显示"资料可能已过时"之类的提示。 |
| 响应体 `generated_at` | 港铁生成这份数据的时间。通常比 `fetched_at` 早几秒。 |
| 响应体 `fetched_at` | 本服务从港铁取回这份数据的时间。 |

### 请求 ID

每个响应都带有 `x-request-id` 响应头（UUID），服务端日志里的每一行也都带这个 ID，排查问题时可以据此定位。客户端也可以自己在请求头里带上 `x-request-id`，服务端会沿用并原样返回。

### 错误格式

所有错误都返回统一的 JSON 结构，`code` 是稳定的机器可读值，`message` 是给人看的英文说明：

```json
{
  "error": {
    "code": "unknown_station",
    "message": "No station matches the requested station code"
  }
}
```

错误响应不带 `Cache-Control` 头。完整错误码见[错误码](#错误码)。

---

## 接口一览

| 方法 | URL | 用途 | 数据来源 |
|---|---|---|---|
| `GET` | `/api/lines` | 获取所有线路、车站、中英文站名、行车方向和线路颜色 | 服务内置的静态资料 |
| `GET` | `/api/lines/status` | 获取全线路服务状态（正常、延误、受阻等） | 港铁线路状态 JSON |
| `GET` | `/api/lines/{line}/stations/{station}/next-trains` | 获取某条线在某个车站的下几班列车 | 港铁 Next Train API |
| `GET` | `/api/stations/{station}/next-trains` | 获取途经某个车站的所有线路的下几班列车（适合换乘站） | 港铁 Next Train API |

---

## 1. 获取线路与车站资料

```
GET /api/lines
```

返回 Next Train 覆盖的 10 条线路。每条线路包含：

- 线路代码、中英文名称、品牌色
- 按行车顺序排列的车站
- 两个行车方向，以及各自驶往的终点站（见[行车方向](#行车方向)）

资料编译在服务内部，只会随部署更新，适合 App 启动时拉取一次后在本地缓存。

### 参数

无。

### 请求示例

```bash
curl http://127.0.0.1:3000/api/lines
```

### 响应示例

`200 OK`，响应头包含 `Cache-Control: public, max-age=86400`。

以下为节选：实际返回 10 条线路，这里只展示迪士尼綫。

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

### 字段说明

| 字段 | 类型 | 说明 |
|---|---|---|
| `lines` | array | 线路列表，顺序为 AEL、TCL、TML、TKL、EAL、SIL、TWL、ISL、KTL、DRL |
| `lines[].code` | string | 线路代码，见[附录 A](#附录-a线路代码) |
| `lines[].name` | object | 线路名称 `{ en, tc }` |
| `lines[].color` | string | 线路品牌色，格式为 `#RRGGBB` |
| `lines[].stations` | array | 本线车站，按行车顺序排列，元素为[车站引用](#车站引用-station) |
| `lines[].directions` | array | 两个行车方向，依次为 `up`、`down` |
| `lines[].directions[].direction` | string | 方向 ID，`up` 或 `down`，见[行车方向](#行车方向) |
| `lines[].directions[].towards` | array | 这个方向驶往的终点站，写法与月台指示牌一致，元素为[车站引用](#车站引用-station)。例如将军澳綫 `up` 为寶琳、康城 |

#### 车站引用 `station`

各接口里出现的车站都使用这个结构：

| 字段 | 类型 | 说明 |
|---|---|---|
| `code` | string | 三个大写字母的车站代码，见[附录 B](#附录-b车站代码) |
| `name` | object 或 `null` | 站名 `{ en, tc }`。只有当港铁返回了本服务不认识的车站代码时（例如新开通的车站）才会是 `null` |

### 错误

本接口没有业务错误。

---

## 2. 获取全线路服务状态

```
GET /api/lines/status
```

返回每条港铁线路的当前服务状态，包括轻铁（`LR`）。

响应里同时有两个字段描述状态：

- **`condition`**：语义值，表示港铁报告的状况，适合做逻辑判断。
- **`display`**：展示色，表示港铁官网怎么显示这个状况，直接拿来上色就能和官网保持一致。

### 参数

无。

### 请求示例

```bash
curl http://127.0.0.1:3000/api/lines/status
```

### 响应示例

`200 OK`，响应头包含 `Cache-Control: public, max-age=30`。

以下为节选：实际返回 11 条线路，这里只展示 3 条。

```json
{
  "updated_at": "2026-09-27T06:15:00+08:00",
  "fetched_at": "2026-09-28T00:18:39+08:00",
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

下面是线路出现延误时单个元素的**示意**（非真实数据）：

```json
{
  "line": { "code": "KTL", "name": { "en": "Kwun Tong Line", "tc": "觀塘綫" } },
  "color": "#1A9431",
  "condition": "delayed_or_disrupted",
  "display": "yellow",
  "message": "Trains are delayed"
}
```

### 字段说明

| 字段 | 类型 | 说明 |
|---|---|---|
| `updated_at` | string（时间） | 港铁最后一次发布这份状态的时间 |
| `fetched_at` | string（时间） | 本服务取回数据的时间 |
| `stale` | boolean | 数据是否已超过新鲜期，见下方"缓存行为" |
| `lines` | array | 各线路状态，顺序与港铁数据源一致 |
| `lines[].line` | object | 线路 `{ code, name }` |
| `lines[].color` | string | 线路品牌色，格式为 `#RRGGBB` |
| `lines[].condition` | string | 语义状态，取值见下表 |
| `lines[].display` | string | 展示色：`green`、`yellow`、`red`、`grey` 或 `typhoon` |
| `lines[].message` | string 或 `null` | 港铁附带的文字说明，没有时为 `null` |

#### `condition` 与 `display` 对照

| 港铁原始值 | `condition` | 含义 | `display` |
|---|---|---|---|
| `green` | `normal` | 服务正常 | `green` |
| `yellow` | `delayed` | 延误，需要额外候车或行车时间；也用于"逐步恢复正常" | `yellow` |
| `red` | `disrupted` | 服务受阻，建议改用其他交通工具 | `red` |
| `pink` | `delayed_or_disrupted` | 延误或受阻 | `yellow`（与港铁官网一致） |
| `grey` | `non_service_hours` | 非服务时间 | `grey` |
| `typhoon` | `typhoon_signal` | 热带气旋警告信号生效 | `typhoon` |
| 其他 | `unknown` | 本服务尚未识别的新状态（会记 warn 日志） | `grey` |

### 缓存行为

- 数据缓存 30 秒。
- 过期后 60 秒内的请求会**立即返回旧数据**，同时在后台刷新。这时响应的 `stale` 为 `true`，`Cache-Control` 为 `no-cache`，下一次请求就会拿到新数据。所以即使上游一切正常，偶尔也会看到 `stale: true`。
- 上游故障时，15 分钟内的旧数据仍会返回（同样标记 `stale: true`）。

### 错误

| HTTP 状态 | `code` | 触发条件 |
|---|---|---|
| `502` | `upstream_unavailable` | 港铁状态源不可用，且没有 15 分钟内的旧数据可以返回 |

---

## 3. 获取单线单站列车到站

```
GET /api/lines/{line}/stations/{station}/next-trains
```

按行车方向返回某条线路在某个车站的列车。每个方向写明驶往哪里（`towards`），并列出最多 4 班列车的终点站、月台和预计到站时间。

- 已开出超过 30 秒的班次会被自动过滤，所以每个方向可能少于 4 班。
- 本站是某个方向的终点时，不返回这个方向。例如寶琳站只有 `down`（往北角）；康城站在支线末端，也没有 `up`。如果港铁仍在这个方向报了列车，则照常返回，不会隐藏任何列车。
- 非服务时间等情况下，方向照常返回，但 `trains` 可能是空数组。

### 路径参数

| 参数 | 类型 | 必填 | 说明 | 示例 |
|---|---|---|---|---|
| `line` | string | 是 | 线路代码，大小写不敏感。可选值见[附录 A](#附录-a线路代码)，轻铁 `LR` 不支持 | `TKL` |
| `station` | string | 是 | 车站代码，大小写不敏感，必须是这条线路上的车站。可选值见[附录 B](#附录-b车站代码) | `TKO` |

### 请求示例

```bash
curl http://127.0.0.1:3000/api/lines/TKL/stations/TKO/next-trains
```

### 响应示例

`200 OK`，响应头包含 `Cache-Control: public, max-age=10`。

以下为 01:09 的真实响应，上行只展示前 2 班。这时往北角的尾班车已经开出，所以 `down` 的 `trains` 为空，但 `towards` 仍然告诉用户这是往北角的方向。

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
          "platform": 1,
          "arrival_at": "2026-09-28T01:13:49+08:00",
          "time_type": null,
          "via_racecourse": false
        },
        {
          "destination": { "code": "LHP", "name": { "en": "LOHAS Park", "tc": "康城" } },
          "platform": 1,
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

在终点站寶琳（`GET /api/lines/TKL/stations/POA/next-trains`，同一时刻）只返回离开本站的方向：

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

东铁线的列车会额外填上 `time_type`（`GET /api/lines/EAL/stations/SHT/next-trains` 中 `trains` 的一个元素）：

```json
{
  "destination": { "code": "SHS", "name": { "en": "Sheung Shui", "tc": "上水" } },
  "platform": 2,
  "arrival_at": "2026-09-28T00:19:34+08:00",
  "time_type": "arrival",
  "via_racecourse": false
}
```

港铁发布特别安排（如台风、特别班次）时，`alert` 的**示意**如下（非真实数据）：

```json
{
  "alert": {
    "en": { "message": "Special train service arrangement", "url": "https://example.com/notice" },
    "tc": { "message": "特別列車服務安排", "url": "https://example.com/notice" }
  }
}
```

### 字段说明

| 字段 | 类型 | 说明 |
|---|---|---|
| `line` | object | 线路 `{ code, name }` |
| `station` | object | 车站，见[车站引用](#车站引用-station) |
| `generated_at` | string（时间） | 港铁生成这份数据的时间 |
| `fetched_at` | string（时间） | 本服务取回数据的时间 |
| `stale` | boolean | `true` 表示港铁暂时不可用，这是 90 秒内的旧数据 |
| `delayed` | boolean | 港铁标记的列车延误 |
| `alert` | object 或 `null` | 特别安排通告，没有时为 `null`。结构为 `{ en: notice, tc: notice }` |
| `alert.*.message` | string | 通告文字 |
| `alert.*.url` | string 或 `null` | 通告详情链接 |
| `directions` | array | 本站可以乘车的行车方向，`up` 在前。本站是某个方向的终点时不含这个方向 |

#### 行车方向 `directions[]`

| 字段 | 类型 | 说明 |
|---|---|---|
| `direction` | string | 方向 ID，`up` 或 `down`，见[行车方向](#行车方向) |
| `towards` | array | 从本站出发、这个方向能到达的主要终点站，写法与月台指示牌一致，元素为[车站引用](#车站引用-station)。适合直接做成"往 寶琳／康城"这样的标题。港铁报了列车、但本站之后已没有主要终点时为空数组 |
| `trains` | array | 这个方向即将到站的列车，最多 4 班，按到站先后排列 |

#### 列车 `directions[].trains[]`

| 字段 | 类型 | 说明 |
|---|---|---|
| `destination` | object | 列车终点站，见[车站引用](#车站引用-station) |
| `platform` | integer | 月台编号 |
| `arrival_at` | string（时间） | 预计到站时间；`time_type` 为 `departure` 时是预计开出时间。**倒计时请用它计算** |
| `time_type` | string 或 `null` | 仅东铁线提供：`arrival`（到站）或 `departure`（开出，常见于始发站）。其他线路为 `null` |
| `via_racecourse` | boolean | 仅东铁线有意义：列车是否经马场站（而不是火炭站）。其他线路恒为 `false` |

### 缓存行为

- 新鲜期跟随港铁 CDN 的剩余缓存时间，通常 10 秒，范围 2–15 秒。建议客户端按 `Cache-Control` 的 `max-age` 轮询。
- 过期后，下一次请求会等待刷新完成再返回（通常几百毫秒），保证拿到的是最新数据。同一时刻的大量请求只会触发一次上游请求。
- 港铁故障时，90 秒内的旧数据仍会返回，并标记 `stale: true`。刷新失败后的 5 秒内不会再请求上游。

### 错误

| HTTP 状态 | `code` | 触发条件 |
|---|---|---|
| `404` | `unknown_line` | `line` 不是已知的线路代码 |
| `404` | `unknown_station` | `station` 不是三个字母，或不是已知车站 |
| `404` | `station_not_on_line` | 这条线路不经过该车站（例如 `TKL` + `ADM`），或线路没有到站数据（`LR`） |
| `502` | `upstream_unavailable` | 港铁 Next Train API 不可用，且没有 90 秒内的旧数据 |

错误请求示例：

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

---

## 4. 获取车站所有线路的列车到站

```
GET /api/stations/{station}/next-trains
```

一次性返回途经某个车站的**所有线路**的列车，适合在车站详情页或换乘站使用。例如金钟（`ADM`）会同时返回东铁线、南港岛綫、荃湾綫和港岛綫。

- 服务端并发查询每条线路，每条线路各自走缓存。
- **只要有一条线路成功就返回 `200`**。失败的线路 `board` 为 `null`，`error` 说明原因，不影响其他线路。

### 路径参数

| 参数 | 类型 | 必填 | 说明 | 示例 |
|---|---|---|---|---|
| `station` | string | 是 | 车站代码，大小写不敏感。可选值见[附录 B](#附录-b车站代码) | `ADM` |

### 请求示例

```bash
curl http://127.0.0.1:3000/api/stations/ADM/next-trains
```

### 响应示例

`200 OK`，响应头包含 `Cache-Control: public, max-age=10`。

以下为 01:09 的真实响应节选：实际返回 EAL、SIL、TWL、ISL 四条线，这里只展示 EAL 和 TWL，此时两条线的尾班车都已开出。金钟是东铁线的终点，所以 EAL 只有 `up` 一个方向。

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

某条线路失败时，`lines` 中对应元素的**示意**如下（非真实数据）：

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

### 字段说明

| 字段 | 类型 | 说明 |
|---|---|---|
| `station` | object | 车站，见[车站引用](#车站引用-station) |
| `lines` | array | 途经该站的线路，顺序同 `GET /api/lines` |
| `lines[].line` | object | 线路 `{ code, name }` |
| `lines[].board` | object 或 `null` | 该线路的到站数据。字段与[接口 3](#3-获取单线单站列车到站) 相同，但不含 `line` 和 `station`。失败时为 `null` |
| `lines[].error` | object 或 `null` | 失败原因 `{ code, message }`，成功时为 `null` |

### 缓存行为

- `Cache-Control` 的 `max-age` 取所有成功线路中最短的剩余新鲜期。
- 只要有任何一条线路返回的是旧数据，就是 `no-cache`。
- 每条线路的缓存行为与[接口 3](#3-获取单线单站列车到站) 相同。

### 错误

| HTTP 状态 | `code` | 触发条件 |
|---|---|---|
| `404` | `unknown_station` | `station` 不是三个字母，或不是已知车站 |
| `502` | `upstream_unavailable` | 途经该站的**所有**线路都失败 |

---

## 错误码

| HTTP 状态 | `code` | 说明 |
|---|---|---|
| `404` | `not_found` | 路由不存在，例如 `GET /api/nope` |
| `404` | `unknown_line` | 线路代码不存在 |
| `404` | `unknown_station` | 车站代码格式错误或车站不存在 |
| `404` | `station_not_on_line` | 线路不经过该车站 |
| `502` | `upstream_unavailable` | 上游数据源（港铁）不可用，且没有可用的旧数据 |
| `500` | `internal_error` | 服务内部错误 |

错误响应里的 `message` 不会回显客户端输入，也不会包含上游错误细节。上游错误的完整信息只记录在服务端日志里，用 `x-request-id` 查找。

---

## 附录 A：线路代码

| 代码 | 中文 | English | 颜色 | 列车到站 |
|---|---|---|---|---|
| `AEL` | 機場快綫 | Airport Express | `#1C7670` | ✅ |
| `TCL` | 東涌綫 | Tung Chung Line | `#FE7F1D` | ✅ |
| `TML` | 屯馬綫 | Tuen Ma Line | `#9A3B26` | ✅ |
| `TKL` | 將軍澳綫 | Tseung Kwan O Line | `#6B208B` | ✅ |
| `EAL` | 東鐵綫 | East Rail Line | `#5EB6E4` | ✅ |
| `SIL` | 南港島綫 | South Island Line | `#99CF16` | ✅ |
| `TWL` | 荃灣綫 | Tsuen Wan Line | `#FF0000` | ✅ |
| `ISL` | 港島綫 | Island Line | `#0860A8` | ✅ |
| `KTL` | 觀塘綫 | Kwun Tong Line | `#1A9431` | ✅ |
| `DRL` | 迪士尼綫 | Disneyland Resort Line | `#F550A6` | ✅ |
| `LR` | 輕鐵 | Light Rail | `#9F7A00` | ❌ 仅出现在线路状态中 |

## 附录 B：车站代码

按线路列出，顺序为行车顺序。同一个代码在不同线路上指的是同一个车站，例如 `ADM` 同时在 EAL、SIL、TWL、ISL 上。

| 线路 | 车站 |
|---|---|
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

---

## 变更记录

| 日期 | 变更 |
|---|---|
| 2026-09-28 | **不兼容变更。** 列车到站接口（`GET /api/lines/{line}/stations/{station}/next-trains`、`GET /api/stations/{station}/next-trains`）的 `up` / `down` 数组改为 `directions` 数组，每个方向带 `direction`、`towards`、`trains`；本站是某方向终点时不再返回该方向；列车移除 `sequence`，改以数组顺序表示先后。`GET /api/lines` 的 `destinations` 改为 `directions`，`towards` 只列月台指示牌上的主要终点。 |
| 2026-09-28 | 首版。新增 `GET /api/lines`、`GET /api/lines/status`、`GET /api/lines/{line}/stations/{station}/next-trains`、`GET /api/stations/{station}/next-trains`。移除 Hello 示例接口 `GET /api/hello`、`GET /api/hello.json`。 |
