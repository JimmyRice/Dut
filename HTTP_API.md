# Dut HTTP API 文档

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
- [5. 健康检查](#5-健康检查)
- [开放数据接口的共同行为](#开放数据接口的共同行为)
- [6. 开放数据索引](#6-开放数据索引)
- [7. 获取开放数据原始文件](#7-获取开放数据原始文件)
- [8. 获取车站与路线（开放数据）](#8-获取车站与路线开放数据)
- [9. 获取港铁车费](#9-获取港铁车费)
- [10. 获取机场快綫车费](#10-获取机场快綫车费)
- [11. 获取轻铁车站与路线](#11-获取轻铁车站与路线)
- [12. 获取轻铁车费](#12-获取轻铁车费)
- [13. 获取无障碍设施](#13-获取无障碍设施)
- [Mock 接口的共同行为](#mock-接口的共同行为)
- [14. Mock 场景列表](#14-mock-场景列表)
- [15. 模拟全线路服务状态](#15-模拟全线路服务状态)
- [16. 模拟单线单站列车到站](#16-模拟单线单站列车到站)
- [17. 模拟车站所有线路的列车到站](#17-模拟车站所有线路的列车到站)
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
| HTTP 方法 | 目前只有 `GET`（健康检查也接受 `HEAD`） |
| 开发用接口 | `/api/mock` 下的 Mock 接口按场景返回模拟数据，默认关闭，见 [Mock 接口的共同行为](#mock-接口的共同行为) |
| 鉴权 | 暂无 |
| 响应格式 | `application/json`，UTF-8。健康检查不返回响应体；开放数据原始文件返回 `text/csv` |
| 压缩 | 请求带 `Accept-Encoding: gzip` 时，响应以 gzip 压缩 |
| 数据来源 | [MTR Next Train API](https://rt.data.gov.hk/v1/transport/mtr/getSchedule.php)、[MTR 线路状态 JSON](https://tnews.mtr.com.hk/alert/ryg_line_status.json)、[港铁开放数据平台](https://opendata.mtr.com.hk/) |

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

### 金额

车费一律以**港仙整数**表示：`490` 就是 HK$4.90。港铁公布的车费最多两位小数，用整数可以精确表示，客户端不会遇到浮点误差。显示时除以 100 即可。

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
| 响应头 `ETag` | 开放数据接口才有。下次请求时放进 `If-None-Match`，数据没变就返回 `304 Not Modified`，不传输数据，见[开放数据接口的共同行为](#开放数据接口的共同行为)。 |

### 请求 ID

除健康检查外，每个响应都带有 `x-request-id` 响应头（UUID），服务端日志里的每一行也都带这个 ID，排查问题时可以据此定位。客户端也可以自己在请求头里带上 `x-request-id`，服务端会沿用并原样返回。

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
| `GET` | `/api/health` | 健康检查，只返回 `200`，不返回响应体 | 不访问上游 |
| `GET` | `/api/data` | 开放数据索引：各数据集和原始文件的版本 | 港铁开放数据平台 |
| `GET` | `/api/data/sources/{file}` | 原样返回港铁开放数据的 CSV 文件 | 港铁开放数据平台 |
| `GET` | `/api/data/stations` | 开放数据里的车站和各线路的行车路线（含支线） | 港铁开放数据平台 |
| `GET` | `/api/data/fares` | 重铁任意两站的车费 | 港铁开放数据平台 |
| `GET` | `/api/data/airport-express-fares` | 机场快綫车费 | 港铁开放数据平台 |
| `GET` | `/api/data/light-rail` | 轻铁车站和路线 | 港铁开放数据平台 |
| `GET` | `/api/data/light-rail-fares` | 轻铁任意两站的车费 | 港铁开放数据平台 |
| `GET` | `/api/data/accessibility` | 无障碍设施目录和各站设施 | 港铁开放数据平台 |
| `GET` | `/api/mock/scenarios` | Mock 接口能模拟的场景（需开启 Mock 接口，下同） | 服务内置 |
| `GET` | `/api/mock/lines/status` | 按场景模拟全线路服务状态 | 服务模拟，不访问上游 |
| `GET` | `/api/mock/lines/{line}/stations/{station}/next-trains` | 按场景模拟单线单站列车到站 | 服务模拟，不访问上游 |
| `GET` | `/api/mock/stations/{station}/next-trains` | 按场景模拟车站所有线路的列车到站 | 服务模拟，不访问上游 |

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

`200 OK`，响应头包含 `Cache-Control: public, max-age=16`（距下一次轮询的剩余秒数，见下方"缓存行为"）。

以下为节选：实际返回 11 条线路，这里只展示 3 条。

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
| `updated_at` | string（时间） | 港铁最后一次发布这份状态的时间。港铁只在有线路状态变化时才重新发布，所以它可能是几小时前的，不代表数据过时 |
| `fetched_at` | string（时间） | 本服务最近一次成功取回数据的时间 |
| `stale` | boolean | 最近几次拉取是否失败、正在用旧数据顶替，见下方"缓存行为" |
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

- 服务在后台每 30 秒拉取一次港铁状态源，请求本身不会触发上游调用，直接返回最近一次拉取的结果。
- `Cache-Control` 的 `max-age` 是这份数据还能保持新鲜的秒数：从拉取时起算 33 秒（30 秒间隔加 3 秒请求超时），所以最多为 33，并随时间递减。
- 上游正常时 `stale` 始终为 `false`。只有拉取失败时，旧数据才会以 `stale: true`、`Cache-Control: no-cache` 返回，最长顶替 15 分钟。
- 服务刚启动、第一次拉取还没完成时，请求会等它完成再返回。

### 错误

| HTTP 状态 | `code` | 触发条件 |
|---|---|---|
| `502` | `upstream_unavailable` | 从未成功拉取到港铁状态源，或最近一次成功拉取已超过新鲜期加 15 分钟 |

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
  "platforms": [2],
  "arrival_at": "2026-09-28T00:19:34+08:00",
  "time_type": "arrival",
  "via_racecourse": false
}
```

机场站的机场快綫列车两侧车门同时打开，所以停靠两个月台（`GET /api/lines/AEL/stations/AIR/next-trains` 于 2026-10-02 16:32 的真实响应中 `trains` 的一个元素）。往香港的列车是 `[2, 4]`：

```json
{
  "destination": { "code": "AWE", "name": { "en": "AsiaWorld-Expo", "tc": "博覽館" } },
  "platforms": [1, 3],
  "arrival_at": "2026-10-02T16:33:00+08:00",
  "time_type": null,
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
| `platforms` | array（integer） | 列车停靠的月台编号，按港铁给出的顺序排列。通常只有一个，例如 `[1]`。机场站的机场快綫列车两侧车门同时打开，停靠两个月台：往博覽館为 `[1, 3]`，往香港为 `[2, 4]`，适合显示为"1 及 3 号月台"。港铁给出的月台无法识别时为空数组 `[]`，列车照常列出 |
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

## 5. 健康检查

```
GET /api/health
```

供负载均衡、容器编排和监控服务探测服务是否存活。只要进程还在处理 HTTP 请求，就返回 `200 OK`，不返回响应体。也接受 `HEAD` 请求，结果相同。

- **不访问上游，也不读取任何数据。** 港铁或天文台出故障时它照样返回 `200`，编排系统不会因此重启一个正常的进程。上游的可用性看服务端日志里的数据源健康状态。
- **返回 `200` 而不是 `204`。** AWS ALB、Google Cloud 负载均衡和 Cloudflare 的健康检查默认只认 `200`。

### 参数

无。

### 请求示例

```bash
curl -i http://127.0.0.1:3000/api/health
```

### 响应示例

`200 OK`，响应体为空：

```
HTTP/1.1 200 OK
cache-control: no-store
content-length: 0
date: Mon, 28 Sep 2026 16:44:12 GMT
```

### 字段说明

没有响应体，也没有 `Content-Type` 头。只需判断状态码是否为 `200`。

### 缓存行为

响应头为 `Cache-Control: no-store`，任何缓存都不应保存它，每次探测都必须到达服务本身。

### 请求日志

健康检查不记入请求日志，响应也不带 `x-request-id`。探针通常每几秒请求一次，记下来会淹没真实请求的日志。

### 错误

无。服务不可用时连接会失败或超时，不会返回其他状态码。

---

## 开放数据接口的共同行为

第 6 至 13 节的接口都来自[港铁开放数据平台](https://opendata.mtr.com.hk/)的 7 个 CSV 文件：车站、车费、轻铁和无障碍设施。这些资料一年只变几次，适合 Local First 的 App 下载后存在本地，只在有变化时重新下载。

**推荐的同步方式**：App 启动时请求一次[索引](#6-开放数据索引)，把每个数据集的 `revision` 和本地保存的比较，只下载有变化的数据集。也可以对每个接口发条件请求（带 `If-None-Match`），没变化时服务返回 `304`，不传输数据。

- **后台拉取。** 服务启动时立即拉取全部 7 个文件，之后每 24 小时一次。请求本身不会触发上游调用。拉取失败时 5 分钟后重试。
- **一致性。** 7 个文件在同一次拉取里读取，任何一个下载或清洗失败，这次拉取就整体作废，继续使用上一份完整数据。所以同一时刻各接口返回的数据互相对得上（例如车费里的车站代码一定能在车站资料里找到），原始 CSV 与清洗结果也出自同一份文件。
- **`Cache-Control`。** `max-age` 是距下一次拉取的剩余秒数，最长约一天（86400 秒加 30 秒请求超时），并随时间递减。拉取一直失败时，旧数据以 `stale: true`、`Cache-Control: no-cache` 返回，最长顶替 30 天。
- **`ETag`。** 每个响应都带弱 ETag，例如 `W/"0.4.1-5987f5c9680ce780"`。清洗后的数据集，ETag 由服务版本号和数据内容决定：港铁文件改了、但清洗结果没变时 ETag 不变；服务升级改了 JSON 结构时 ETag 会变。原始 CSV 的 ETag 只由文件字节决定。
- **压缩。** 清洗后的数据集和原始 CSV 在每次拉取后只编码一次：第一个请求触发序列化和 gzip（最高压缩级别），之后的请求共享同一份结果，所以下载大数据集几乎不增加服务端负担，响应也带 `Content-Length`。每次拉取后的第一个请求会多等编码的十几毫秒。是否压缩的判断与其他接口相同；响应都带 `Vary: Accept-Encoding`，供中间的缓存区分压缩和未压缩的版本。
- **启动时。** 服务刚启动、第一次拉取还没完成时，请求会等它完成再返回。
- **错误。** 从未成功拉取过，或最近一次成功拉取已超过新鲜期加 30 天，返回 `502 upstream_unavailable`。

清洗后的 JSON 数据集都以同样三个字段开头：

| 字段 | 类型 | 说明 |
|---|---|---|
| `updated_at` | string（时间）或 `null` | 港铁最后一次修改这份数据所用文件的时间（取自上游 `Last-Modified`），上游没给时为 `null`。它往往是几个月甚至几年前，不代表数据过时 |
| `fetched_at` | string（时间） | 本服务最近一次成功拉取的时间 |
| `stale` | boolean | 最近的拉取是否失败、正在用旧数据顶替 |

---

## 6. 开放数据索引

```
GET /api/data
```

列出所有清洗后的数据集和所有原始文件，以及各自的 `revision`。App 只需请求这一个接口，就能判断哪些数据要重新下载。

### 参数

无。

### 请求示例

```bash
curl http://127.0.0.1:3000/api/data
```

### 响应示例

`200 OK`，响应头包含 `Cache-Control: public, max-age=86414`。

以下为节选：`sources` 实际有 7 个文件，这里只展示 2 个。

```json
{
  "fetched_at": "2026-09-29T03:08:55+08:00",
  "stale": false,
  "datasets": [
    {
      "name": "stations",
      "path": "/api/data/stations",
      "revision": "0.4.1-0d1201f5804cd4cd",
      "updated_at": "2023-11-21T18:09:07+08:00"
    },
    {
      "name": "fares",
      "path": "/api/data/fares",
      "revision": "0.4.1-5987f5c9680ce780",
      "updated_at": "2026-04-03T01:02:50+08:00"
    },
    {
      "name": "airport-express-fares",
      "path": "/api/data/airport-express-fares",
      "revision": "0.4.1-545393302ce4c18f",
      "updated_at": "2025-06-22T01:05:16+08:00"
    },
    {
      "name": "light-rail",
      "path": "/api/data/light-rail",
      "revision": "0.4.1-97a676332b659216",
      "updated_at": "2026-07-05T00:58:02+08:00"
    },
    {
      "name": "light-rail-fares",
      "path": "/api/data/light-rail-fares",
      "revision": "0.4.1-e9deb534253c3bde",
      "updated_at": "2024-06-30T01:39:03+08:00"
    },
    {
      "name": "accessibility",
      "path": "/api/data/accessibility",
      "revision": "0.4.1-b199444f08da98bf",
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

### 字段说明

| 字段 | 类型 | 说明 |
|---|---|---|
| `fetched_at` | string（时间） | 本服务最近一次成功拉取的时间 |
| `stale` | boolean | 最近的拉取是否失败、正在用旧数据顶替 |
| `datasets` | array | 清洗后的数据集，顺序固定 |
| `datasets[].name` | string | 数据集名：`stations`、`fares`、`airport-express-fares`、`light-rail`、`light-rail-fares`、`accessibility` |
| `datasets[].path` | string | 数据集的请求路径 |
| `datasets[].revision` | string | 数据集版本，与该路径响应的 `ETag` 相同（去掉 `W/` 和引号）。只用于比较是否相等，不要解析它 |
| `datasets[].updated_at` | string（时间）或 `null` | 同各数据集响应里的 `updated_at` |
| `sources` | array | 原始文件，顺序固定 |
| `sources[].file` | string | 港铁开放数据平台上的文件名 |
| `sources[].path` | string | 原始文件的请求路径 |
| `sources[].revision` | string | 文件版本，与该路径响应的 `ETag` 相同（去掉 `W/` 和引号），只随文件字节变化 |
| `sources[].updated_at` | string（时间）或 `null` | 上游 `Last-Modified` |
| `sources[].bytes` | integer | 文件大小（未压缩） |

### 缓存行为

见[开放数据接口的共同行为](#开放数据接口的共同行为)。索引本身不带 `ETag`，它很小，每次直接请求即可。

### 错误

| HTTP 状态 | `code` | 触发条件 |
|---|---|---|
| `502` | `upstream_unavailable` | 从未成功拉取到开放数据，或最近一次成功拉取已超过新鲜期加 30 天 |

---

## 7. 获取开放数据原始文件

```
GET /api/data/sources/{file}
```

**原样**返回港铁开放数据平台上的某个 CSV 文件，与上游逐字节相同（包括 BOM、`\r\n` 换行和原有的错字）。`{file}` 就是上游文件名，所以已经从 `https://opendata.mtr.com.hk/data/{file}` 下载的代码，只需把 base URL 换成本服务即可。

需要干净、结构化的数据时，请改用第 8 至 13 节的接口。

### 路径参数

| 参数 | 说明 |
|---|---|
| `file` | 文件名，不区分大小写。只接受以下 7 个：`mtr_lines_and_stations.csv`、`mtr_lines_fares.csv`、`airport_express_fares.csv`、`light_rail_routes_and_stops.csv`、`light_rail_fares.csv`、`barrier_free_facility_category.csv`、`barrier_free_facilities.csv` |

### 请求示例

```bash
curl -i http://127.0.0.1:3000/api/data/sources/airport_express_fares.csv
```

### 响应示例

`200 OK`，`Content-Type: text/csv; charset=utf-8`。以下为节选，只展示前 5 行：

```
HTTP/1.1 200 OK
content-type: text/csv; charset=utf-8
cache-control: public, max-age=86364
etag: W/"e289177a7cb46ae8"
vary: accept-encoding
content-length: 691

ST_FROM,ST_FROM_ID,ST_TO,ST_TO_ID,OCT_ADT_FARE,OCT_CHD_FARE,SINGLE_ADT_FARE,SINGLE_CHD_FARE
HongKong,44,Airport,47,120,60,130,65
HongKong,44,AsiaWorld-Expo,56,120,60,130,65
Kowloon,45,Airport,47,105,52.5,115,57.5
Kowloon,45,AsiaWorld-Expo,56,105,52.5,115,57.5
```

### 字段说明

响应体就是港铁的 CSV 文件，列的含义见港铁开放数据平台的说明文件。

### 缓存行为

见[开放数据接口的共同行为](#开放数据接口的共同行为)。`ETag` 只随文件字节变化，服务升级不会让它改变。

### 错误

| HTTP 状态 | `code` | 触发条件 |
|---|---|---|
| `404` | `unknown_source` | `file` 不是上面 7 个文件名之一 |
| `502` | `upstream_unavailable` | 从未成功拉取到开放数据，或最近一次成功拉取已超过新鲜期加 30 天 |

---

## 8. 获取车站与路线（开放数据）

```
GET /api/data/stations
```

返回港铁开放数据里的重铁车站列表，以及每条线的每条行车路线，来自 `mtr_lines_and_stations.csv`。

它与 [`GET /api/lines`](#1-获取线路与车站资料) 的区别：`/api/lines` 是编译进服务的静态资料，列车到站接口依赖它；本接口原样反映港铁最新发布的内容。两者目前一致，只有马场（`RAC`）不在开放数据里。两者不一致时（例如新车站开通），服务日志会提示更新静态资料，见 README 的 `scripts/sync-network.py`。

清洗内容：

- 数字车站 ID 换成车站代码，站名去掉多余空格。
- 「茘」（荔景、荔枝角）统一成标准写法「荔」。
- 去掉文件末尾的空行。
- 支线单独成为一条路线，用 `from` 和 `towards` 区分，不使用 `LMC-UT` 这类内部代号。

### 参数

无。

### 请求示例

```bash
curl http://127.0.0.1:3000/api/data/stations
```

### 响应示例

`200 OK`。以下为节选：实际有 97 个车站、10 条线，这里只展示 2 个车站和将军澳綫。

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

### 字段说明

| 字段 | 类型 | 说明 |
|---|---|---|
| `updated_at`、`fetched_at`、`stale` | | 见[开放数据接口的共同行为](#开放数据接口的共同行为) |
| `stations` | array | 车站，按代码排序 |
| `stations[].code` | string | 车站代码 |
| `stations[].name` | object | 开放数据里的双语站名 `{ en, tc }` |
| `stations[].lines` | array of string | 途经该站的线路代码，按[附录 A](#附录-a线路代码)的顺序 |
| `lines` | array | 线路，按附录 A 的顺序；开放数据里没有路线的线路（轻铁）不列出 |
| `lines[].code`、`lines[].name` | | 线路代码和双语线路名 |
| `lines[].routes` | array | 行车路线：先 `up` 后 `down`，同方向先主线后支线 |
| `lines[].routes[].direction` | string | `up` 或 `down`，与[行车方向](#行车方向)一致 |
| `lines[].routes[].from` | object | 路线起点 `{ code, name }` |
| `lines[].routes[].towards` | object | 路线终点 `{ code, name }`，用来向用户区分主线和支线 |
| `lines[].routes[].stations` | array of string | 依次停靠的车站代码。支线路线可能只列支线本身，例如往康城的路线从調景嶺开始 |

注意迪士尼綫的 `up` 是从迪士尼开往欣澳，与港铁 Next Train API 一致。

### 缓存行为

见[开放数据接口的共同行为](#开放数据接口的共同行为)。

### 错误

| HTTP 状态 | `code` | 触发条件 |
|---|---|---|
| `502` | `upstream_unavailable` | 从未成功拉取到开放数据，或最近一次成功拉取已超过新鲜期加 30 天 |

---

## 9. 获取港铁车费

```
GET /api/data/fares
```

返回重铁任意两站之间的车费，来自 `mtr_lines_fares.csv`，不含机场快綫（见第 10 节）。金额单位是**港仙**，见[金额](#金额)。

清洗内容：

- 数字车站 ID 换成车站代码。马场不在车站列表里，按站名对应到 `RAC`。
- 去掉起点和终点相同的行。

### 参数

无。

### 请求示例

```bash
curl --compressed http://127.0.0.1:3000/api/data/fares
```

响应约 1.7 MB，gzip 后约 71 KB，请求时请带 `Accept-Encoding: gzip`（curl 用 `--compressed`，`URLSession` 默认就会带）。

### 响应示例

`200 OK`，响应头包含 `ETag: W/"0.4.1-5987f5c9680ce780"`。以下为节选：实际有 9120 个行程，这里只展示 2 个。

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

### 字段说明

| 字段 | 类型 | 说明 |
|---|---|---|
| `updated_at`、`fetched_at`、`stale` | | 见[开放数据接口的共同行为](#开放数据接口的共同行为) |
| `fares` | array | 行程，按起点代码、再按终点代码排序。A→B 和 B→A 各占一项 |
| `fares[].from` | string | 起点车站代码 |
| `fares[].to` | string | 终点车站代码 |
| `fares[].octopus.adult` | integer | 八达通成人车费（港仙） |
| `fares[].octopus.student` | integer | 学生八达通（学生乘车优惠计划） |
| `fares[].octopus.joyyou_sixty` | integer | 乐悠咭（60 至 64 岁） |
| `fares[].octopus.child` | integer | 小童八达通 |
| `fares[].octopus.elderly` | integer | 长者八达通（65 岁或以上） |
| `fares[].octopus.disability` | integer | 残疾人士 |
| `fares[].single_journey.adult` | integer | 单程票成人 |
| `fares[].single_journey.child` | integer | 单程票小童 |
| `fares[].single_journey.elderly` | integer | 单程票长者 |

### 缓存行为

见[开放数据接口的共同行为](#开放数据接口的共同行为)。

### 错误

| HTTP 状态 | `code` | 触发条件 |
|---|---|---|
| `502` | `upstream_unavailable` | 从未成功拉取到开放数据，或最近一次成功拉取已超过新鲜期加 30 天 |

---

## 10. 获取机场快綫车费

```
GET /api/data/airport-express-fares
```

返回机场快綫各站之间的车费，来自 `airport_express_fares.csv`。机场快綫只分成人和小童票价。金额单位是**港仙**。

港铁文件为机场快綫的香港、九龍、青衣使用了独立的 ID（44 至 46），这里统一换成与其他线路相同的车站代码 `HOK`、`KOW`、`TSY`。

### 参数

无。

### 请求示例

```bash
curl http://127.0.0.1:3000/api/data/airport-express-fares
```

### 响应示例

`200 OK`。以下为节选：实际有 14 个行程，这里只展示 1 个。

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

### 字段说明

| 字段 | 类型 | 说明 |
|---|---|---|
| `updated_at`、`fetched_at`、`stale` | | 见[开放数据接口的共同行为](#开放数据接口的共同行为) |
| `fares` | array | 行程，按起点代码、再按终点代码排序 |
| `fares[].from`、`fares[].to` | string | 起点、终点车站代码 |
| `fares[].octopus.adult`、`fares[].octopus.child` | integer | 八达通成人、小童车费（港仙） |
| `fares[].single_journey.adult`、`fares[].single_journey.child` | integer | 单程票成人、小童车费（港仙） |

### 缓存行为

见[开放数据接口的共同行为](#开放数据接口的共同行为)。

### 错误

| HTTP 状态 | `code` | 触发条件 |
|---|---|---|
| `502` | `upstream_unavailable` | 从未成功拉取到开放数据，或最近一次成功拉取已超过新鲜期加 30 天 |

---

## 11. 获取轻铁车站与路线

```
GET /api/data/light-rail
```

返回轻铁的所有车站，以及每条路线在每个方向依次停靠的车站，来自 `light_rail_routes_and_stops.csv`。

- 轻铁车站以数字 `id` 标识，港铁轻铁实时到站 API 用的也是这个数字。
- 轻铁车站的三字母 `code` 与重铁车站代码是两套独立的编号，不能混用。

清洗内容：

- 路线按路线号排序（`614` 在 `614P` 之前），不按文件里的顺序。
- 同一站在路线上连续出现两次（总站掉头处）时只保留一次。

### 参数

无。

### 请求示例

```bash
curl http://127.0.0.1:3000/api/data/light-rail
```

### 响应示例

`200 OK`。以下为节选：实际有 68 个车站、11 条路线，这里只展示 1 个车站和 505 路线。

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

### 字段说明

| 字段 | 类型 | 说明 |
|---|---|---|
| `updated_at`、`fetched_at`、`stale` | | 见[开放数据接口的共同行为](#开放数据接口的共同行为) |
| `stops` | array | 车站，按 `id` 排序 |
| `stops[].id` | integer | 车站编号 |
| `stops[].code` | string | 三字母车站代码 |
| `stops[].name` | object | 双语站名 `{ en, tc }` |
| `stops[].routes` | array of string | 停靠该站的路线号，按路线号排序 |
| `routes` | array | 路线，按路线号排序 |
| `routes[].route` | string | 路线号，例如 `505`、`614P` |
| `routes[].directions` | array | 各方向，顺序与港铁文件的方向 1、2 一致 |
| `routes[].directions[].from` | object | 起点 `{ id, name }` |
| `routes[].directions[].towards` | object | 终点 `{ id, name }` |
| `routes[].directions[].stops` | array of integer | 依次停靠的车站编号。天水围的循环线（705、706）在文件里被拆成两段，在折返处相接 |

### 缓存行为

见[开放数据接口的共同行为](#开放数据接口的共同行为)。

### 错误

| HTTP 状态 | `code` | 触发条件 |
|---|---|---|
| `502` | `upstream_unavailable` | 从未成功拉取到开放数据，或最近一次成功拉取已超过新鲜期加 30 天 |

---

## 12. 获取轻铁车费

```
GET /api/data/light-rail-fares
```

返回轻铁任意两站之间的车费，来自 `light_rail_fares.csv`。车站以编号标识，与第 11 节的 `stops[].id` 对应。票种与港铁车费相同，金额单位是**港仙**。

### 参数

无。

### 请求示例

```bash
curl --compressed http://127.0.0.1:3000/api/data/light-rail-fares
```

响应约 810 KB，gzip 后约 17 KB。

### 响应示例

`200 OK`。以下为节选：实际有 4556 个行程，这里只展示 1 个。

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

### 字段说明

| 字段 | 类型 | 说明 |
|---|---|---|
| `updated_at`、`fetched_at`、`stale` | | 见[开放数据接口的共同行为](#开放数据接口的共同行为) |
| `fares` | array | 行程，按起点编号、再按终点编号排序；去掉起点和终点相同的行 |
| `fares[].from`、`fares[].to` | integer | 起点、终点轻铁车站编号 |
| `fares[].octopus`、`fares[].single_journey` | object | 与[港铁车费](#9-获取港铁车费)的同名字段相同 |

### 缓存行为

见[开放数据接口的共同行为](#开放数据接口的共同行为)。

### 错误

| HTTP 状态 | `code` | 触发条件 |
|---|---|---|
| `502` | `upstream_unavailable` | 从未成功拉取到开放数据，或最近一次成功拉取已超过新鲜期加 30 天 |

---

## 13. 获取无障碍设施

```
GET /api/data/accessibility
```

返回港铁的无障碍设施目录，以及每个车站提供的设施，由 `barrier_free_facility_category.csv`（目录）和 `barrier_free_facilities.csv`（各站情况）合并而成。

清洗内容：

- 数字车站 ID 换成车站代码，马场按车费文件里的站名对应到 `RAC`。
- 设施名称里的 HTML 实体（例如 `&#32171;`）解码成文字（綫）。
- 每个车站只列出有提供（`Y`）的设施。一个设施也不提供的记录（例如无法识别的车站 `888`）不列出。
- 位置文字去掉多余空格，换行保留为 `\n`。

### 参数

无。

### 请求示例

```bash
curl --compressed http://127.0.0.1:3000/api/data/accessibility
```

### 响应示例

`200 OK`。以下为节选：实际有 4 个类别、36 种设施、98 个车站，这里每处只展示几项。

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

### 字段说明

| 字段 | 类型 | 说明 |
|---|---|---|
| `updated_at`、`fetched_at`、`stale` | | 见[开放数据接口的共同行为](#开放数据接口的共同行为)。`updated_at` 取两个文件中较晚的一个 |
| `categories` | array | 设施类别，顺序固定：`station_access`、`visually_impaired`、`hearing_impaired`、`mobility_impaired` |
| `categories[].category` | string | 类别：`station_access` 是进出车站的方式（同层、斜道、升降机、轮椅升降台等，港铁称"出入口設施"）；另外三个分别是视障、听障、行动不便人士设施 |
| `categories[].name` | object | 港铁的双语类别名 |
| `categories[].facilities` | array | 该类别下的设施，按港铁的排序 |
| `categories[].facilities[].code` | string | 设施代码，例如 `AJ3`，大小写照原样（例如 `VIn1`） |
| `categories[].facilities[].name` | object | 双语设施名 |
| `stations` | array | 车站，按车站代码排序 |
| `stations[].station` | object | 车站 `{ code, name }`，站名来自服务内置的静态资料 |
| `stations[].facilities` | array | 该站提供的设施，按目录顺序 |
| `stations[].facilities[].code` | string | 设施代码，对应 `categories[].facilities[].code` |
| `stations[].facilities[].location` | object 或 `null` | 设施位置的双语文字，例如 `Exits A & D`；港铁没给时为 `null`。它是自由文本，格式不统一，适合直接显示，不适合解析 |

### 缓存行为

见[开放数据接口的共同行为](#开放数据接口的共同行为)。

### 错误

| HTTP 状态 | `code` | 触发条件 |
|---|---|---|
| `502` | `upstream_unavailable` | 从未成功拉取到开放数据，或最近一次成功拉取已超过新鲜期加 30 天 |

---

## Mock 接口的共同行为

第 14 至 17 节的 Mock 接口供 App 开发调试使用：按指定的场景返回模拟数据，例如繁忙时间、尾班车、延误、八号风球、港铁故障，不必等真实情况出现，也能随机抽取场景检查 App 的各种显示。

- **默认关闭。** 用 `--mock-api` 或环境变量 `DUT_MOCK_API=true`（也接受 `1`、`yes`、`on`）开启，见 README 的"配置"。未开启时，这些路径与其他不存在的路由一样返回 `404 not_found`。乘客使用的正式部署不需要开启：App 正式版万一误调 Mock 接口，得到的是 404，而不是把模拟数据显示给乘客。
- **与正式接口相同。** 路径就是正式接口在 `/api` 后加上 `/mock`，例如 `/api/mock/lines/status`。响应体结构、`Cache-Control`、`x-request-id`、gzip 和错误格式都与正式接口一致，所以客户端只需换路径前缀，字段说明见对应的正式接口。
- **不访问上游。** 数据全部由服务模拟生成，港铁不可用时照样能用。

### 查询参数

所有 Mock 接口（场景列表除外）都接受以下两个查询参数。正式接口没有这两个参数，也会忽略它们。

| 参数 | 类型 | 必填 | 说明 | 示例 |
|---|---|---|---|---|
| `scenario` | string | 否 | 场景名，大小写不敏感。可选值见各接口的场景表和[场景列表](#14-mock-场景列表)；`random` 表示按权重随机抽一个场景。不传等于 `random` | `typhoon_signal` |
| `seed` | integer | 否 | 0 到 18446744073709551615 之间的整数，决定模拟出哪一组数据，见下表 | `42` |

同一个 `seed` 就是同一个"模拟世界"：同一场景下的列车班次、受影响的线路和通告文字都相同。

| | 不传 `seed` | 传 `seed` |
|---|---|---|
| 指定场景 | 使用 `0`，所以连续轮询、或在不同车站之间切换时，看到的是同一个世界 | 使用这个 `seed` |
| `random` | 每次请求随机抽一个 `seed`，也就每次抽到不同的场景 | 由 `seed` 决定抽中哪个场景，所以可以重现 |

列车按绝对时间运行：固定 `seed` 轮询时，列车会像真实数据一样逐渐接近、到站、离开，下一个车站也会在一两分钟后看到同一班车。想要另一组数据，换一个 `seed` 即可。

### 响应头

除参数错误（`400`、`404`）外，每个模拟响应都带这两个响应头，包括模拟出来的 `502`：

| 响应头 | 说明 |
|---|---|
| `x-mock-scenario` | 实际模拟的场景。`scenario=random` 时是抽中的场景 |
| `x-mock-seed` | 实际使用的 `seed` |

用 `?scenario=<x-mock-scenario>&seed=<x-mock-seed>` 再请求一次，就能重现同一份数据（时间会随当前时刻推进）。

### 模拟数据有多接近真实

- **线路与车站**：线路、车站、支线、行车方向和 `towards` 都来自服务内置的静态资料，与正式接口完全相同。
- **月台**：取自 2026-10-02 早上两次抓取的全部车站的港铁实时数据。包括同一车站不同线路的月台编号重复（例如美孚两条线都有 1 号月台）、换乘站的特殊编号（例如金鐘港島綫往柴灣是 3 号），以及终点站轮流使用两个月台（例如中環荃灣綫 1、2 号）。机场站的机场快綫列车与正式接口一样停靠两个月台（`[1, 3]` 和 `[2, 4]`）。
- **班次**：各线按下表的班距行车，并有真实的中途折返班次：觀塘綫隔一班往何文田；將軍澳綫每三班有一班往康城，深夜康城列车只往返調景嶺；東鐵綫平时每三班有一班往落馬洲，繁忙时间每四班中一班往落馬洲、一班只到大埔墟，深夜每三班有一班只到上水；東涌綫繁忙时间每三班有一班只到青衣；機場快綫全部驶往博覽館。支线车站的班次相应较疏，例如坑口只有往寶琳的列车。
- **时间格式**：与港铁一样，`arrival_at` 等于 `generated_at` 加整数分钟，正在月台上的列车为 0 分钟；`generated_at` 比 `fetched_at` 早 2 至 8 秒；数据每 10 秒刷新一次，`max-age` 与正式接口一样在 2 至 10 秒之间。東鐵綫在始发站（金鐘、羅湖、落馬洲）给出 `departure`，其余车站给出 `arrival`。
- **不规则**：每班车都有少量随机偏差，但不会超越前车。`delayed` 场景下班距拉长到 1.7 倍，偏差更大，约八分之一的班次取消，所以会出现列车扎堆和长时间空档。
- **线路状态**：线路顺序与港铁状态源相同（荃灣綫在前，轻铁在最后）。说明文字仿照港铁通告的英文写法，例如 "Due to a signalling fault at Kowloon Bay Station, Kwun Tong Line train service is delayed. Passengers please allow extra travelling time."

各线班距（分钟，取整到 0.1）：

| 线路 | 繁忙时间 | 非繁忙时间 | 深夜 |
|---|---|---|---|
| `AEL` | 10 | 10 | 12 |
| `TCL` | 5 | 7.5 | 10 |
| `TML` | 2.8 | 4.5 | 7.5 |
| `TKL` | 2.3 | 4 | 6 |
| `EAL` | 2.8 | 5 | 7.5 |
| `SIL` | 3.3 | 4.5 | 7 |
| `TWL` | 2.1 | 3.5 | 6 |
| `ISL` | 2.5 | 3.5 | 6 |
| `KTL` | 2.1 | 3.5 | 6 |
| `DRL` | 6 | 8 | 10 |

模拟数据不是时刻表：班距是概略值，与当前真实时刻无关，场景也与真实日期无关（例如非赛马日也能请求 `race_day`）。

---

## 14. Mock 场景列表

```
GET /api/mock/scenarios
```

列出每类 Mock 数据可以模拟的场景、说明和随机权重，方便 App 做一个开发用的场景选择菜单，不必把场景名写死。

### 参数

无。

### 请求示例

```bash
curl http://127.0.0.1:3000/api/mock/scenarios
```

### 响应示例

`200 OK`，响应头包含 `Cache-Control: public, max-age=86400`。

以下为节选：`next_trains` 实际有 11 个场景，`line_status` 有 9 个，这里各展示 1 个。

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

### 字段说明

| 字段 | 类型 | 说明 |
|---|---|---|
| `next_trains` | array | 列车到站 Mock 接口（[第 16 节](#16-模拟单线单站列车到站)、[第 17 节](#17-模拟车站所有线路的列车到站)）的场景，顺序固定 |
| `line_status` | array | 线路状态 Mock 接口（[第 15 节](#15-模拟全线路服务状态)）的场景，顺序固定 |
| `*[].scenario` | string | 场景名，即查询参数 `scenario` 的值 |
| `*[].random_weight` | integer | `scenario=random` 时被抽中的相对权重。`0` 表示只能指定，不会被随机抽中 |
| `*[].description` | object | 双语说明 `{ en, tc }`，适合直接显示在开发菜单里 |

### 缓存行为

场景随部署更新，响应头与[接口 1](#1-获取线路与车站资料) 一样是 `Cache-Control: public, max-age=86400`。

### 错误

本接口没有业务错误。未开启 Mock 接口时返回 `404 not_found`。

---

## 15. 模拟全线路服务状态

```
GET /api/mock/lines/status
```

按场景模拟[接口 2](#2-获取全线路服务状态) 的响应，响应体与之完全相同。受影响的线路由 `seed` 决定，可能是任何一条线，包括轻铁。

### 查询参数

`scenario` 和 `seed`，见[查询参数](#查询参数)。`scenario` 的可选值：

| `scenario` | 模拟的情况 | 随机权重 |
|---|---|---|
| `normal` | 所有线路服务正常 | 8 |
| `delayed` | 一条线 `delayed`（黄色），附港铁说明 | 3 |
| `disrupted` | 一条线 `disrupted`（红色），部分路段暂停服务，附港铁说明 | 1 |
| `delayed_or_disrupted` | 一条线 `delayed_or_disrupted`（港铁网站以黄色显示），附港铁说明 | 1 |
| `typhoon_signal` | 所有线路 `typhoon_signal`，没有说明文字 | 1 |
| `non_service_hours` | 所有线路 `non_service_hours`（灰色） | 2 |
| `unknown_condition` | 一条线是本服务尚未识别的状态（港铁原始值 `blue`），即 `unknown`、灰色。用于检查 App 能否显示未来新增的状态 | 0 |
| `stale` | 与 `normal` 相同，但数据是 2 至 12 分钟前的：`stale: true`、`Cache-Control: no-cache` | 1 |
| `upstream_unavailable` | 返回 `502 upstream_unavailable` | 1 |

`updated_at` 也按真实情况模拟：`normal` 和 `stale` 是当天 06:15（港铁每天开始服务时重新发布），`non_service_hours` 是 01:20，其余场景是最近一小时内的某个时刻。

### 请求示例

```bash
curl -i 'http://127.0.0.1:3000/api/mock/lines/status?scenario=disrupted&seed=8'
```

### 响应示例

`200 OK`，响应头包含 `Cache-Control: public, max-age=29`、`x-mock-scenario: disrupted`、`x-mock-seed: 8`。

以下为节选：实际返回 11 条线路，这里只展示 3 条。

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

### 字段说明

与[接口 2](#2-获取全线路服务状态) 相同。

### 缓存行为

与[接口 2](#2-获取全线路服务状态) 相同：数据视为每 30 秒拉取一次，`max-age` 最多 33 并随时间递减；`stale` 场景为 `no-cache`。

### 错误

| HTTP 状态 | `code` | 触发条件 |
|---|---|---|
| `400` | `unknown_scenario` | `scenario` 不是上表的场景名，也不是 `random` |
| `400` | `invalid_query` | `seed` 不是 0 到 18446744073709551615 的整数，或某个参数出现了两次 |
| `404` | `not_found` | 服务没有开启 Mock 接口 |
| `502` | `upstream_unavailable` | 场景为 `upstream_unavailable` |

错误请求示例：

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

---

## 16. 模拟单线单站列车到站

```
GET /api/mock/lines/{line}/stations/{station}/next-trains
```

按场景模拟[接口 3](#3-获取单线单站列车到站) 的响应，响应体与之完全相同。延误、特别安排等事件发生在所请求的线路上。

### 路径参数

与[接口 3](#3-获取单线单站列车到站) 相同。

### 查询参数

`scenario` 和 `seed`，见[查询参数](#查询参数)。`scenario` 的可选值：

| `scenario` | 模拟的情况 | 随机权重 |
|---|---|---|
| `peak` | 繁忙时间班次，包括中途折返的班次 | 3 |
| `off_peak` | 非繁忙时间班次 | 4 |
| `late_night` | 深夜班次，康城列车只往返調景嶺 | 2 |
| `last_train` | 尾班车：部分方向只剩一两班，部分已经没有列车。每 20 分钟重演一次，所以轮询时能看到列车逐班开走 | 1 |
| `non_service_hours` | 非服务时间：方向照常列出，`trains` 为空数组 | 1 |
| `delayed` | 繁忙时间，事件线路 `delayed: true`，班次脱班、扎堆、部分取消 | 2 |
| `special_arrangement` | 非繁忙时间，事件线路附港铁的特别列车服务安排通告 `alert`，列车照常列出 | 1 |
| `race_day` | 沙田赛马日：部分东铁綫列车经马场站（`via_racecourse: true`），不停火炭站。其他日子马场站没有列车 | 1 |
| `stale` | 非繁忙时间，数据是约 40 至 90 秒前的：`stale: true`、`Cache-Control: no-cache` | 1 |
| `partial_outage` | 事件线路取不到数据。本接口返回 `502`；[第 17 节](#17-模拟车站所有线路的列车到站)只有这条线失败 | 1 |
| `upstream_unavailable` | 返回 `502 upstream_unavailable` | 1 |

`alert` 的内容与港铁 Next Train API 一致：

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

### 请求示例

```bash
curl -i 'http://127.0.0.1:3000/api/mock/lines/EAL/stations/SHT/next-trains?scenario=peak&seed=2'
```

### 响应示例

`200 OK`，响应头包含 `Cache-Control: public, max-age=7`、`x-mock-scenario: peak`、`x-mock-seed: 2`。

以下为节选：`down` 实际有 4 班车，这里只展示 1 班。`up` 的最后一班是只到大埔墟的中途折返班次。

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

### 字段说明

与[接口 3](#3-获取单线单站列车到站) 相同。

### 缓存行为

与[接口 3](#3-获取单线单站列车到站) 相同：数据视为每 10 秒刷新一次，`max-age` 在 2 至 10 秒之间；`stale` 场景为 `no-cache`。轮询间隔可以照样用 `max-age`。

### 错误

| HTTP 状态 | `code` | 触发条件 |
|---|---|---|
| `400` | `unknown_scenario` | `scenario` 不是上表的场景名，也不是 `random` |
| `400` | `invalid_query` | `seed` 不是 0 到 18446744073709551615 的整数，或某个参数出现了两次 |
| `404` | `not_found` | 服务没有开启 Mock 接口 |
| `404` | `unknown_line`、`unknown_station`、`station_not_on_line` | 与[接口 3](#3-获取单线单站列车到站) 相同，先于查询参数检查 |
| `502` | `upstream_unavailable` | 场景为 `partial_outage` 或 `upstream_unavailable` |

错误请求示例（响应头包含 `x-mock-scenario: upstream_unavailable`、`x-mock-seed: 0`，不带 `Cache-Control`）：

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

---

## 17. 模拟车站所有线路的列车到站

```
GET /api/mock/stations/{station}/next-trains
```

按场景模拟[接口 4](#4-获取车站所有线路的列车到站) 的响应，响应体与之完全相同。场景、班次和月台与[第 16 节](#16-模拟单线单站列车到站)相同；延误、特别安排、`partial_outage` 这类事件只发生在途经该站的其中一条线上（由 `seed` 决定），因为真实事件很少同时影响所有线路。在只有一条线的车站，`partial_outage` 等于所有线路失败，返回 `502`。

### 路径参数

与[接口 4](#4-获取车站所有线路的列车到站) 相同。

### 查询参数

`scenario` 和 `seed`，见[查询参数](#查询参数)。`scenario` 的可选值与[第 16 节](#16-模拟单线单站列车到站)相同。

### 请求示例

```bash
curl -i 'http://127.0.0.1:3000/api/mock/stations/ADM/next-trains?scenario=partial_outage'
```

### 响应示例

`200 OK`，响应头包含 `Cache-Control: public, max-age=6`、`x-mock-scenario: partial_outage`、`x-mock-seed: 0`。

以下为节选：实际返回 EAL、SIL、TWL、ISL 四条线，这里只展示 EAL（只保留 1 班车）和失败的 TWL。

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

### 字段说明

与[接口 4](#4-获取车站所有线路的列车到站) 相同。

### 缓存行为

与[接口 4](#4-获取车站所有线路的列车到站) 相同。

### 错误

| HTTP 状态 | `code` | 触发条件 |
|---|---|---|
| `400` | `unknown_scenario` | `scenario` 不是[第 16 节](#16-模拟单线单站列车到站)的场景名，也不是 `random` |
| `400` | `invalid_query` | `seed` 不是 0 到 18446744073709551615 的整数，或某个参数出现了两次 |
| `404` | `not_found` | 服务没有开启 Mock 接口 |
| `404` | `unknown_station` | 与[接口 4](#4-获取车站所有线路的列车到站) 相同，先于查询参数检查 |
| `502` | `upstream_unavailable` | 场景为 `upstream_unavailable`，或在只有一条线的车站为 `partial_outage` |

---

## 错误码

| HTTP 状态 | `code` | 说明 |
|---|---|---|
| `400` | `invalid_query` | 查询参数格式错误或重复，目前只有 Mock 接口读取查询参数 |
| `400` | `unknown_scenario` | Mock 场景名不存在 |
| `404` | `not_found` | 路由不存在，例如 `GET /api/nope`；未开启 Mock 接口时的 `/api/mock/*` 也是如此 |
| `404` | `unknown_line` | 线路代码不存在 |
| `404` | `unknown_station` | 车站代码格式错误或车站不存在 |
| `404` | `station_not_on_line` | 线路不经过该车站 |
| `404` | `unknown_source` | 开放数据文件名不存在 |
| `502` | `upstream_unavailable` | 上游数据源（港铁）不可用，且没有可用的旧数据 |

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
| 2026-10-02 | **不兼容变更。** 列车到站接口（`GET /api/lines/{line}/stations/{station}/next-trains`、`GET /api/stations/{station}/next-trains`）和对应的 Mock 接口中，列车的 `platform`（整数）改为 `platforms`（整数数组）。通常只有一个月台，例如 `[1]`；机场站的机场快綫列车两侧开门，为 `[1, 3]` 或 `[2, 4]`；港铁给出的月台无法识别时为 `[]`，列车照常列出。此前港铁在机场站返回 `1/3` 这样的月台，导致 `GET /api/lines/AEL/stations/AIR/next-trains` 和 `GET /api/stations/AIR/next-trains`（机场站只有机场快綫）都返回 `502 upstream_unavailable`。 |
| 2026-10-02 | 新增 Mock 接口，供 App 开发调试：`GET /api/mock/scenarios`、`GET /api/mock/lines/status`、`GET /api/mock/lines/{line}/stations/{station}/next-trains`、`GET /api/mock/stations/{station}/next-trains`。按 `scenario`（或 `random`）和 `seed` 查询参数返回与正式接口结构相同的模拟数据，响应头带 `x-mock-scenario` 和 `x-mock-seed`。默认关闭，用 `--mock-api` 或 `DUT_MOCK_API` 开启。新增错误码 `400 invalid_query`、`400 unknown_scenario`。正式接口不变。 |
| 2026-10-01 | 开放数据的数据集（`GET /api/data/stations` 等 6 个）和原始文件（`GET /api/data/sources/{file}`）改为每次拉取只编码一次、所有请求共享，gzip 改用最高压缩级别：`/api/data/fares` 从约 75 KB 降到约 71 KB。响应改带 `Content-Length` 和 `Vary: Accept-Encoding`。解压后的内容、`ETag` 和 `Cache-Control` 不变。 |
| 2026-09-29 | 新增港铁开放数据接口：`GET /api/data`（索引）、`GET /api/data/sources/{file}`（原样的 CSV 文件）、`GET /api/data/stations`、`GET /api/data/fares`、`GET /api/data/airport-express-fares`、`GET /api/data/light-rail`、`GET /api/data/light-rail-fares`、`GET /api/data/accessibility`。数据每天拉取一次，响应带 `ETag`，支持 `If-None-Match` 返回 `304`；车费以港仙整数表示。所有响应在客户端接受时以 gzip 压缩。新增错误码 `404 unknown_source`。 |
| 2026-09-29 | 新增 `GET /api/health` 健康检查：返回 `200`、空响应体和 `Cache-Control: no-store`，不访问上游，不记入请求日志。 |
| 2026-09-28 | `GET /api/lines/status` 改为读取后台每 30 秒一次的轮询结果。`max-age` 最多为 33，并随距下一次轮询的时间递减；上游正常时不再出现 `stale: true`。响应字段不变。 |
| 2026-09-28 | 文档修正：错误码表移除 `500 internal_error`。服务从未返回过这个错误码，客户端行为不受影响。 |
| 2026-09-28 | **不兼容变更。** 列车到站接口（`GET /api/lines/{line}/stations/{station}/next-trains`、`GET /api/stations/{station}/next-trains`）的 `up` / `down` 数组改为 `directions` 数组，每个方向带 `direction`、`towards`、`trains`；本站是某方向终点时不再返回该方向；列车移除 `sequence`，改以数组顺序表示先后。`GET /api/lines` 的 `destinations` 改为 `directions`，`towards` 只列月台指示牌上的主要终点。 |
| 2026-09-28 | 首版。新增 `GET /api/lines`、`GET /api/lines/status`、`GET /api/lines/{line}/stations/{station}/next-trains`、`GET /api/stations/{station}/next-trains`。移除 Hello 示例接口 `GET /api/hello`、`GET /api/hello.json`。 |
