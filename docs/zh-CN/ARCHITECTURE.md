# 架构

[English](../../ARCHITECTURE.md) · [繁體粵語](../zh-HK/ARCHITECTURE.md) · [简体中文](ARCHITECTURE.md)

Dut 是一个 Cargo workspace，只有一个依赖组装入口。依赖指向 domain 和 application：HTTP、适配器及后台任务使用业务契约，业务代码保持独立，不绑定传输框架或 I/O。各 crate 的 manifest 约束这些边界。

## 目录

- [进程状态可以重建](#stateless-first)
- [Crate 与边界](#crates)
- [请求与后台流程](#flows)
- [启动配置](#configuration)
- [缓存与新鲜度](#caching-and-freshness)
- [Mock API](#mock-api)
- [监控与投递](#monitor)
- [新增功能](#adding-a-feature)
- [规划：推送通知](#planned-push)
- [部署方向](#deployment)

客户端契约见 [HTTP_API.md](HTTP_API.md)，业务代码使用的类型见 [RUST_API.md](RUST_API.md)，设计取舍见 [ARCHITECTURE.md](ARCHITECTURE.md)，开发规范见 [AGENTS.md](../../AGENTS.md)。

<a id="stateless-first"></a>

## 进程状态可以重建

缓存、feed 快照、开放数据及监控基线都保存在内存中。重启后从上游重建，没有数据库、本地持久化或必需的 volume。可选日志文件仅用于输出，不会读回。如果启动时上游故障，相关接口会返回 `502`，直到首次取得可用数据。

未来功能优先采用不需要跨重启保存状态的设计，再考虑外部服务的幂等键等机制。只有具体功能确实需要时才引入共享存储，不使用本地磁盘。早期持久化通知历史的构想以此规则为准；重启可以丢失冷却时间及去重窗口。

<a id="crates"></a>

## Crate 与边界

| Crate | 职责 | Workspace 依赖 |
| --- | --- | --- |
| `dut-core` | 纯业务类型、差异比较、应用服务、port 及快照 | 无 |
| `dut-telemetry` | 日志过滤、输出、结构化字段及请求区块 | 无 |
| `dut-http` | 共享出站 HTTP 及新鲜度解析 | telemetry |
| `dut-upstream` | 港铁／天文台适配、CSV 清洗、缓存及启动探测 | core, http, telemetry |
| `dut-mock` | Mock API 的纯模拟数据 | core |
| `dut-poll` | 定时 feed、最新值及来源健康 | core, telemetry |
| `dut-monitor` | Feed 监视及事件投递 | core, poll |
| `dut-api` | Axum 路由、DTO、中间件、错误及 AppState | core, mock, telemetry |
| `dut` | 配置、依赖组装及进程生命周期 | 所有 crate |

`dut-core` 不依赖 Axum、Reqwest、Serde、Tokio、数据库、文件系统或网络。工作流程放在 `application`，业务规则放在 `domain`。`dut-api` 通过 `AppState` 接收泛型数据源，不导入上游适配器。生产环境的具体依赖在 `src/bootstrap` 构建；`main.rs` 仅启动 runtime。

模拟时刻表使用近似值，因此不放入面向乘客的 core。轮询与监控分为不同 crate，方便以后按部署角色分工。目前 bootstrap 在同一进程启动两者；仅运行 API 的角色选择属于未来改动，并非现有命令行开关。

<a id="flows"></a>

## 请求与后台流程

```text
HTTP request
  -> dut-api: parse and validate
  -> dut-core: application service and domain rules
  -> dut-upstream cache/adapter OR dut-poll snapshot
  -> dut-api: DTO and HTTP response

Scheduled poll
  -> Feed adapter -> watch channel: latest value + health
  -> API source port reads the snapshot
  -> monitor compares successful values
  -> MonitorEvent -> broadcast -> Subscriber
```

每个 `lib.rs` 和 `mod.rs` 只做索引：文档、模块声明及重新导出。实现文件按用途命名。仅导出其他 crate 使用的项目，其余使用 private 或 `pub(crate)`；`unreachable_pub` 辅助检查。`dut-core` 公开 domain 和 application，根 library 仅公开启动及路由测试入口。

根目录 Cargo 命令覆盖所有默认 workspace 成员。版本和 lint 从 workspace 继承。`dist` profile 使用 fat LTO、单个 codegen unit、移除符号及 panic abort；部分启动与 HTTPS 依赖按体积优化，请求处理保留以速度为主的默认设置。

<a id="configuration"></a>

## 启动配置

发布默认值定义上游 URL、超时、缓存策略及轮询计划。`CommandLine` 使用 clap，在日志启动前一次性把运行选项解析为有类型的值。每个选项都有长参数和环境变量，参数优先；除惯用的 `RUST_LOG` 外，变量均使用 `DUT_`。

相关选项组织为 `clap::Args` struct，再 flatten 到 `CommandLine`。解析时验证格式，依赖或冲突关系用 `requires`／`conflicts_with` 表达。无效值或空值退出码为 2。Crate 仅接收配置值，不自行读取参数或环境。测试解析时排除选项变量，不修改进程环境。

以后引入凭证时，通过环境变量传入，在 help 中隐藏值，遮盖 `Debug` 输出，并标记敏感 header。不得记录秘密。目前没有推送凭证选项；详见下面的规划。

<a id="caching-and-freshness"></a>

## 缓存与新鲜度

缓存绝对数据，在边界计算相对值。保存 `arrival_at`，丢弃上游倒计时。`BoardView` 每次读取都过滤到站时间已过去超过 30 秒的列车（`DEPARTED_GRACE`）。不可变数据通过 `Arc` 共享，DTO 尽量借用。

Next Train 使用 `dut-upstream` 私有的泛型 `RefreshingCache<K, V>`。Key 是经内置路网验证的线路／车站组合，客户端输入无法令缓存无限增长。并发读取共享一次刷新。新鲜期遵循上游 `max-age - Age`，限制在 2–15 秒，默认 10 秒。不启用 stale-while-revalidate；stale-if-error 为过期后 90 秒，失败后退避 5 秒。

到站板先获取英文，站名来自内置路网；仅为特别安排通告额外获取繁体中文。所有出站请求均通过共享 `OutboundHttpClient::fetch`，以结构化日志记录开始、完成和失败。缓存日志覆盖 miss、过期、刷新、合并等待、旧数据回退、后台重新验证及退避。

<a id="polled-feeds"></a>

### 轮询 Feed

| Feed | 间隔／首次轮询 | 新鲜期／旧数据窗口 | 失明阈值 |
| --- | --- | --- | --- |
| 线路状态 | 30 s / 0 s | 33 s / 15 min | 2 min |
| 天文台警告 | 60 s / 0 s | 65 s / 15 min | 5 min |
| 抽样列车信号 | 60 s / 60 s | 63 s / 15 min | 5 min |
| 港铁开放数据 | 24 h / 0 s | 86430 s / 30 d | 48 h |

`dut_poll::spawn` 为每个 feed 启动一个 Tokio task。`MissedTickBehavior::Delay` 避免慢轮询后集中补发请求。开放数据在 5 分钟后重试，其余使用正常间隔。首次轮询未完成时，读取会在计划规定的启动等待期限内等待。失败保留上次成功值；新鲜期和旧数据窗口耗尽后，快照不可用。来源健康分为 `Starting`、`Healthy`、`Failing` 和 `Blind`。

线路状态不使用上游五秒 TTL：发布时间仅在状态变化时更新，并非心跳。HTTP 新鲜度由轮询计划决定。新鲜响应使用 `public, max-age=N`，旧数据使用 `no-cache`；支持的响应通过协商使用 gzip。

<a id="open-data"></a>

### 开放数据使用同一快照

`MtrOpenDataFeed` 使用可复用连接顺序读取七个 CSV。全部下载及清洗成功才发布，否则保留上一份完整数据。适配器转换车站 ID、把车费解析为整数港仙、修正已知文本问题、记录无法匹配而跳过的行，并拒绝格式错误的值。原始文件保留原始字节。

清洗结果和原始字节各有确定的版本。数据集 ETag 额外包含服务版本，原始文件不包含。`/api/data` 列出版本，数据集和文件支持 `If-None-Match`。每份抓取状态的 JSON／CSV 与最高级别 gzip 在 blocking pool 编码一次，然后共享。编码 key 包含 `fetched_at` 和 `stale`，因为这些字段可以在版本不变时改变。响应附带 `Content-Length` 和 `Vary: Accept-Encoding`。

Next Train 仍以内置路网为准。轮询报告公开路网差异，`scripts/sync-network.py` 生成改动供审核。启动探测独立检查实际适配器 URL 及文档格式；探测失败仅报告问题，不会停止监听。

<a id="mock-api"></a>

## Mock API

需要先启用的 `/api/mock` 路由共用正式 DTO 和应用层验证。`dut-mock` 提供纯模拟来源，没有轮询、缓存或 I/O。场景和 seed 决定各项可重现选择；绝对时间推进时刻表，无需保存请求状态。月台及中途折返参考实际抓取数据，班距仍是近似值。每次请求每线每方向最多模拟 64 班车。

<a id="monitor"></a>

## 监控与投递

监控使用 domain 的 `changes_since` 比较连续成功值，发布每个观察到的变化：线路状态或消息、天文台警告生命周期、抽样到站板延误／通告及来源健康。事件描述事实；严重程度、阈值、跨来源关联和通知策略由订阅者决定。轮询失败不代表服务恢复或警告取消。

首次成功数据建立基线，不产生数据变化事件；健康状态变化仍可能发布。Bootstrap 立即挂接 `Subscriber`，每个在自己的 task 中顺序处理事件，使用可变状态而无需锁。Broadcast 保留 256 个事件，不重播历史。落后的订阅者收到 `on_lagged(missed)`；若遗漏会影响业务，应重新同步。事件日志在 watcher 开始前已挂接。

Next Train 监控每分钟抽查每条线一个中段车站，通过乘客共用的缓存服务；不计额外本地化请求，每分钟最多增加十次到站板抓取。这并非全网事故检测。运输署自由文本交通消息仍待分类方案，暂未接入。

<a id="adding-a-feature"></a>

## 新增功能

1. 在 `dut-core/domain` 添加业务概念及有测试的规则。
2. 工作流程放在 `application`；具体用例需要外部数据时才添加 port。
3. 在 `dut-upstream` 实现适配器，使用共享 HTTP client。按请求获取的到站板使用 key 缓存，定时读取整份文档使用 `Feed`。
4. 在 bootstrap 组装具体依赖，通过 `AppState` 传递服务。
5. 在 `dut-api` 添加 DTO 和轻量 handler；响应改动同步 Mock。
6. 使用离线样本测试业务决策，以及路由状态、content type 和 body。
7. 更新 `HTTP_API.md` 契约及变更记录、`RUST_API.md` 业务接口和全部语言版本。运行 [AGENTS.md](../../AGENTS.md) 的三项必要检查。

<a id="planned-push"></a>

## 规划：推送通知

`dut-push` 尚未存在。计划依赖 core、http 和 telemetry，实现 `Subscriber`，通过供应商适配器转换业务通知。目前候选是 OneSignal；以后有 APNs 或 FCM 第二个供应商时再抽象接口。聊天频道不属于此规划。

策略保持纯函数，阈值、冷却及去重独立于发送进行测试。通知使用业务受众、`Localized` 标题和内容，以及由事件派生的 collapse／幂等键。供应商 tag 按线路代码定位受众，不保存用户数据；发布前与 App 确定 tag 名称。依靠幂等键处理重启或部署重叠前，需要确认供应商语义及保留时间。

拟议选项为 `--onesignal-app-id`／`DUT_ONESIGNAL_APP_ID` 和 `--onesignal-api-key`／`DUT_ONESIGNAL_API_KEY`。均未设置则关闭推送，两者非空则启用，缺一或为空则启动失败。这些仅是设计记录，当前 CLI 不接受。

<a id="deployment"></a>

## 部署方向

当前服务是一个程序，通过进程内 channel 连接。多个实例各自重建缓存并轮询，上游负载随实例数增加，监控也会重复发布事件。扩展前需要增加配置，把 monitor 和 push 限定在一个角色，并使用幂等机制处理替换重叠。存在跨进程消费者时才把事件移到 Redis 或 NATS；可序列化事件 DTO 放在 `dut-core` 之外。
