# HTTP API 文件

[English](../../HTTP_API.md) · [繁體廣東話](HTTP_API.md) · [简体中文](../zh-CN/HTTP_API.md)

呢份文件係 Dut 嘅客戶端合約。接口實作有改動時，同一次改動要更新 URL、方法、參數、範例、欄位、快取、錯誤同變更記錄，並同步全部語言版本。下面嘅實際回應保留自原有文件，係歷史範例，唔係即時資料；節錄仍然係有效 JSON。模擬同示意範例會另外註明。

## 目錄

- [概覽](#overview)
- [共通約定](#conventions)
- [1. 路綫同車站](#lines)
- [2. 全網服務狀態](#line-status)
- [3. 單綫下一班車](#next-trains)
- [4. 全站下一班車](#station-next-trains)
- [5. 存活檢查](#health)
- [開放數據：共同行為](#open-data)
- [6. 開放數據索引](#data-index)
- [7. 原始檔案](#source-files)
- [8. 公開車站同路綫](#published-stations)
- [9. 重鐵車費](#fares)
- [10. 機場快綫車費](#airport-express-fares)
- [11. 輕鐵車站同路綫](#light-rail)
- [12. 輕鐵車費](#light-rail-fares)
- [13. 無障礙設施](#accessibility)
- [Mock API：共同行為](#mock-api)
- [14. 場景目錄](#mock-scenarios)
- [15. 模擬全網狀態](#mock-line-status)
- [16. 模擬單綫到站板](#mock-next-trains)
- [17. 模擬全站到站板](#mock-station-next-trains)
- [18. 模擬事件流](#mock-events)
- [錯誤碼](#errors)
- [附錄 A：路綫代碼](#line-codes)
- [附錄 B：車站代碼](#station-codes)
- [變更記錄](#changelog)

<a id="overview"></a>

## 概覽

本機 base URL 係 `http://127.0.0.1:3000`。全部路由喺 `/api` 下，用 `GET`；健康檢查亦支援 `HEAD`。目前冇鑑權。JSON 回應用 UTF-8 `application/json`；原始檔用 `text/csv; charset=utf-8`；模擬事件流用 `text/event-stream`。健康檢查冇 body 或 content type。gzip 由 `Accept-Encoding` 協商。Mock 路由要用 `--mock-api` 或 `DUT_MOCK_API` 啟用。

| 方法 | URL | 用途 |
| --- | --- | --- |
| `GET` | `/api/lines` | 編譯內置嘅路綫、車站、顏色同行車方向 |
| `GET` | `/api/lines/status` | 全網服務狀態，包括輕鐵 |
| `GET` | `/api/lines/{line}/stations/{station}/next-trains` | 某站某條綫嘅下一班車 |
| `GET` | `/api/stations/{station}/next-trains` | 某站所有路綫嘅下一班車 |
| `GET` | `/api/health` | 存活檢查；空白 200 回應 |
| `GET` | `/api/data` | 數據集同原始檔案版本 |
| `GET` | `/api/data/sources/{file}` | 港鐵原始 CSV 位元組 |
| `GET` | `/api/data/stations` | 公開車站同路綫資料 |
| `GET` | `/api/data/fares` | 重鐵車費，以港仙計 |
| `GET` | `/api/data/airport-express-fares` | 機場快綫車費 |
| `GET` | `/api/data/light-rail` | 輕鐵車站同路綫 |
| `GET` | `/api/data/light-rail-fares` | 輕鐵車費 |
| `GET` | `/api/data/accessibility` | 設施目錄同各站設施 |
| `GET` | `/api/mock/scenarios` | 可用模擬場景；要先啟用 |
| `GET` | `/api/mock/lines/status` | 模擬服務狀態 |
| `GET` | `/api/mock/lines/{line}/stations/{station}/next-trains` | 模擬單綫到站板 |
| `GET` | `/api/mock/stations/{station}/next-trains` | 模擬全站到站板 |
| `GET` | `/api/mock/events` | 模擬綫路狀態事件（Server-Sent Events）；要啟用 |

<a id="conventions"></a>

## 共通約定

- **時間：** RFC 3339，香港時差 `+08:00`，精確到秒。倒數用 `arrival_at - 裝置時間`，唔好用 `generated_at` 或 `fetched_at`；上游 `ttnt` 已丟棄。
- **代碼：** 路徑嘅路綫同車站代碼唔分大小寫；回應統一大寫。
- **名稱：** `{ "en": "...", "tc": "..." }` 一次提供英文同繁體中文。文件語言唔會改 JSON 合約。
- **金額：** 整數港仙，`490` 即 HK$4.90；顯示時除以 100。
- **方向：** `up` 同 `down` 係港鐵穩定 ID，唔係羅盤方向。月台標題用 `towards`，每班實際終點用 `destination`；中途折返仍屬同一方向。迪士尼綫 `up` 係往欣澳，支綫亦唔可以單靠一份平面車站清單推方向。

| Header／欄位 | 意思 |
| --- | --- |
| `Cache-Control: public, max-age=N` | 剩餘新鮮期秒數，可作輪詢提示 |
| `Cache-Control: no-cache` | 重用之前要驗證；仍然可以儲存 |
| `stale` | 資料已過新鮮期，喺頂替視窗內回傳 |
| `generated_at` / `updated_at` | 上游產生／發佈時間；發佈得早唔代表資料過時 |
| `fetched_at` | Dut 最近成功抓取時間 |
| `ETag` | 數據集／檔案驗證值，下次放入 `If-None-Match` |
| `x-request-id` | 有追蹤嘅路由會產生 UUID 或沿用客戶端 header；健康檢查冇 |

應用錯誤用下面嘅 JSON，`code` 穩定，`message` 係英文。錯誤冇 `Cache-Control`。上游細節只喺伺服器日誌。查找錯誤唔會照抄提交嘅代碼；`not_found` 訊息就會包含未匹配路徑。

```json
{
  "error": {
    "code": "unknown_station",
    "message": "No station matches the requested station code"
  }
}
```

<a id="lines"></a>

## 1. 路綫同車站

```http
GET /api/lines
```

冇路徑參數。

唔讀取查詢參數。

按內置順序回傳十條 Next Train 路綫：AEL、TCL、TML、TKL、EAL、SIL、TWL、ISL、KTL、DRL。導航資料下載一次就用得，隨部署先改。`Cache-Control: public, max-age=86400`；冇業務錯誤。

```bash
curl http://127.0.0.1:3000/api/lines
```

`200 OK`，`application/json`。保留嘅歷史回應節錄：

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

| 欄位 | 型別 | 意思 |
| --- | --- | --- |
| `lines` | array | 有序路綫 |
| `lines[].code`, `lines[].name` | string, object | 路綫代碼同雙語名 |
| `lines[].color` | string | 品牌色 `#RRGGBB` |
| `lines[].stations` | array | 按路綫順序排列嘅車站引用 |
| `lines[].directions` | array | 先 `up` 後 `down`，各有 `direction` 同車站引用陣列 `towards` |

<a id="station-reference"></a>

### 車站引用

| 欄位 | 型別 | 意思 |
| --- | --- | --- |
| `code` | string | 三個大寫字母 |
| `name` | object \| null | `{ en, tc }`；只有上游返回內置路網未知車站先係 null |

<a id="line-status"></a>

## 2. 全網服務狀態

```http
GET /api/lines/status
```

冇路徑參數。

唔讀取查詢參數。

```bash
curl http://127.0.0.1:3000/api/lines/status
```

包括十一條綫，輕鐵喺上游順序最後。`condition` 用嚟做邏輯，`display` 跟港鐵網站顯示。

`200 OK`，`application/json`。保留嘅歷史回應節錄：

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

| 欄位 | 型別 | 意思 |
| --- | --- | --- |
| `updated_at`, `fetched_at`, `stale` | string, string, boolean | 發佈時間、抓取時間同新鮮度旗標 |
| `lines` | array | 上游路綫順序 |
| `lines[].line`, `lines[].color` | object, string | 路綫 `{ code, name }` 同品牌色 |
| `lines[].condition`, `lines[].display` | string | 下面嘅語義狀態同展示值 |
| `lines[].message` | string \| null | 上游有提供就附說明 |

| 上游 | condition | display | 意思 |
| --- | --- | --- | --- |
| green | `normal` | green | 服務正常 |
| yellow | `delayed` | yellow | 延誤或逐步恢復 |
| red | `disrupted` | red | 服務受阻，考慮其他交通 |
| pink | `delayed_or_disrupted` | yellow | 延誤或受阻 |
| grey | `non_service_hours` | grey | 非服務時間 |
| typhoon | `typhoon_signal` | typhoon | 熱帶氣旋信號 |
| 其他 | `unknown` | grey | 未知狀態，會記 warn 日誌 |

延誤路綫元素示意，唔係實際抓取：

```json
{
  "line": { "code": "KTL", "name": { "en": "Kwun Tong Line", "tc": "觀塘綫" } },
  "color": "#1A9431",
  "condition": "delayed_or_disrupted",
  "display": "yellow",
  "message": "Trains are delayed"
}
```

每 30 秒輪詢；成功抓取起計新鮮期 33 秒，`max-age` 最多 33，隨資料年齡減少。請求讀輪詢結果，啟動時等首次輪詢。過期值可以再頂替 15 分鐘，用 `stale: true` 同 `no-cache`。冇可用快照就 `502 upstream_unavailable`。

<a id="next-trains"></a>

## 3. 單綫下一班車

```http
GET /api/lines/{line}/stations/{station}/next-trains
```

| 路徑參數 | 意思 |
| --- | --- |
| `line` | 必填路綫代碼；輕鐵 `LR` 冇到站板 |
| `station` | 必填，要係指定路綫嘅已知車站 |

唔讀取查詢參數。

```bash
curl http://127.0.0.1:3000/api/lines/TKL/stations/TKO/next-trains
```

每方向最多四班車，開出超過 30 秒寬限期會濾走。終點方向唔列出，除非上游仍然報有車。非服務時間，有效方向可以冇車。`towards` 列主要可到終點，中途折返睇每班 `destination`。

`200 OK`，`application/json`。保留嘅歷史回應節錄：

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

| 欄位 | 型別 | 意思 |
| --- | --- | --- |
| `line`, `station` | object | 路綫同車站引用 |
| `generated_at`, `fetched_at` | string | 上游產生同 Dut 抓取時間 |
| `stale`, `delayed` | boolean | 舊資料頂替；港鐵延誤旗標，兩者獨立 |
| `alert` | object \| null | `{ en: notice, tc: notice }`，各通告有 `message` 同可空 `url` |
| `directions` | array | 可乘車方向，`up` 在先 |
| `directions[].direction` | string | `up` 或 `down` |
| `directions[].towards` | array | 主要終點引用；如果已過最後主要終點但上游仍有車，可以係空陣列 |
| `directions[].trains` | array | 按到站時間排序，最多四班 |
| `directions[].trains[].destination` | object | 列車實際終點引用 |
| `directions[].trains[].platforms` | array of integer | 按上游順序嘅月台，通常 `[1]`；機場往博覽館係 `[1, 3]`，往香港係 `[2, 4]`。無法辨認就 `[]`，保留列車 |
| `directions[].trains[].arrival_at` | string | 預計到站，`time_type=departure` 時係開出時間；用嚟倒數 |
| `directions[].trains[].time_type` | string \| null | 東鐵：`arrival` 或 `departure`；其他綫 null |
| `directions[].trains[].via_racecourse` | boolean | 東鐵經馬場而唔經火炭；其他綫 false |

另外保留咗寶琳開出方向、一班東鐵列車，同 2026-10-02 機場站一班機場快綫嘅節錄。第一個物件只有 `directions`，唔係完整回應。

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

特別安排通告示意，唔係實際抓取：

```json
{
  "alert": {
    "en": { "message": "Special train service arrangement", "url": "https://example.com/notice" },
    "tc": { "message": "特別列車服務安排", "url": "https://example.com/notice" }
  }
}
```

新鮮期跟 CDN 剩餘 TTL，通常 10 秒，限制喺 2–15。到站板過期會等一次合併刷新。刷新失敗可以喺過期後 90 秒內回傳舊資料，標記 `stale: true`；重試退避 5 秒。錯誤有 `404 unknown_line`、`404 unknown_station`、`404 station_not_on_line`（包括 `LR`）；冇可用板就 `502 upstream_unavailable`。

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

## 4. 全站下一班車

```http
GET /api/stations/{station}/next-trains
```

| 路徑參數 | 意思 |
| --- | --- |
| `station` | 必填已知車站代碼，唔使路綫參數 |

唔讀取查詢參數。

```bash
curl http://127.0.0.1:3000/api/stations/ADM/next-trains
```

各途經路綫並行查詢，行各自快取。一條成功就回傳 `200`；失敗路綫用 `board: null` 加錯誤保留。金鐘有 EAL、SIL、TWL 同 ISL。下面節錄 EAL 同 TWL 尾班車開出後嘅資料。

`200 OK`，`application/json`。保留嘅歷史回應節錄：

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

| 欄位 | 型別 | 意思 |
| --- | --- | --- |
| `station` | object | 車站引用 |
| `lines` | array | 按 `/api/lines` 順序嘅途經路綫 |
| `lines[].line` | object | `{ code, name }` |
| `lines[].board` | object \| null | 第 3 節欄位，但冇 `line` 同 `station`；失敗係 null |
| `lines[].error` | object \| null | `{ code, message }`；成功係 null |

失敗路綫元素示意：

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

回應用成功到站板最短剩餘新鮮期。任何成功板係舊資料就用 `no-cache`。錯誤：`404 unknown_station`；全部路綫失敗先 `502 upstream_unavailable`。

<a id="health"></a>

## 5. 存活檢查

```http
GET /api/health
```

冇路徑參數。

唔讀取查詢參數。

```bash
curl -i http://127.0.0.1:3000/api/health
```

`GET` 同 `HEAD` 回傳 `200`、空 body，冇 content type，並帶 `Cache-Control: no-store`。唔訪問上游、唔記請求日誌，亦唔產生請求 ID。呢個檢查 HTTP 進程，唔係上游可用性。冇業務錯誤；服務連唔到就係連線層失敗。

```http
HTTP/1.1 200 OK
cache-control: no-store
content-length: 0
```

<a id="open-data"></a>

## 開放數據：共同行為

第 6–13 節讀同一份記憶體快照，來自七個港鐵 CSV。啟動即刻首次輪詢，之後每 24 小時一次，失敗 5 分鐘後重試。全部成功先發佈。讀取可能要等首次輪詢；請求唔會觸發平台抓取。抓取起計新鮮期 86430 秒，再有 30 日舊資料視窗，之後 `502 upstream_unavailable`。新進程冇舊快照。

Local First 同步可以請求 `/api/data`，比較本機版本，只下載有改嘅數據集。數據集弱 ETag 包括服務版本同內容 hash，例如 `W/"0.5.2-5987f5c9680ce780"`；原始檔只有位元組 hash。下次傳 `If-None-Match`，冇變就回空 `304`，帶 ETag 同快取 header。索引本身冇 ETag。版本只用嚟比較相等，唔好當時間戳或可排序版本。

```bash
curl -i -H 'If-None-Match: W/"0.5.2-5987f5c9680ce780"' http://127.0.0.1:3000/api/data/fares
```

```http
HTTP/1.1 304 Not Modified
etag: W/"0.5.2-5987f5c9680ce780"
cache-control: public, max-age=86000
```

上面 304 header 係條件請求合約示意，剩餘新鮮期按請求時間而定。新鮮回應用 `public, max-age=N`，舊資料用 `no-cache`。每份抓取狀態嘅數據集／檔案共用原始同最高級別 gzip 編碼，帶 `Content-Length` 同 `Vary: Accept-Encoding`。首次請求負責編碼，之後重用位元組。

| 欄位 | 型別 | 意思 |
| --- | --- | --- |
| `updated_at` | string \| null | 數據集來源 `Last-Modified`；可以係幾個月前或冇提供，唔代表快照過時 |
| `fetched_at` | string | 最近成功完整輪詢 |
| `stale` | boolean | 快照已過新鮮期 |

<a id="data-index"></a>

## 6. 開放數據索引

```http
GET /api/data
```

冇路徑參數。

唔讀取查詢參數。

```bash
curl http://127.0.0.1:3000/api/data
```

`200 OK`，`application/json`。保留嘅歷史回應節錄：

```json
{
  "fetched_at": "2026-09-29T03:08:55+08:00",
  "stale": false,
  "datasets": [
    {
      "name": "stations",
      "path": "/api/data/stations",
      "revision": "0.5.2-0d1201f5804cd4cd",
      "updated_at": "2023-11-21T18:09:07+08:00"
    },
    {
      "name": "fares",
      "path": "/api/data/fares",
      "revision": "0.5.2-5987f5c9680ce780",
      "updated_at": "2026-04-03T01:02:50+08:00"
    },
    {
      "name": "airport-express-fares",
      "path": "/api/data/airport-express-fares",
      "revision": "0.5.2-545393302ce4c18f",
      "updated_at": "2025-06-22T01:05:16+08:00"
    },
    {
      "name": "light-rail",
      "path": "/api/data/light-rail",
      "revision": "0.5.2-97a676332b659216",
      "updated_at": "2026-07-05T00:58:02+08:00"
    },
    {
      "name": "light-rail-fares",
      "path": "/api/data/light-rail-fares",
      "revision": "0.5.2-e9deb534253c3bde",
      "updated_at": "2024-06-30T01:39:03+08:00"
    },
    {
      "name": "accessibility",
      "path": "/api/data/accessibility",
      "revision": "0.5.2-b199444f08da98bf",
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

| 欄位 | 型別 | 意思 |
| --- | --- | --- |
| `fetched_at`, `stale` | string, boolean | 快照資料；冇頂層 `updated_at` |
| `datasets`, `sources` | array | 固定順序嘅清洗數據集同七個原始檔 |
| `datasets[].name`, `datasets[].path` | string | 數據集名同下載路徑；名稱對應六個 URL 後綴 |
| `datasets[].revision`, `sources[].revision` | string | ETag 去除 `W/` 同引號嘅值 |
| `datasets[].updated_at`, `sources[].updated_at` | string \| null | 上游修改時間 |
| `sources[].file`, `sources[].path` | string | 原始檔名同下載路徑 |
| `sources[].bytes` | integer | 未壓縮位元組數 |

索引冇 ETag，唔會回 `304`；用佢嘅版本驗證個別下載。

快取同條件請求跟[開放數據共同行為](#open-data)。冇可用完整快照就 `502 upstream_unavailable`。

<a id="source-files"></a>

## 7. 原始檔案

```http
GET /api/data/sources/{file}
```

| 路徑參數 | 意思 |
| --- | --- |
| `file` | 必填，限下面檔名，唔分大小寫 |

唔讀取查詢參數。

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

`200 OK`，`text/csv; charset=utf-8`。解碼後 body 保留上游每個位元組，包括 BOM、CRLF 同原有錯字。下面歷史節錄顯示標題同頭四行，完整未壓縮 body 當時係 691 bytes。

```csv
ST_FROM,ST_FROM_ID,ST_TO,ST_TO_ID,OCT_ADT_FARE,OCT_CHD_FARE,SINGLE_ADT_FARE,SINGLE_CHD_FARE
HongKong,44,Airport,47,120,60,130,65
HongKong,44,AsiaWorld-Expo,56,120,60,130,65
Kowloon,45,Airport,47,105,52.5,115,57.5
Kowloon,45,AsiaWorld-Expo,56,105,52.5,115,57.5
```

欄位定義請睇港鐵平台嘅來源文件。要標準化欄位合約就用下面 JSON。原始檔 ETag 只跟位元組改，唔受服務升級影響。未知檔名回 `404 unknown_source`。

快取同條件請求跟[開放數據共同行為](#open-data)。冇可用完整快照就 `502 upstream_unavailable`。

<a id="published-stations"></a>

## 8. 公開車站同路綫

```http
GET /api/data/stations
```

冇路徑參數。

唔讀取查詢參數。

```bash
curl http://127.0.0.1:3000/api/data/stations
```

來自 `mtr_lines_and_stations.csv`，反映公開資料，唔同 `/api/lines` 嘅內置路網。保留樣本有 97 站、十條綫，冇馬場。ID 轉車站代碼，站名去多餘空白，`茘` 統一為 `荔`，空行移除，支綫用 `from`／`towards` 而唔係 `LMC-UT` 等內部名。

`200 OK`，`application/json`。保留嘅歷史回應節錄：

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

| 欄位 | 型別 | 意思 |
| --- | --- | --- |
| `updated_at`, `fetched_at`, `stale` | metadata | 共用數據集資料 |
| `stations` | array | 按車站代碼排序 |
| `stations[].code`, `stations[].name`, `stations[].lines` | string, object, array of string | 代碼、公開雙語名，途經路綫按內置綫序 |
| `lines[].code`, `lines[].name` | string, object | 路綫代碼同雙語名，冇公開路綫資料嘅綫唔列出 |
| `lines[].routes` | array | 先 `up` 後 `down`，先主綫後支綫 |
| `lines[].routes[].direction` | string | `up`／`down`；迪士尼 `up` 係迪士尼往欣澳 |
| `lines[].routes[].from`, `lines[].routes[].towards` | object | 起點同終點車站引用 |
| `lines[].routes[].stations` | array of string | 依序停站；支綫可能只列支綫段，例如調景嶺往康城 |

快取同條件請求跟[開放數據共同行為](#open-data)。冇可用完整快照就 `502 upstream_unavailable`。

<a id="fares"></a>

## 9. 重鐵車費

```http
GET /api/data/fares
```

冇路徑參數。

唔讀取查詢參數。

```bash
curl --compressed http://127.0.0.1:3000/api/data/fares
```

來自 `mtr_lines_fares.csv`，唔包括機場快綫。ID 轉車站代碼，馬場按名稱對應 `RAC`。歷史完整回應有 9120 個行程，約 1.7 MB JSON／71 KB gzip；完整下載建議用壓縮。

`200 OK`，`application/json`。保留嘅歷史回應節錄：

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

| 欄位 | 型別 | 意思 |
| --- | --- | --- |
| `updated_at`, `fetched_at`, `stale` | metadata | 共用數據集資料 |
| `fares[].from`, `fares[].to` | string | 起點同終點車站代碼 |
| `fares` | array | 按起點再終點排序；A→B 同 B→A 分開，同站行程移除 |
| `fares[].octopus.adult` | integer | 成人八達通，港仙 |
| `fares[].octopus.student` | integer | 學生乘車優惠八達通 |
| `fares[].octopus.joyyou_sixty` | integer | 樂悠咭，60–64 歲 |
| `fares[].octopus.child` | integer | 小童八達通 |
| `fares[].octopus.elderly` | integer | 長者八達通，65 歲或以上 |
| `fares[].octopus.disability` | integer | 殘疾人士優惠 |
| `fares[].single_journey.adult` | integer | 成人單程票 |
| `fares[].single_journey.child` | integer | 小童單程票 |
| `fares[].single_journey.elderly` | integer | 長者單程票 |

快取同條件請求跟[開放數據共同行為](#open-data)。冇可用完整快照就 `502 upstream_unavailable`。

<a id="airport-express-fares"></a>

## 10. 機場快綫車費

```http
GET /api/data/airport-express-fares
```

冇路徑參數。

唔讀取查詢參數。

```bash
curl http://127.0.0.1:3000/api/data/airport-express-fares
```

來自 `airport_express_fares.csv`，金額全部係港仙。上游獨立 ID 44–46 對應 HOK、KOW 同 TSY。歷史完整樣本有 14 個行程。

`200 OK`，`application/json`。保留嘅歷史回應節錄：

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

| 欄位 | 型別 | 意思 |
| --- | --- | --- |
| `updated_at`, `fetched_at`, `stale` | metadata | 共用數據集資料 |
| `fares` | array | 按起點再終點排序 |
| `fares[].from`, `fares[].to` | string | 起點同終點代碼 |
| `fares[].octopus.adult`, `fares[].octopus.child` | integer | 成人同小童八達通港仙 |
| `fares[].single_journey.adult`, `fares[].single_journey.child` | integer | 成人同小童單程票港仙 |

快取同條件請求跟[開放數據共同行為](#open-data)。冇可用完整快照就 `502 upstream_unavailable`。

<a id="light-rail"></a>

## 11. 輕鐵車站同路綫

```http
GET /api/data/light-rail
```

冇路徑參數。

唔讀取查詢參數。

```bash
curl http://127.0.0.1:3000/api/data/light-rail
```

來自 `light_rail_routes_and_stops.csv`。車站 ID 係數字，對應港鐵輕鐵到站 API；三字母代碼同重鐵係兩套編號。路綫按數字加後綴排序（`614` 先過 `614P`），折返處連續重複車站合併。保留樣本有 68 站同十一條路綫。

`200 OK`，`application/json`。保留嘅歷史回應節錄：

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

| 欄位 | 型別 | 意思 |
| --- | --- | --- |
| `updated_at`, `fetched_at`, `stale` | metadata | 共用數據集資料 |
| `stops` | array | 按車站 ID 排序 |
| `stops[].id`, `stops[].code`, `stops[].name` | integer, string, object | 數字 ID、車站代碼同雙語名 |
| `stops[].routes` | array of string | 途經路綫號，已排序 |
| `routes` | array | 已排序路綫 |
| `routes[].route` | string | 路綫號，例如 `505` 或 `614P` |
| `routes[].directions` | array | 上游方向 1 再 2 |
| `routes[].directions[].from`, `routes[].directions[].towards` | object | 起點／終點 `{ id, name }` |
| `routes[].directions[].stops` | array of integer | 有序 ID；循環綫 705／706 保留來源兩段接駁形式 |

快取同條件請求跟[開放數據共同行為](#open-data)。冇可用完整快照就 `502 upstream_unavailable`。

<a id="light-rail-fares"></a>

## 12. 輕鐵車費

```http
GET /api/data/light-rail-fares
```

冇路徑參數。

唔讀取查詢參數。

```bash
curl --compressed http://127.0.0.1:3000/api/data/light-rail-fares
```

來自 `light_rail_fares.csv`，票種同整數港仙同重鐵一樣。`from` 同 `to` 係第 11 節嘅數字車站 ID。按起點／終點排序，同站行程移除。歷史樣本有 4556 個行程，約 810 KB JSON／17 KB gzip。

`200 OK`，`application/json`。保留嘅歷史回應節錄：

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

| 欄位 | 型別 | 意思 |
| --- | --- | --- |
| `updated_at`, `fetched_at`, `stale` | metadata | 共用數據集資料 |
| `fares[].from`, `fares[].to` | integer | 起點同終點 ID |
| `fares` | array | 按起點再終點排序；A→B 同 B→A 分開，同站行程移除 |
| `fares[].octopus.adult` | integer | 成人八達通，港仙 |
| `fares[].octopus.student` | integer | 學生乘車優惠八達通 |
| `fares[].octopus.joyyou_sixty` | integer | 樂悠咭，60–64 歲 |
| `fares[].octopus.child` | integer | 小童八達通 |
| `fares[].octopus.elderly` | integer | 長者八達通，65 歲或以上 |
| `fares[].octopus.disability` | integer | 殘疾人士優惠 |
| `fares[].single_journey.adult` | integer | 成人單程票 |
| `fares[].single_journey.child` | integer | 小童單程票 |
| `fares[].single_journey.elderly` | integer | 長者單程票 |

快取同條件請求跟[開放數據共同行為](#open-data)。冇可用完整快照就 `502 upstream_unavailable`。

<a id="accessibility"></a>

## 13. 無障礙設施

```http
GET /api/data/accessibility
```

冇路徑參數。

唔讀取查詢參數。

```bash
curl --compressed http://127.0.0.1:3000/api/data/accessibility
```

合併兩個 `barrier_free_*.csv`，轉車站 ID，解碼名稱 HTML entity，只保留有提供（`Y`）嘅設施，位置去多餘空白但保留換行。冇任何設施嘅行會略過。歷史樣本有四類、36 種設施同 98 站。

`200 OK`，`application/json`。保留嘅歷史回應節錄：

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

| 欄位 | 型別 | 意思 |
| --- | --- | --- |
| `updated_at`, `fetched_at`, `stale` | metadata | 共用資料；`updated_at` 取兩個檔較後修改時間 |
| `categories` | array | 固定順序：`station_access`、`visually_impaired`、`hearing_impaired`、`mobility_impaired` |
| `categories[].category`, `categories[].name` | string, object | 類別 ID 同公開雙語名；出入口類包括同層、斜道、升降機同輪椅輔助設施 |
| `categories[].facilities` | array | 目錄順序 |
| `categories[].facilities[].code`, `categories[].facilities[].name` | string, object | 保留大小寫代碼（例如 `VIn1`）同雙語設施名 |
| `stations` | array | 按車站代碼排序 |
| `stations[].station` | object | 車站引用，名稱用內置路網 |
| `stations[].facilities` | array | 按目錄順序列已提供設施 |
| `stations[].facilities[].code` | string | 對應目錄代碼 |
| `stations[].facilities[].location` | object \| null | 雙語自由文字，或者 null；直接顯示，唔好解析結構 |

快取同條件請求跟[開放數據共同行為](#open-data)。冇可用完整快照就 `502 upstream_unavailable`。

<a id="mock-api"></a>

## Mock API：共同行為

用 `--mock-api` 或 `DUT_MOCK_API=true` 啟用，未啟用路徑回 `404 not_found`。模擬唔訪問上游，共用正式回應合約、gzip、請求 ID 同錯誤映射。正式路由忽略 Mock 查詢參數。模擬資料同錯誤適合試客戶端行為，唔係乘客即時班次。

| 查詢 | 意思 |
| --- | --- |
| `scenario` | 可選，唔分大小寫，場景名或 `random`；唔傳就按權重隨機 |
| `seed` | 可選 u64，0–18446744073709551615。指定場景預設 0；random 冇 seed 就每次產生新 seed |

固定 seed 就係同一個模擬世界，配 `random` 亦會揀同一場景。時間仍然前進，重複請求會見到列車到站同開走。重現時用回應 `x-mock-scenario` 同 `x-mock-seed`。模擬回應包括模擬 `502` 都有呢兩個 header；參數／路徑錯誤同目錄就冇。

月台同中途折返參考 2026-10-02 全網樣本，包括機場雙月台同終點交替月台。模擬有康城支綫、觀塘綫往何文田折返、東鐵支綫／折返，同東涌綫繁忙時段往青衣班次。時間係 `generated_at` 加整數分鐘，佢比 `fetched_at` 早 2–8 秒。延誤會將班距拉長 1.7 倍、加偏差，約八班取消一班。呢啲係近似值，唔跟真實日期。

| 路綫 | 繁忙分鐘 | 非繁忙分鐘 | 深夜分鐘 |
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

## 14. 場景目錄

```http
GET /api/mock/scenarios
```

冇路徑參數。

唔讀取查詢參數。

```bash
curl http://127.0.0.1:3000/api/mock/scenarios
```

`200 OK`，`application/json`。範例取自運行中 Mock 服務，陣列已節錄。

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

| 欄位 | 型別 | 意思 |
| --- | --- | --- |
| `next_trains`, `line_status` | array | 固定順序：十一個到站場景、九個狀態場景 |
| `*[].scenario` | string | 查詢名稱 |
| `*[].random_weight` | integer | 相對抽選權重，零只可指定 |
| `*[].description` | object | 開發選單用雙語 `{ en, tc }` 說明 |

Cache-Control 係 `public, max-age=86400`。啟用後冇業務錯誤，否則 `404 not_found`。

<a id="mock-line-status"></a>

## 15. 模擬全網狀態

```http
GET /api/mock/lines/status
```

冇路徑參數。

接受可選 `scenario` 同 `seed`，見 [Mock 共同行為](#mock-api)。

| scenario | 權重 | 行為 |
| --- | --- | --- |
| normal | 8 | 全綫正常 |
| delayed | 3 | 一綫延誤，附通告 |
| disrupted | 1 | 一綫受阻，部分路段暫停 |
| delayed_or_disrupted | 1 | 一綫延誤／受阻，顯示黃 |
| typhoon_signal | 1 | 全綫風球狀態，冇訊息 |
| non_service_hours | 2 | 全綫非服務時間 |
| unknown_condition | 0 | 一綫未知原值 `blue`，顯示灰 |
| stale | 1 | 正常資料，2–12 分鐘前，no-cache |
| upstream_unavailable | 1 | 502 upstream_unavailable |

```bash
curl -i 'http://127.0.0.1:3000/api/mock/lines/status?scenario=disrupted&seed=8'
```

`200 OK`，`application/json`。範例取自運行中 Mock 服務，陣列已節錄。

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

欄位同[全網狀態](#line-status)一樣。Seed 揀事故綫，包括輕鐵。`updated_at` 正常／stale 用 06:15，非服務時間用 01:20，其餘用最近一小時。新鮮度模擬 30 秒輪詢，`max-age` 最多 33，stale 用 `no-cache`。錯誤係 `400 unknown_scenario`、`400 invalid_query`、未啟用 `404 not_found`，或模擬 `502 upstream_unavailable`。

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

## 16. 模擬單綫到站板

```http
GET /api/mock/lines/{line}/stations/{station}/next-trains
```

| 路徑參數 | 意思 |
| --- | --- |
| `line` | 必填路綫代碼；輕鐵 `LR` 冇到站板 |
| `station` | 必填，要係指定路綫嘅已知車站 |

接受可選 `scenario` 同 `seed`，見 [Mock 共同行為](#mock-api)。

| scenario | 權重 | 行為 |
| --- | --- | --- |
| peak | 3 | 繁忙班次，含中途折返 |
| off_peak | 4 | 非繁忙班次 |
| late_night | 2 | 深夜疏班；康城只往返調景嶺 |
| last_train | 1 | 尾班車逐班開走，每 20 分鐘重演 |
| non_service_hours | 1 | 方向保留，列車陣列為空 |
| delayed | 2 | 繁忙事故綫有脫班、取消同扎堆 |
| special_arrangement | 1 | 非繁忙班次，附雙語通告 |
| race_day | 1 | 部分東鐵經馬場唔經火炭；其他場景馬場冇車 |
| stale | 1 | 非繁忙資料約 40–90 秒前，no-cache |
| partial_outage | 1 | 一條事故綫不可用 |
| upstream_unavailable | 1 | 全部到站板不可用 |

```bash
curl -i 'http://127.0.0.1:3000/api/mock/lines/EAL/stations/SHT/next-trains?scenario=peak&seed=2'
```

`200 OK`，`application/json`。範例取自運行中 Mock 服務，陣列已節錄。

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

欄位同[單綫板](#next-trains)一樣，事故影響指定綫。東鐵喺 ADM、LOW 同 LMC 用 departure，其餘 arrival。新鮮度模擬十秒更新，`max-age` 2–10；stale 用 `no-cache`。路徑先過查詢參數驗證。正式錯誤之外，加 `400 unknown_scenario` 同 `400 invalid_query`。`partial_outage` 同 `upstream_unavailable` 回 `502`，帶 Mock header。

模擬雙語通告內容：

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

## 17. 模擬全站到站板

```http
GET /api/mock/stations/{station}/next-trains
```

| 路徑參數 | 意思 |
| --- | --- |
| `station` | 必填已知車站代碼 |

接受可選 `scenario` 同 `seed`，見 [Mock 共同行為](#mock-api)。

```bash
curl -i 'http://127.0.0.1:3000/api/mock/stations/ADM/next-trains?scenario=partial_outage'
```

`200 OK`，`application/json`。範例取自運行中 Mock 服務，陣列已節錄。

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

欄位、部分成功同合併新鮮度都跟[全站板](#station-next-trains)。場景同第 16 節一樣。Seed 揀一條途經綫做延誤、特別安排或部分故障。部分故障有其他綫成功就 `200`，單綫站就 `502`。車站路徑先過查詢參數驗證。錯誤：`400 unknown_scenario`、`400 invalid_query`、`404 unknown_station`、未啟用 `404 not_found`，全部失敗就 `502 upstream_unavailable`。

<a id="mock-events"></a>

## 18. 模擬事件流

```http
GET /api/mock/events
```

冇路徑參數。

| 查詢 | 意思 |
| --- | --- |
| `scenario`、`seed` | 見 [Mock 共同行為](#mock-api)。場景即第 14 節嘅 `line_status` 場景 |
| `interval` | 可選，兩次變更之間嘅整數秒數，1–60；預設 5 |

```bash
curl -N 'http://127.0.0.1:3000/api/mock/events?scenario=delayed&seed=7&interval=2'
```

`200 OK`，`text/event-stream`。連線會保持打開，直至客戶端斷線；請用 `curl -N` 或 `EventSource`。每個事件有 `event:` 名稱，`data:` 係一行 JSON。樣本取自運行中嘅 Mock 服務；兩個 `line_status` 事件係同一宗事故同佢嘅恢復，相隔兩秒。

```
event: hello
data: {"server_time":"2026-10-08T01:25:49+08:00"}

event: line_status
data: {"observed_at":"2026-10-08T01:25:51+08:00","line":{"code":"DRL","name":{"en":"Disneyland Resort Line","tc":"迪士尼綫"}},"color":"#F550A6","previous":{"condition":"normal","display":"green","message":null},"current":{"condition":"delayed","display":"yellow","message":"Due to a signalling fault at Disneyland Resort Station, Disneyland Resort Line train service is delayed. Passengers please allow extra travelling time."}}

event: line_status
data: {"observed_at":"2026-10-08T01:25:53+08:00","line":{"code":"DRL","name":{"en":"Disneyland Resort Line","tc":"迪士尼綫"}},"color":"#F550A6","previous":{"condition":"delayed","display":"yellow","message":"Due to a signalling fault at Disneyland Resort Station, Disneyland Resort Line train service is delayed. Passengers please allow extra travelling time."},"current":{"condition":"normal","display":"green","message":null}}
```

連線後即刻發送 `hello`，之後每隔 `interval` 發送一批變更，並循環：報告場景入面嘅事故、服務恢復，再重新開始。Seed 揀受影響嘅綫同措辭，做法同狀態路由一樣，所以每個事件就係第 15 節兩次讀數之間嘅差異。全網場景（`typhoon_signal`、`non_service_hours`）每條綫發一個事件，同一批。`normal`、`stale` 同 `upstream_unavailable` 冇變化，所以流只發 `hello`，之後除咗保活註解外保持安靜。

| 事件 | Data 欄位 | 類型 | 意思 |
| --- | --- | --- | --- |
| `hello` | `server_time` | string | RFC 3339 時間，連線打開時發一次 |
| `line_status` | `observed_at` | string | 發現變更嘅時間 |
| | `line` | object | 綫嘅 `{ code, name }` |
| | `color` | string | 綫嘅顏色，同[綫路](#lines)一樣 |
| | `previous`、`current` | object | 變更前後綫嘅 `condition`、`display` 同 `message`，意思同[第 2 節](#line-status) |

事件只係提示，唔係日誌：唔會重播，冇 `id`，重連嘅客戶端會錯過期間發生嘅事。連線或重連後，請由狀態路由攞返當前狀態。正式服務暫時未有事件流；呢條路由先定好 App 可以跟嘅形狀。

Header：`Cache-Control: no-cache`；`X-Accel-Buffering: no`，等 nginx 唔會扣住事件；仲有模擬嘅 `x-mock-scenario` 同 `x-mock-seed`。回應唔會經 gzip 壓縮。每 15 秒發一行註解（`:`），等閒置連線保持打開。錯誤：`interval` 格式錯或超出範圍係 `400 invalid_query`，另有 `400 unknown_scenario`，未啟用就 `404 not_found`。模擬嘅故障唔會用 `502` 結束流。

<a id="errors"></a>

## 錯誤碼

| HTTP | code | 原因 |
| --- | --- | --- |
| 400 | `invalid_query` | Mock 參數格式錯或重複，包括無效 u64 seed 或超出範圍嘅 `interval` |
| 400 | `unknown_scenario` | 呢個接口唔識嘅場景 |
| 404 | `not_found` | 未知路由或 Mock 未啟用 |
| 404 | `unknown_line` | 未知路綫代碼 |
| 404 | `unknown_station` | 格式錯或未知車站代碼 |
| 404 | `station_not_on_line` | 路綫唔經本站或唔支援 Next Train |
| 404 | `unknown_source` | 未知來源檔名 |
| 502 | `upstream_unavailable` | 冇可用新鮮或舊資料 |

<a id="line-codes"></a>

## 附錄 A：路綫代碼

| 代碼 | 繁體中文名 | English | 顏色 | 列車到站 |
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
| `LR` | 輕鐵 | Light Rail | `#9F7A00` | 冇；只限狀態 |

<a id="station-codes"></a>

## 附錄 B：車站代碼

車站按內置路綫順序分組，共用代碼喺各綫都係同一換乘站。支綫項目唔代表單一路徑；導航用 `towards` 或公開路綫段。`/api/lines` 有呢啲代碼嘅雙語名。

| 路綫 | 車站代碼同官方名稱 |
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

## 變更記錄

| 日期 | 改動 |
| --- | --- |
| 2026-10-08 | 新增要啟用嘅 Mock 路由 `GET /api/mock/events`：用 Server-Sent Events 推送模擬嘅 `line_status` 變更，支援 `scenario`、`seed` 同 `interval`。正式合約不變。 |
| 2026-10-07 | 文件重寫為英文，新增粵語同國語版本。釐清 no-cache、重啟行為、健康事件同推送規劃。HTTP 合約冇改，保留原有樣本。 |
| 2026-10-02 | 不相容：正式同 Mock 列車 `platform` 整數改成 `platforms` 整數陣列。機場支援 `[1, 3]`／`[2, 4]`，未知值用 `[]` 並保留列車，修正以前 `1/3` 令機場板失敗。 |
| 2026-10-02 | 新增四個要啟用嘅 Mock 路由，支援場景／seed、實際值 header 同 `invalid_query`／`unknown_scenario`。正式合約不變。 |
| 2026-10-01 | 數據集／檔案每次抓取共用編碼，最高 gzip 將車費下載約 75 KB 減至 71 KB。加 Content-Length 同 Vary；解壓內容、ETag 同快取不變。 |
| 2026-09-29 | 新增八個開放數據路由、每日輪詢、ETag／304、整數港仙、gzip 同 `unknown_source`。 |
| 2026-09-29 | 新增健康檢查，空 200、no-store，唔追蹤請求。 |
| 2026-09-28 | 路綫狀態改為 30 秒輪詢，新鮮期最多 33 秒；欄位不變。 |
| 2026-09-28 | 錯誤表移除未實作嘅 500 internal_error。 |
| 2026-09-28 | 不相容：up／down 陣列改為 directions，含 direction／towards／trains；省略終點方向，移除 sequence。路綫 destinations 改為 directions，towards 只列主要終點。 |
| 2026-09-28 | 首版路綫、狀態同兩個 Next Train 路由；移除 /api/hello 同 /api/hello.json。 |
