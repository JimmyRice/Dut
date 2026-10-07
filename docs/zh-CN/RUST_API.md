# Rust API

[English](../../RUST_API.md) · [繁體粵語](../zh-HK/RUST_API.md) · [简体中文](RUST_API.md)

编写 Dut 业务代码时，可通过本文档查询应传递哪些 domain 类型、调用哪些服务及实现哪些 trait。生产环境的具体依赖在 `src/bootstrap/app.rs` 组装。业务使用的 trait、handle、事件或入口变化时，需要同步本文档及翻译。

## 目录

- [从哪里入手](#where-to-start)
- [应用层 Port](#ports)
- [读取数据](#reading-data)
- [监控与事件](#monitor)
- [业务类型](#domain-vocabulary)
- [出站 HTTP](#outbound-http)
- [上游适配器](#upstream-adapters)
- [模拟数据](#simulated-data)
- [错误、日志与测试](#conventions)

客户端契约见 [HTTP_API.md](HTTP_API.md)，业务代码使用的类型见 [RUST_API.md](RUST_API.md)，设计取舍见 [ARCHITECTURE.md](ARCHITECTURE.md)，开发规范见 [AGENTS.md](../../AGENTS.md)。

Markdown 代码片段用于参考，并非完整可编译程序。公开类型的 Rustdoc 示例由 `cargo test` 编译，可通过 `cargo doc --open` 查看。签名需要与源码一致。引入新来源 ID 的适配器草稿，需要先添加 domain 定义才能编译。

<a id="where-to-start"></a>

## 从哪里入手

| 任务 | 入口 |
| --- | --- |
| 响应变化 | [`Subscriber`](#subscriber) + `MonitorHandle::attach` |
| 读取轮询值 | [`FeedHandle::snapshot`](#feedhandle) |
| 读取到站板 | [`NextTrainService`](#nexttrainservice) |
| 读取参考数据集／文件 | [`ReferenceDataService`](#referencedataservice) |
| 添加后台文档来源 | [`Feed`](#feed) + `dut_poll::spawn` |
| HTTP 获取 | [`OutboundHttpClient::fetch`](#outbound-http) |
| 建立铁路业务模型 | [`dut_core::domain`](#domain-vocabulary) |
| 添加启动选项 | `CommandLine`; [configuration](ARCHITECTURE.md#configuration) |
| 模拟事故 | [`dut-mock`](#simulated-data) |

<a id="ports"></a>

## 应用层 Port

| Trait | 用途 | 实现 |
| --- | --- | --- |
| `Subscriber` | 处理监控事件 | `EventLog` |
| `Feed` | 获取整份文档，不自行缓存 | `MtrLineStatusFeed`, `HkoWarningFeed`, `MtrOpenDataFeed`, `NextTrainSignalFeed` |
| `NextTrainSource` | 提供到站板快照 | `MtrNextTrainSource`; private mock source |
| `LineStatusSource` | 提供全网状态快照 | `FeedHandle<NetworkStatus>` |
| `ReferenceDataSource` | 提供完整参考数据快照 | `FeedHandle<ReferenceData>` |

公开 async port 使用 `fn … -> impl Future<Output = …> + Send` 声明。实现可以写 `async fn`，编译器验证 future 为 Send。优先使用泛型／静态分派。在有具体实现和测试替身的真实边界引入 trait，不为可能出现的使用方预先添加。

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

每个订阅者在自己的 Tokio task 中，按发布顺序逐个处理事件。状态放在 `&mut self`，不得跨 await 持有 std 锁。`NAME` 标识日志 span。事件不重播，因此需要在 bootstrap 时挂接。Broadcast 容量为 256；`on_lagged` 默认无操作，遗漏有影响时请重写以重新同步。

示例：识别正常服务转为结束服务。判断保持纯函数，状态改动放在 `on_event`。

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

启动 monitor 后，立即在 bootstrap 挂接：

```rust
let monitor = dut_monitor::spawn(&line_status, &weather_warnings, &next_train_signals);
monitor.attach(ServiceEnds::default());
```

局部测试可以直接构造事件，无需运行 monitor。以下片段除上方导入外，还需导入 `LineStatus`、`LineStatusChange` 及 `jiff::Timestamp`：

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

每次调用重新获取整份文档；时间安排和最新值由 poller 管理。在 `dut-upstream` 解码 transport DTO，显式转换为 domain 类型，用 `SourceUnavailable::new(error)` 包装适配器错误。`SOURCE` 标识日志及健康事件。新来源需要先添加 `SourceId` 变体，适配器才能编译。

适配器草稿（`Example*` 类型及变体为占位）：

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

监控新文档需要添加 `changes_since` 及单元测试、添加 `Change` 变体、扩展 `dut_monitor::spawn` 和 watcher 组装，并增加事件日志分支。仅轮询不会自动发布业务变化。

<a id="source-ports"></a>

### 来源签名

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

## 读取数据

<a id="snapshot"></a>

### 快照与新鲜度

`dut_core::application::source`

| 项目 | 行为 |
| --- | --- |
| `Snapshot<T>` | Arc 共享值：`value()`、`fetched_at()`、`freshness()` |
| `Freshness::Fresh { expires_in }` | 剩余时段可复用 |
| `Freshness::Stale` | 过期回退；`is_stale()` 检查，`combine` 取最旧／最短剩余期 |
| `SourceUnavailable` | 无可用值；原因仅供日志，不提供给客户端 |

<a id="feedhandle"></a>

### FeedHandle 与 Schedule

`dut_poll::spawn(feed, schedule) -> FeedHandle<F::Item>` 在 Tokio runtime 中启动 feed，handle 克隆成本低。

| 方法／字段 | 契约 |
| --- | --- |
| `snapshot().await -> Result<Snapshot<T>, SourceUnavailable>` | 当前快照，首次轮询未完成时有期限地等待 |
| `subscribe() -> watch::Receiver<FeedState<T>>` | 每次尝试后通知，仅保留最新状态 |
| `source() -> SourceId` | 稳定来源 ID |
| `FeedState<T>` | `latest() -> Option<&Polled<T>>`、`health()`、`attempts()` |
| `Polled<T>` | `value() -> &Arc<T>`、`fetched_at()` |
| `Schedule` | 均为 Duration：`interval`、`first_poll_after`、`retry_after`、`fresh_for`、`stale_if_error`、`blind_after` |

Watch 值应短暂借用，持有它会阻止 poller 发布。`FeedHandle<NetworkStatus>` 实现 `LineStatusSource`，`FeedHandle<ReferenceData>` 实现 `ReferenceDataSource`。`retry_after` 小于 interval 则失败后提前重试，大于或等于则保持正常计划。默认值见[轮询 Feed](ARCHITECTURE.md#polled-feeds)。

<a id="nexttrainservice"></a>

### NextTrainService

`dut_core::application::next_train::NextTrainService<S>`

| 调用 | 结果 |
| --- | --- |
| `new(source: S) -> Self` | 通过 Arc 共享来源 |
| `board(line, station).await` | `Result<BoardView, NextTrainError>` |
| `station_boards(station).await` | `Result<StationBoards, NextTrainError>` |

读取来源前先用内置路网验证，限制缓存 key。全站板并发获取并保留各线错误；没有任何线路成功时才整体失败。`BoardView` 提供 `board()`、`snapshot()`、`upcoming(direction)` 和 `directions()`，过滤已开出超过 30 秒的列车。`StationBoards` 提供 `station()`、`lines()` 和 `freshness()`；各 `LineBoard` 含 `line` 及 `board: Result<BoardView, SourceUnavailable>`。错误为 `UnknownStation(StationCode)`、`StationNotOnLine { line, station }` 或 `Unavailable(SourceUnavailable)`。

<a id="referencedataservice"></a>

### ReferenceDataService 与 LineStatusService

`dut_core::application::reference_data::ReferenceDataService<S>::reference_data().await` 返回 `Result<Snapshot<ReferenceData>, SourceUnavailable>`，全部数据集及来源文件来自同一次成功轮询。`dut_core::application::line_status::LineStatusService<S>::status().await` 返回 `Result<Snapshot<NetworkStatus>, SourceUnavailable>`。两者通过 `new(source)` 构造，克隆成本低。

```rust
let snapshot = service.reference_data().await?;
let data = snapshot.value();
let fare = data.fares.value().get(from, to);
let csv = data.files.get(SourceFile::LinesFares);
```

<a id="monitor"></a>

## 监控与事件

| 入口 | 契约 |
| --- | --- |
| `dut_monitor::spawn(&line_status, &weather_warnings, &next_train_signals) -> MonitorHandle` | 每个 feed 一个 watcher，先挂接事件日志 |
| `MonitorHandle::attach<S: Subscriber>(&self, subscriber: S)` | 在独立 task 中顺序投递 |
| `MonitorHandle::subscribe() -> broadcast::Receiver<MonitorEvent>` | 直接接收循环需处理 Lagged 和 Closed |
| `NextTrainSignalFeed::new(next_trains)` | 每条支持线路抽样一个中段站 |

`dut_core::domain::event::MonitorEvent { observed_at: Timestamp, change: Change }` 记录事实。首次成功数据轮询不产生数据变化，健康转变可以产生事件。失败轮询保留数据，只可能改变来源健康。同一 feed 的观察顺序保持一致。部署应仅有一个角色执行事件副作用，角色选择尚非 CLI 功能。

| Change | 内容／触发 |
| --- | --- |
| `LineStatus(LineStatusChange)` | 前／后 LineStatus，状态或消息变化，含结束服务 |
| `WeatherWarning(WarningChange)` | Issued、Changed { previous, current }、Cancelled，警告级别／更新时间变化 |
| `NextTrainSignal(SignalChange)` | 线路／车站，前／后 signal，延误标记或通告变化 |
| `SourceHealth(HealthChange)` | 来源、前／后 HealthState，含 Starting→Healthy |

<a id="domain-vocabulary"></a>

## 业务类型

以下模块均位于 `dut_core::domain`。输入在边界解析一次，业务代码传递有类型的值。

<a id="network"></a>

### network

| 类型 | 用途 |
| --- | --- |
| `Line` | 不区分大小写解析；`code()`、`name()`、`color()`、`stations()`、`termini()`、`towards(station, direction)`、`leads(from, to, direction)`、`serves(station)`、`Line::serving(station)`、`Line::with_next_train()`、`Line::ALL`。`leads` 要求同一支线且终点位于起点之后。 |
| `StationCode` | Copy newtype，三个大写字母。运行时使用 `"tko".parse::<StationCode>()`；常量使用 `StationCode::from_static("TKO")`，错误常量导致 const 求值失败。 |
| `Station` | `find(code)` 验证路网成员并提供名称，`all()` 按代码排序 |
| `Direction`, `ByDirection<T>` | 港铁 Up／Down；`ByDirection::get(direction)` |

代码格式正确不代表已知车站，需要用 `Station::find` 或 `Line::serves` 检查。`scripts/sync-network.py --write` 重建 GENERATED 标记之间的 `STATIONS`；`line.rs` 的布局、支线及终点由人工审核。

<a id="reference"></a>

### reference

| 类型 | 契约 |
| --- | --- |
| `ReferenceData` | 一次成功轮询：`files: BySourceFile<PublishedFile>`，以及 stations、fares、airport_express_fares、light_rail、light_rail_fares、accessibility 各项 `Dataset` |
| `Dataset<T>` | `new(value, updated_at)` 对内容求 hash；`value()`、`revision()`、`updated_at()` |
| `Revision` | 确定的 FNV-1a：`of(&value)`、`of_bytes(bytes)`，Display 为 16 位十六进制 |
| `SourceFile`, `BySourceFile<T>` | 七个有限来源文件：`file_name()`、FromStr、ALL；容器 `get(file)`、`from_fn`、`try_from_fn` |
| `PublishedFile` | 原始 `body() -> &Arc<[u8]>`、`updated_at()`、`revision()` |
| `Fare` | 整数港仙；`"4.90".parse::<Fare>()` 为 490，`cents()`、`from_cents()` |
| `FareTable<K, F>`, `Trip<K, F>` | `get(from, to)` 查询起终点，`trips()` 已排序；移除同站行程 |
| `RailFares` | `octopus: OctopusFares`、`single_journey: SingleJourneyFares`，重铁及轻铁共用 |
| `AirportExpressFares` | 八达通／单程票各使用 `AdultAndChildFares` |
| `PublishedNetwork` | 公开车站及 Route（line、direction、stations）；`station(code)`、`drift() -> Vec<NetworkDrift>` |
| `LightRailNetwork` | Stop（StopId、StopCode、name）及 LightRailRoute（RouteNumber、按方向停站）；`stop(id)`、`routes_serving(id)` |
| `Accessibility` | FacilityGroup（类别、名称、设施）及 StationAccessibility（已提供的 StationFacility，可选位置） |

`StopCode` 与 `StationCode` 有意分开，轻铁和重铁使用独立代码系统。

<a id="other-domain-types"></a>

### 其他业务类型

| 模块 | 类型／行为 |
| --- | --- |
| `localized` | `Localized<T> { en, tc }` |
| `time` | `HONG_KONG` 固定 UTC+8；jiff Timestamp，`display_with_offset(HONG_KONG)` |
| `line_status` | LineCondition：Normal、Delayed、Disrupted、DelayedOrDisrupted、NonServiceHours、TyphoonSignal、Unknown(String)；`display_color()`；LineStatus；`NetworkStatus::changes_since` |
| `next_train` | NextTrainBoard `signal()`；TrainArrival 绝对时间；Platforms 为 Copy，含 `one(n)`、`pair(1, 3)`、NONE、`"1/3".parse()`、`as_slice()`；AlertNotice、NextTrainSignal、`NextTrainSignals::changes_since` |
| `weather` | WeatherWarning 包含 TropicalCyclone(CycloneSignal)、PreNo8Announcement、Rainstorm(RainstormLevel)、Unrecognised(String) 及其他天文台警告；ActiveWarning、`WeatherWarnings::changes_since` |
| `source_health` | SourceId：MtrLineStatus、MtrNextTrain、HkoWarnings、MtrOpenData；HealthState：Starting、Healthy、Failing、Blind；HealthChange |

未知上游值保留在 `Unknown`／`Unrecognised` 中并记录日志，不会使整份文档失效。无法识别月台时使用 `Platforms::NONE`，保留列车。

<a id="outbound-http"></a>

## 出站 HTTP

使用 `dut-http` 及共享连接池，不要为每个请求构建 client。Bootstrap 调用 `dut_http::build(user_agent, timeout, proxy) -> Result<OutboundHttpClient, reqwest::Error>`。`ProxyMode::System` 使用环境／系统代理，测试通过 `Direct` 绕过。

| 调用／类型 | 契约 |
| --- | --- |
| `fetch(UpstreamRequest).await -> Result<UpstreamResponse, UpstreamError>` | GET、读取完整 body，记录开始／完成／失败；非 2xx 为错误 |
| `UpstreamRequest { upstream, url, timeout }` | 稳定日志名称、URL、请求超时 |
| `json::<T>() -> Result<T, serde_json::Error>` | 解码，失败时记录诊断节选 |
| `ttl_hint() -> Option<Duration>` | 上游 max-age 减 Age |
| `last_modified() -> Option<Timestamp>` | 解析 Last-Modified |
| `body() -> &Bytes` | 原始响应字节 |
| `UpstreamError` | Transport 或 Status，包装为适配器自身错误 |

<a id="upstream-adapters"></a>

## 上游适配器

`dut-upstream`；下表为构造调用形式，传入共享 client 及 bootstrap 的 endpoint／超时。

| 构造／类型 | 角色 |
| --- | --- |
| `mtr::next_train::MtrNextTrainSource::new(http, endpoint, timeout, CachePolicy)` | 按线路／车站缓存的 NextTrainSource |
| `mtr::line_status::MtrLineStatusFeed::new(http, endpoint, timeout)` | `Feed<Item = NetworkStatus>` |
| `hko::warnings::HkoWarningFeed::new(http, endpoint, timeout)` | `Feed<Item = WeatherWarnings>` |
| `mtr::open_data::MtrOpenDataFeed::new(http, &base_url, timeout)?` | Feed<Item = ReferenceData>，七个文件顺序下载，URL 以 / 结尾 |
| `connectivity::ConnectivityCheck::new(http, probes)` | 每个上游一次启动探测 |
| `connectivity::Probe::json(request)`, `Probe::csv(request)` | 验证预期格式，拒绝认证门户 HTML |
| `CachePolicy` | default_ttl、ttl_floor、ttl_ceiling、stale_while_revalidate、stale_if_error、failure_backoff |

适配器提供 `probe()` 供 bootstrap 进行连通性检查。`RefreshingCache` 在 `dut-upstream` 内保持私有。

<a id="simulated-data"></a>

## 模拟数据

`dut-mock` 不执行 I/O，返回与真实来源相同的 domain 值，因此 Mock 路由可共用正式 DTO 及应用服务。详见[模拟设计](ARCHITECTURE.md#mock-api)。

| 类型 | 用途 |
| --- | --- |
| `Scenario` | BoardScenario／StatusScenario 实现 ALL、USUAL、name()、description()、random_weight() |
| `ScenarioChoice<S>` | Random 或 Named(S)，不区分大小写解析，失败为 UnknownScenario；`resolve(Option<Seed>) -> (S, Seed)` |
| `Seed` | new(u64)、DEFAULT（0）、fresh()、value()；指定场景默认 DEFAULT，random 无 seed 时生成新值 |
| `SimulatedNextTrains::new(scenario, seed)` | board(line, station).await 和 station_boards(station).await 使用 NextTrainService；事故位于指定线或一条途经线 |
| `SimulatedLineStatus::new(scenario, seed)` | `status() -> Result<Snapshot<NetworkStatus>, SourceUnavailable>` |

```rust
let (scenario, seed) = "peak"
    .parse::<ScenarioChoice<BoardScenario>>()?
    .resolve(None);
let view = SimulatedNextTrains::new(scenario, seed)
    .board(Line::EastRail, "SHT".parse()?)
    .await?;
```

新场景添加到 BoardScenario 或 StatusScenario，定义名称、说明及权重，在 `board::Conditions::of` 或 `network_status` 实现条件，再更新 HTTP_API。班次／月台改动应对照 `tests/fixtures/mtr/next_train_network.json`，测试会与样本比较。

<a id="conventions"></a>

## 错误、日志与测试

- 每层拥有自己的 `thiserror` 错误类型，原因链保存在 `#[source]` 中，错误文本不重复原因。客户端仅接收 `ApiError` 映射，不接收上游细节。
- 使用结构化 tracing 字段，如 `line = %line` 和 `error = &err as &dyn Error`；`dut_telemetry::millis(duration)` 格式化时长。不得记录秘密。
- 泛型 wrapper 若 derive 引入多余约束，则手写 Clone／Debug。不可变值通过 Arc 共享。
- 不得阻塞 runtime 或跨 await 持有 std 锁。除测试及编译期 const 求值外，不使用 unwrap、expect 或 panic。
- 非简单 domain／application 规则需要单元测试。时间测试使用暂停的 Tokio 时钟，上游使用 wiremock 和实际样本。路由测试验证状态、content type 及 body。
- 完成前运行 fmt、拒绝 warning 的 Clippy 及 cargo test；完整规范见 [AGENTS.md](../../AGENTS.md)。
