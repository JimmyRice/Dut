# Rust API

[English](../../RUST_API.md) · [繁體粵語](RUST_API.md) · [简体中文](../zh-CN/RUST_API.md)

寫 Dut 業務程式時，可以用呢份參考查應該傳邊啲 domain 型別、呼叫邊個服務，同實作邊個 trait。正式環境嘅具體依賴喺 `src/bootstrap/app.rs` 組裝。業務會用到嘅 trait、handle、事件或入口有改動，就要同步呢份文件同翻譯。

## 目錄

- [由邊度入手](#where-to-start)
- [應用層 Port](#ports)
- [讀資料](#reading-data)
- [監控同事件](#monitor)
- [業務型別](#domain-vocabulary)
- [出站 HTTP](#outbound-http)
- [上游適配器](#upstream-adapters)
- [模擬資料](#simulated-data)
- [錯誤、日誌同測試](#conventions)

客戶端合約請睇 [HTTP_API.md](HTTP_API.md)；業務程式用嘅型別請睇 [RUST_API.md](RUST_API.md)；設計取捨喺 [ARCHITECTURE.md](ARCHITECTURE.md)，開發規範喺 [AGENTS.md](../../AGENTS.md)。

Markdown 程式片段係參考，唔係完整可編譯程式。公開型別嘅 Rustdoc 範例由 `cargo test` 編譯，用 `cargo doc --open` 可以睇。簽名要同原始碼一致。新增來源 ID 嘅適配器草稿，要先加 domain 定義先編譯得。

<a id="where-to-start"></a>

## 由邊度入手

| 工作 | 入口 |
| --- | --- |
| 回應變化 | [`Subscriber`](#subscriber) + `MonitorHandle::attach` |
| 讀輪詢值 | [`FeedHandle::snapshot`](#feedhandle) |
| 讀到站板 | [`NextTrainService`](#nexttrainservice) |
| 讀參考數據集／檔案 | [`ReferenceDataService`](#referencedataservice) |
| 加背景文件來源 | [`Feed`](#feed) + `dut_poll::spawn` |
| HTTP 抓取 | [`OutboundHttpClient::fetch`](#outbound-http) |
| 建立鐵路業務模型 | [`dut_core::domain`](#domain-vocabulary) |
| 加啟動選項 | `CommandLine`; [configuration](ARCHITECTURE.md#configuration) |
| 模擬事故 | [`dut-mock`](#simulated-data) |

<a id="ports"></a>

## 應用層 Port

| Trait | 用途 | 實作 |
| --- | --- | --- |
| `Subscriber` | 處理監控事件 | `EventLog` |
| `Feed` | 抓整份文件，唔自行快取 | `MtrLineStatusFeed`, `HkoWarningFeed`, `MtrOpenDataFeed`, `NextTrainSignalFeed` |
| `NextTrainSource` | 提供到站板快照 | `MtrNextTrainSource`; private mock source |
| `LineStatusSource` | 提供全網狀態快照 | `FeedHandle<NetworkStatus>` |
| `ReferenceDataSource` | 提供完整參考資料快照 | `FeedHandle<ReferenceData>` |

公開 async port 用 `fn … -> impl Future<Output = …> + Send` 宣告。實作可以寫 `async fn`，編譯器會驗證 future 係 Send。優先用泛型／靜態分派。有具體實作同測試替身嘅真正邊界先加 trait，唔好為可能出現嘅用戶預先加。

<a id="subscriber"></a>

### Subscriber

`dut_core::application::subscriber::Subscriber`

```rust
pub trait Subscriber: Send + 'static {
    const NAME: &'static str;
    fn on_event(&mut self, event: &MonitorEvent) -> impl Future<Output = ()> + Send;
    fn on_lagged(&mut self, missed: u64) -> impl Future<Output = ()> + Send;
}
```

每個訂閱者喺自己 Tokio task，按發佈順序逐個處理事件。狀態放喺 `&mut self`，唔可以跨 await 持有 std 鎖。`NAME` 係日誌 span 名。事件唔重播，所以 bootstrap 時就掛上。Broadcast 容量 256；`on_lagged` 預設唔做嘢，漏事件有影響就覆寫佢重新同步。

範例：識別正常服務轉成收車。判斷保持純函式，狀態改動放喺 `on_event`。

```rust
use dut_core::{
    application::subscriber::Subscriber,
    domain::{
        event::{Change, MonitorEvent},
        line_status::LineCondition,
        network::Line,
    },
};

/// Remembers which lines have ended service tonight.
#[derive(Debug, Default)]
pub struct ServiceEnds {
    ended: Vec<Line>,
}

impl Subscriber for ServiceEnds {
    const NAME: &'static str = "service_ends";

    async fn on_event(&mut self, event: &MonitorEvent) {
        if let Some(line) = service_ended(event) {
            self.ended.push(line);
        }
    }
}

/// The line that has just ended service, if this event says so. Pure, so it
/// is unit-tested without a task or a channel.
fn service_ended(event: &MonitorEvent) -> Option<Line> {
    match &event.change {
        Change::LineStatus(change)
            if change.previous.condition == LineCondition::Normal
                && change.current.condition == LineCondition::NonServiceHours =>
        {
            Some(change.current.line)
        }
        _ => None,
    }
}
```

啟動 monitor 之後，即刻喺 bootstrap 掛上：

```rust
let monitor = dut_monitor::spawn(&line_status, &weather_warnings, &next_train_signals);
monitor.attach(ServiceEnds::default());
```

局部測試可以直接建立事件，唔使跑 monitor。以下片段除咗上面匯入，仲要匯入 `LineStatus`、`LineStatusChange` 同 `jiff::Timestamp`：

```rust
let event = MonitorEvent {
    observed_at: Timestamp::UNIX_EPOCH,
    change: Change::LineStatus(LineStatusChange {
        previous: LineStatus { line: Line::KwunTong, condition: LineCondition::Normal, message: None },
        current: LineStatus { line: Line::KwunTong, condition: LineCondition::NonServiceHours, message: None },
    }),
};
assert_eq!(service_ended(&event), Some(Line::KwunTong));
```

<a id="feed"></a>

### Feed

`dut_core::application::feed::Feed`

```rust
pub trait Feed: Send + Sync + 'static {
    type Item: Send + Sync + 'static;
    const SOURCE: SourceId;
    fn fetch(&self) -> impl Future<Output = Result<Self::Item, SourceUnavailable>> + Send;
}
```

每次呼叫都重新抓整份文件；時間安排同最新值由 poller 管。喺 `dut-upstream` 解 transport DTO，明確轉成 domain 型別，適配器錯誤用 `SourceUnavailable::new(error)` 包裝。`SOURCE` 標識日誌同健康事件。新來源要先加 `SourceId` 變體，適配器先編譯得。

適配器草稿（`Example*` 型別同變體係佔位）：

```rust
#[derive(Clone, Debug)]
pub struct ExampleFeed {
    http: OutboundHttpClient,
    endpoint: Url,
    request_timeout: Duration,
}

impl Feed for ExampleFeed {
    type Item = ExampleDocument; // a domain type in dut-core
    const SOURCE: SourceId = SourceId::Example;

    async fn fetch(&self) -> Result<ExampleDocument, SourceUnavailable> {
        self.fetch_document().await.map_err(SourceUnavailable::new)
    }
}
```

```rust
let example = dut_poll::spawn(example_feed, polling.example);
```

想監控新文件，就加 `changes_since` 同單元測試、加 `Change` 變體、擴展 `dut_monitor::spawn` 同 watcher 組裝，再加事件日誌分支。只輪詢唔會自動發佈業務變化。

<a id="source-ports"></a>

### 來源簽名

```rust
pub trait NextTrainSource: Send + Sync + 'static {
    fn board(&self, line: Line, station: StationCode)
        -> impl Future<Output = Result<Snapshot<NextTrainBoard>, SourceUnavailable>> + Send;
}

pub trait LineStatusSource: Send + Sync + 'static {
    fn status(&self)
        -> impl Future<Output = Result<Snapshot<NetworkStatus>, SourceUnavailable>> + Send;
}

pub trait ReferenceDataSource: Send + Sync + 'static {
    fn reference_data(&self)
        -> impl Future<Output = Result<Snapshot<ReferenceData>, SourceUnavailable>> + Send;
}
```

<a id="reading-data"></a>

## 讀資料

<a id="snapshot"></a>

### 快照同新鮮度

`dut_core::application::source`

| 項目 | 行為 |
| --- | --- |
| `Snapshot<T>` | Arc 共用值：`value()`、`fetched_at()`、`freshness()` |
| `Freshness::Fresh { expires_in }` | 剩餘時段可重用 |
| `Freshness::Stale` | 過期頂替；`is_stale()` 檢查，`combine` 取最舊／最短剩餘期 |
| `SourceUnavailable` | 冇可用值；原因只畀日誌，唔畀客戶端 |

<a id="feedhandle"></a>

### FeedHandle 同 Schedule

`dut_poll::spawn(feed, schedule) -> FeedHandle<F::Item>` 喺 Tokio runtime 啟動 feed，handle 複製成本低。

| 方法／欄位 | 合約 |
| --- | --- |
| `snapshot().await -> Result<Snapshot<T>, SourceUnavailable>` | 目前快照，首次輪詢未完成時有期限等候 |
| `subscribe() -> watch::Receiver<FeedState<T>>` | 每次嘗試之後通知，只保留最新狀態 |
| `source() -> SourceId` | 穩定來源 ID |
| `FeedState<T>` | `latest() -> Option<&Polled<T>>`、`health()`、`attempts()` |
| `Polled<T>` | `value() -> &Arc<T>`、`fetched_at()` |
| `Schedule` | 全部係 Duration：`interval`、`first_poll_after`、`retry_after`、`fresh_for`、`stale_if_error`、`blind_after` |

Watch 值只借用一陣，持有佢會阻住 poller 發佈。`FeedHandle<NetworkStatus>` 實作 `LineStatusSource`，`FeedHandle<ReferenceData>` 實作 `ReferenceDataSource`。`retry_after` 短過 interval 就失敗後提早重試，大過或等於就維持正常排程。預設見[輪詢 Feed](ARCHITECTURE.md#polled-feeds)。

<a id="nexttrainservice"></a>

### NextTrainService

`dut_core::application::next_train::NextTrainService<S>`

| 呼叫 | 結果 |
| --- | --- |
| `new(source: S) -> Self` | 用 Arc 共用來源 |
| `board(line, station).await` | `Result<BoardView, NextTrainError>` |
| `station_boards(station).await` | `Result<StationBoards, NextTrainError>` |

讀來源之前先用內置路網驗證，限制快取 key。全站板並行抓取，保留各綫錯誤；冇一綫成功先整體失敗。`BoardView` 有 `board()`、`snapshot()`、`upcoming(direction)` 同 `directions()`，濾走超過 30 秒嘅已開出列車。`StationBoards` 有 `station()`、`lines()` 同 `freshness()`；各 `LineBoard` 有 `line` 同 `board: Result<BoardView, SourceUnavailable>`。錯誤係 `UnknownStation(StationCode)`、`StationNotOnLine { line, station }` 或 `Unavailable(SourceUnavailable)`。

<a id="referencedataservice"></a>

### ReferenceDataService 同 LineStatusService

`dut_core::application::reference_data::ReferenceDataService<S>::reference_data().await` 回傳 `Result<Snapshot<ReferenceData>, SourceUnavailable>`，全部數據集同來源檔出自同一次成功輪詢。`dut_core::application::line_status::LineStatusService<S>::status().await` 回傳 `Result<Snapshot<NetworkStatus>, SourceUnavailable>`。兩者用 `new(source)` 建立，複製成本低。

```rust
let snapshot = service.reference_data().await?;
let data = snapshot.value();
let fare = data.fares.value().get(from, to);
let csv = data.files.get(SourceFile::LinesFares);
```

<a id="monitor"></a>

## 監控同事件

| 入口 | 合約 |
| --- | --- |
| `dut_monitor::spawn(&line_status, &weather_warnings, &next_train_signals) -> MonitorHandle` | 每 feed 一個 watcher，先掛事件日誌 |
| `MonitorHandle::attach<S: Subscriber>(&self, subscriber: S)` | 喺獨立 task 順序投遞 |
| `MonitorHandle::subscribe() -> broadcast::Receiver<MonitorEvent>` | 直接接收循環要處理 Lagged 同 Closed |
| `NextTrainSignalFeed::new(next_trains)` | 每條支援路綫抽樣一個中段站 |

`dut_core::domain::event::MonitorEvent { observed_at: Timestamp, change: Change }` 記錄事實。首次成功資料輪詢唔發資料變化，健康轉變可以發。失敗輪詢保留資料，只可能改來源健康。同一 feed 觀察順序會保留。部署應只有一個角色做事件副作用，角色選擇仲未係 CLI 功能。

| Change | 內容／觸發 |
| --- | --- |
| `LineStatus(LineStatusChange)` | 前／後 LineStatus，狀態或訊息變化，包括收車 |
| `WeatherWarning(WarningChange)` | Issued、Changed { previous, current }、Cancelled，警告級別／更新時間變化 |
| `NextTrainSignal(SignalChange)` | 路綫／車站，前／後 signal，延誤旗標或通告變化 |
| `SourceHealth(HealthChange)` | 來源、前／後 HealthState，包括 Starting→Healthy |

<a id="domain-vocabulary"></a>

## 業務型別

下面模組都喺 `dut_core::domain`。輸入喺邊界解析一次，業務程式就傳有型別嘅值。

<a id="network"></a>

### network

| 型別 | 用途 |
| --- | --- |
| `Line` | 唔分大小寫解析；`code()`、`name()`、`color()`、`stations()`、`termini()`、`towards(station, direction)`、`leads(from, to, direction)`、`serves(station)`、`Line::serving(station)`、`Line::with_next_train()`、`Line::ALL`。`leads` 要同一支綫而終點喺起點之後。 |
| `StationCode` | Copy newtype，三個大寫字母。執行時用 `"tko".parse::<StationCode>()`；常量用 `StationCode::from_static("TKO")`，錯誤常量會令 const 求值失敗。 |
| `Station` | `find(code)` 驗證路網成員並提供名稱，`all()` 按代碼排序 |
| `Direction`, `ByDirection<T>` | 港鐵 Up／Down；`ByDirection::get(direction)` |

代碼格式啱唔代表已知車站，要用 `Station::find` 或 `Line::serves` 檢查。`scripts/sync-network.py --write` 重建 GENERATED 標記之間嘅 `STATIONS`；`line.rs` 嘅佈局、支綫同終點要人工審核。

<a id="reference"></a>

### reference

| 型別 | 合約 |
| --- | --- |
| `ReferenceData` | 一次成功輪詢：`files: BySourceFile<PublishedFile>`，以及 stations、fares、airport_express_fares、light_rail、light_rail_fares、accessibility 各個 `Dataset` |
| `Dataset<T>` | `new(value, updated_at)` hash 內容；`value()`、`revision()`、`updated_at()` |
| `Revision` | 穩定 FNV-1a：`of(&value)`、`of_bytes(bytes)`，Display 係 16 位十六進位 |
| `SourceFile`, `BySourceFile<T>` | 七個有限來源檔：`file_name()`、FromStr、ALL；容器 `get(file)`、`from_fn`、`try_from_fn` |
| `PublishedFile` | 原始 `body() -> &Arc<[u8]>`、`updated_at()`、`revision()` |
| `Fare` | 整數港仙；`"4.90".parse::<Fare>()` 係 490，`cents()`、`from_cents()` |
| `FareTable<K, F>`, `Trip<K, F>` | `get(from, to)` 查起終點，`trips()` 已排序；同站行程移除 |
| `RailFares` | `octopus: OctopusFares`、`single_journey: SingleJourneyFares`，重鐵同輕鐵共用 |
| `AirportExpressFares` | 八達通／單程票各用 `AdultAndChildFares` |
| `PublishedNetwork` | 公開車站同 Route（line、direction、stations）；`station(code)`、`drift() -> Vec<NetworkDrift>` |
| `LightRailNetwork` | Stop（StopId、StopCode、name）同 LightRailRoute（RouteNumber、按方向停站）；`stop(id)`、`routes_serving(id)` |
| `Accessibility` | FacilityGroup（類別、名、設施）同 StationAccessibility（已提供 StationFacility，可選位置） |

`StopCode` 同 `StationCode` 特登分開，輕鐵同重鐵係獨立代碼系統。

<a id="other-domain-types"></a>

### 其他業務型別

| 模組 | 型別／行為 |
| --- | --- |
| `localized` | `Localized<T> { en, tc }` |
| `time` | `HONG_KONG` 固定 UTC+8；jiff Timestamp，`display_with_offset(HONG_KONG)` |
| `line_status` | LineCondition：Normal、Delayed、Disrupted、DelayedOrDisrupted、NonServiceHours、TyphoonSignal、Unknown(String)；`display_color()`；LineStatus；`NetworkStatus::changes_since` |
| `next_train` | NextTrainBoard `signal()`；TrainArrival 絕對時間；Platforms 係 Copy，有 `one(n)`、`pair(1, 3)`、NONE、`"1/3".parse()`、`as_slice()`；AlertNotice、NextTrainSignal、`NextTrainSignals::changes_since` |
| `weather` | WeatherWarning 包括 TropicalCyclone(CycloneSignal)、PreNo8Announcement、Rainstorm(RainstormLevel)、Unrecognised(String) 同其他天文台警告；ActiveWarning、`WeatherWarnings::changes_since` |
| `source_health` | SourceId：MtrLineStatus、MtrNextTrain、HkoWarnings、MtrOpenData；HealthState：Starting、Healthy、Failing、Blind；HealthChange |

未知上游值保留喺 `Unknown`／`Unrecognised` 並記日誌，唔會令整份文件失效。無法辨認月台就 `Platforms::NONE`，保留列車。

<a id="outbound-http"></a>

## 出站 HTTP

用 `dut-http` 同共用連線池，唔好每個請求建 client。Bootstrap 呼叫 `dut_http::build(user_agent, timeout, proxy) -> Result<OutboundHttpClient, reqwest::Error>`。`ProxyMode::System` 用環境／系統代理，測試用 `Direct` 略過。

| 呼叫／型別 | 合約 |
| --- | --- |
| `fetch(UpstreamRequest).await -> Result<UpstreamResponse, UpstreamError>` | GET、讀整個 body，記開始／完成／失敗；非 2xx 係錯誤 |
| `UpstreamRequest { upstream, url, timeout }` | 穩定日誌名、URL、請求逾時 |
| `json::<T>() -> Result<T, serde_json::Error>` | 解碼，失敗時記診斷節錄 |
| `ttl_hint() -> Option<Duration>` | 上游 max-age 減 Age |
| `last_modified() -> Option<Timestamp>` | 解析 Last-Modified |
| `body() -> &Bytes` | 原始回應位元組 |
| `UpstreamError` | Transport 或 Status，包入適配器自己嘅錯誤 |

<a id="upstream-adapters"></a>

## 上游適配器

`dut-upstream`；下面係建構呼叫形式，傳共用 client 同 bootstrap 嘅 endpoint／逾時。

| 建構／型別 | 角色 |
| --- | --- |
| `mtr::next_train::MtrNextTrainSource::new(http, endpoint, timeout, CachePolicy)` | 按路綫／車站快取嘅 NextTrainSource |
| `mtr::line_status::MtrLineStatusFeed::new(http, endpoint, timeout)` | `Feed<Item = NetworkStatus>` |
| `hko::warnings::HkoWarningFeed::new(http, endpoint, timeout)` | `Feed<Item = WeatherWarnings>` |
| `mtr::open_data::MtrOpenDataFeed::new(http, &base_url, timeout)?` | Feed<Item = ReferenceData>，七檔順序下載，URL 以 / 結尾 |
| `connectivity::ConnectivityCheck::new(http, probes)` | 每上游一個啟動探測 |
| `connectivity::Probe::json(request)`, `Probe::csv(request)` | 驗證預期格式，拒絕登入頁 HTML |
| `CachePolicy` | default_ttl、ttl_floor、ttl_ceiling、stale_while_revalidate、stale_if_error、failure_backoff |

適配器提供 `probe()` 畀 bootstrap 做連通性檢查。`RefreshingCache` 保持喺 `dut-upstream` 私有。

<a id="simulated-data"></a>

## 模擬資料

`dut-mock` 唔做 I/O，回傳同真實來源一樣嘅 domain 值，所以 Mock 路由可以共用正式 DTO 同應用服務。詳見[模擬設計](ARCHITECTURE.md#mock-api)。

| 型別 | 用途 |
| --- | --- |
| `Scenario` | BoardScenario／StatusScenario 實作 ALL、USUAL、name()、description()、random_weight() |
| `ScenarioChoice<S>` | Random 或 Named(S)，唔分大小寫解析，失敗係 UnknownScenario；`resolve(Option<Seed>) -> (S, Seed)` |
| `Seed` | new(u64)、DEFAULT（0）、fresh()、value()；指定場景預設 DEFAULT，random 冇 seed 就產生新值 |
| `SimulatedNextTrains::new(scenario, seed)` | board(line, station).await 同 station_boards(station).await 用 NextTrainService；事故喺指定綫或一條途經綫 |
| `SimulatedLineStatus::new(scenario, seed)` | `status() -> Result<Snapshot<NetworkStatus>, SourceUnavailable>` |
| `SimulatedStatusChanges::new(scenario, seed)` | `batches() -> Result<impl Iterator<Item = Vec<LineStatusChange>>, SourceUnavailable>`：事故同恢復嘅無限循環，每批係兩個模擬狀態源經領域 diff 得出嘅變更；場景冇變化就係空 |

```rust
let (scenario, seed) = "peak"
    .parse::<ScenarioChoice<BoardScenario>>()?
    .resolve(None);
let view = SimulatedNextTrains::new(scenario, seed)
    .board(Line::EastRail, "SHT".parse()?)
    .await?;
```

新場景加到 BoardScenario 或 StatusScenario，定名、說明同權重，喺 `board::Conditions::of` 或 `network_status` 實作條件，再更新 HTTP_API。班次／月台改動要對住 `tests/fixtures/mtr/next_train_network.json`，測試會同樣本比較。

<a id="conventions"></a>

## 錯誤、日誌同測試

- 每層有自己嘅 `thiserror` 錯誤型別，原因鏈放喺 `#[source]`，錯誤文字唔重複原因。客戶端只收 `ApiError` 映射，唔收上游細節。
- 用結構化 tracing 欄位，例如 `line = %line` 同 `error = &err as &dyn Error`；`dut_telemetry::millis(duration)` 格式化時間。唔可以記秘密。
- 泛型 wrapper 如果 derive 加多餘約束，就手寫 Clone／Debug。不可變值用 Arc 共用。
- 唔可以阻塞 runtime 或跨 await 持有 std 鎖。測試同編譯期 const 求值以外唔用 unwrap、expect 或 panic。
- 非簡單 domain／application 規則要單元測試。時間測試用暫停 Tokio 時鐘，上游用 wiremock 同實際樣本。路由測試驗狀態、content type 同 body。
- 完成前跑 fmt、拒絕 warning 嘅 Clippy 同 cargo test；完整規範見 [AGENTS.md](../../AGENTS.md)。
