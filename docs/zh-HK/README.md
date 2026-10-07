# Dut (嘟)

[English](../../README.md) · [繁體粵語](README.md) · [简体中文](../zh-CN/README.md)

Dut 係 MTRGo (Not Yet Released or Open-Sourced) 嘅 Rust 後端，將港鐵開放數據整理成 App 可以直接用嘅 JSON：路綫同車站、服務狀態、下一班車、車費、輕鐵路綫，以及無障礙設施。

## 目錄

- [本機啟動](#quick-start)
- [HTTP 接口](#endpoints)
- [用模擬資料開發](#mock-api)
- [快取同資料新鮮度](#freshness)
- [設定](#configuration)
- [容器](#docker)
- [發佈](#releases)
- [日誌同運維](#logging)
- [開發](#development)
- [資料來源](#sources)

- **到月台就用得。** 行車方向附有指示牌上嘅終點，名稱同時有英文同繁體中文。到站時間用絕對時間，App 自己更新倒數。
- **共用上游請求。** 記憶體快取會合併同一塊到站板嘅請求。上游故障時，有可用舊資料就回傳，並標記 `stale: true`。
- **適合 Local First。** 車費同設施下載一次之後，可以比較版本或者用條件請求同步；原始 CSV 亦有提供。
- **方便追查。** 回應附有 `x-request-id`，方便將用戶回報同伺服器日誌對返。

<a id="quick-start"></a>

## 本機啟動

Edition 2024 至少要 Rust 1.85；目前依賴套件可能需要更新嘅工具鏈。啟動服務之後，就可以查將軍澳站：

```bash
cargo run
```

```bash
curl http://127.0.0.1:3000/api/lines/TKL/stations/TKO/next-trains
```

預設監聽 `127.0.0.1:3000`。以下保留咗 2026-09-28 嘅深夜實際回應節錄；當時往北角嘅尾班車已經開咗。

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

## HTTP 接口

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

參數、回應欄位、快取、錯誤同範例都喺 [HTTP_API.md](HTTP_API.md)。

<a id="mock-api"></a>

## 用模擬資料開發

想即刻試延誤、風球、尾班車或者上游故障，就啟用 Mock API。佢同正式接口共用回應 DTO，客戶端只要將 `/api` 換成 `/api/mock`。

```bash
cargo run -- --mock-api
```

```bash
curl 'http://127.0.0.1:3000/api/mock/stations/ADM/next-trains?scenario=delayed&seed=7'
```

`scenario` 揀情境；唔傳就按權重隨機揀。`seed` 揀一個模擬世界，固定佢輪詢，就會見到列車隨時間到站同開走。回應用 `x-mock-scenario` 同 `x-mock-seed` 列明實際值。[場景目錄](HTTP_API.md#mock-scenarios) 有全部選項。Mock 路由預設關閉，未啟用就回傳 `404`。

<a id="freshness"></a>

## 快取同資料新鮮度

| 資料 | 新鮮期 | 故障處理 |
| --- | --- | --- |
| 列車到站 | 通常 10 秒，限制喺 2–15 秒 | 過期後可頂替 90 秒；失敗後等 5 秒再試 |
| 路綫狀態 | 每 30 秒輪詢；新鮮期 33 秒 | 新鮮期之後可頂替 15 分鐘 |
| 內置路網 | 隨部署更新；max-age=86400 | 唔使訪問上游 |
| 開放數據 | 啟動時及每 24 小時拉取；新鮮期 86430 秒 | 5 分鐘後重試；過期後可頂替最多 30 日 |

可以用 `Cache-Control: public, max-age=N` 嘅剩餘秒數安排輪詢。舊資料回應用 `no-cache`，重用之前要重新驗證。快取只喺單一進程入面：重啟會失去，多個實例會增加上游請求量。詳情見[快取設計](ARCHITECTURE.md#caching-and-freshness)。

<a id="configuration"></a>

## 設定

| 參數 | 環境變數 | 預設 | 用途 |
| --- | --- | --- | --- |
| `--bind-address <ADDRESS>` | `DUT_BIND_ADDRESS` | `127.0.0.1:3000` | IP 同連接埠；唔接受主機名 |
| `--log-level <LEVEL>` | `RUST_LOG` | `info,dut=debug,tower_http=debug` | 日誌過濾規則 |
| `--log-file <PATH>` | `DUT_LOG_FILE` | 無 | 另加一份純文字日誌，追加寫入 |
| `--mock-api` | `DUT_MOCK_API` | 關閉 | 啟用模擬路由 |

命令列參數優先過環境變數。`DUT_MOCK_API` 接受 `true/false`、`1/0`、`yes/no` 同 `on/off`。`dut --help` 列出選項，`dut --version` 顯示版本。用 Cargo 時，參數放喺 `--` 後面。空值或者格式錯誤會令啟動失敗，退出碼係 2。

```bash
cargo run -- --bind-address 0.0.0.0:3000 --log-level info
```

上游地址、逾時同快取策略都喺 [config.rs](../../src/bootstrap/config.rs) 嘅預設值。收到 `SIGTERM` 或 Ctrl-C 後，服務停止接新請求，等處理中嘅請求完成。出站 HTTP 會用環境變數或 macOS 系統代理；macOS 嘅略過清單唔會套用，要直連嘅主機請設 `NO_PROXY`。測試會直接連本機模擬上游。

<a id="docker"></a>

## 容器

Dockerfile 用 Alpine 同 `xx` 交叉編譯靜態 musl 程式，再放入有 CA 憑證同非 root 用戶嘅 distroless static 映像。映像冇 shell 或 curl，並設咗 `DUT_BIND_ADDRESS=0.0.0.0:3000`。

```bash
docker build -t dut .
docker run --rm -p 3000:3000 dut
```

```bash
docker buildx build --platform linux/amd64,linux/arm64 -t dut .
```

健康檢查交畀編排系統或負載平衡器，請求 `GET /api/health`；映像冇 `HEALTHCHECK`。

```bash
docker run --rm -p 3000:3000 ghcr.io/jimmyrice/dut:latest
```

<a id="releases"></a>

## 發佈

修改 `Cargo.toml` 嘅 `[workspace.package].version`，提交之後推送對應嘅 `v<version>` tag。版本唔一致會喺編譯之前失敗。以下版本只係示範：

```bash
git tag v0.6.0
git push origin v0.6.0
```

[release.yml](../../.github/workflows/release.yml) 發佈六個平台程式同 `SHA256SUMS`；tag 有 `-` 就係預發佈。[docker.yml](../../.github/workflows/docker.yml) 推送版本號同次版本 tag，1.0 起再加主版本 tag；預發佈唔更新 `latest`。`master` 上相關程式或 Dockerfile 改動會更新 `edge`。手動跑 Release 工作流程只會保存構建產物，唔會建立 release。

| 平台 | 架構 | 壓縮檔 |
| --- | --- | --- |
| Linux | x86-64 | `dut-x86_64-unknown-linux-musl.tar.gz` |
| Linux | arm64 | `dut-aarch64-unknown-linux-musl.tar.gz` |
| macOS | Apple Silicon | `dut-aarch64-apple-darwin.tar.gz` |
| macOS | Intel | `dut-x86_64-apple-darwin.tar.gz` |
| Windows | x86-64 | `dut-x86_64-pc-windows-msvc.zip` |
| Windows | arm64 | `dut-aarch64-pc-windows-msvc.zip` |

Linux 版本係靜態連結，唔需要 glibc；自行用精簡容器時仍要裝 `ca-certificates`，憑證放喺其他位置可以用 `SSL_CERT_FILE`。Windows 版本已包含 C runtime。發佈構建用 fat LTO 並移除符號；panic 會終止進程，部署時要設自動重啟。本機用 `cargo build --profile dist` 就得到相同 profile 嘅 `target/dist/dut`，編譯會耐過 release。

倉庫已經公開，之後發佈嘅程式壓縮檔同容器映像，工作流程會附上 GitHub 簽署嘅構建來源證明。可以咁驗證壓縮檔：

```bash
gh attestation verify dut-x86_64-unknown-linux-musl.tar.gz -R JimmyRice/Dut
```

<a id="logging"></a>

## 日誌同運維

預設會輸出 Dut debug 日誌。想少啲輸出，可以設 `--log-level info`；亦可以用 `dut=debug` 呢類 target 規則，按前綴匹配 `dut_api` 同 `dut_upstream` 等。錯誤規則同空值會令啟動失敗。第一條 `logging started` 日誌會記低生效規則同檔案路徑。

終端會將每個請求嘅日誌排成一個彩色區塊；`NO_COLOR=1` 可以關顏色。管道同日誌檔係每條一行純文字。`--log-file` 會額外追加一份，父目錄要已經存在。Dut 唔會輪轉檔案；用 logrotate 時設 `copytruncate`，或者輪轉之後重啟。容器通常收集 stdout 就夠。

啟動探測會記錄各上游可唔可以連到，唔會阻塞監聽。背景輪詢會記低來源、清洗條數、路網差異同健康狀態轉變（`failing` 或 `blind`）。監控事件包括收車呢類日常變化。首次成功資料只做基線；啟動時嘅來源健康轉變仍可能發出事件。

<a id="development"></a>

## 開發

```bash
cargo fmt -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo deny check advisories bans sources
```

完成改動之前，喺 workspace 根目錄跑晒以上檢查。`cargo deny` 要先 `cargo install cargo-deny --locked`，檢查依賴嘅安全公告、被撤回嘅版本同未知來源，唔檢查授權；CI 喺每次 push 同 pull request 都會跑。局部開發可以加 `-p <crate>`。測試係可重現同離線嘅：時間行為用暫停嘅 Tokio 時鐘，上游用 wiremock 同實際抓取樣本。路由測試會組裝完整 App；macOS 會提高檔案描述符上限，支援並行測試伺服器。

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

### 更新內置路網

每日輪詢如果報路網差異，先啟動服務，再跑呢個只用 Python 標準庫嘅腳本：

```bash
python3 scripts/sync-network.py
```

佢比較 `/api/data/stations` 同 `/api/lines`。加 `--write` 會重建 `station.rs` 嘅 `STATIONS` 表，保留馬場等只喺內置資料有嘅站。路綫順序、支綫同終點要自己審核；腳本只印 `codes![...]` 建議，唔會改 `line.rs`。之後格式化、測試同睇 diff。冇差異退出碼係 0，有差異係 1。

客戶端合約請睇 [HTTP_API.md](HTTP_API.md)；業務程式用嘅型別請睇 [RUST_API.md](RUST_API.md)；設計取捨喺 [ARCHITECTURE.md](ARCHITECTURE.md)，開發規範喺 [AGENTS.md](../../AGENTS.md)。

<a id="sources"></a>

## 資料來源

- [港鐵 Next Train API](https://rt.data.gov.hk/v1/transport/mtr/getSchedule.php)
- [港鐵路綫狀態](https://tnews.mtr.com.hk/alert/ryg_line_status.json)
- [港鐵開放數據平台](https://opendata.mtr.com.hk/)
- [香港天文台警告資料](https://data.weather.gov.hk/weatherAPI/opendata/weather.php?dataType=warningInfo&lang=en)

天氣警告每分鐘輪詢一次，供監控事件用；目前冇公開天氣接口。
