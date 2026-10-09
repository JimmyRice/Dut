# Dut (嘟)

[English](../../README.md) · [繁體廣東話](../zh-HK/README.md) · [简体中文](README.md)

Dut 是 MTRGo (Not Yet Released or Open-Sourced) 的 Rust 后端，将香港港铁开放数据整理成 App 可以直接使用的 JSON：线路和车站、服务状态、列车到站、车费、轻铁路线及无障碍设施。

## 目录

- [本地运行](#quick-start)
- [HTTP 接口](#endpoints)
- [使用模拟数据开发](#mock-api)
- [缓存与数据新鲜度](#freshness)
- [配置](#configuration)
- [容器](#docker)
- [发布](#releases)
- [日志与运维](#logging)
- [开发](#development)
- [数据来源](#sources)

- **按乘客需求组织。** 行车方向附带月台指示牌上的终点，名称同时提供英文和繁体中文。到站时间使用绝对时间，App 在本地更新倒计时。
- **共享上游请求。** 内存缓存合并同一到站板的请求。上游故障时，有可用旧数据就返回，并标记 `stale: true`。
- **适合 Local First。** 车费和设施下载一次后，可以比较版本或使用条件请求同步；也提供原始 CSV。
- **便于追查。** 响应附带 `x-request-id`，可以把用户反馈与服务端日志对应起来。

<a id="quick-start"></a>

## 本地运行

Edition 2024 至少需要 Rust 1.85；当前依赖可能要求更新的工具链。启动服务后，可以查询将军澳站：

```bash
cargo run
```

```bash
curl http://127.0.0.1:3000/api/lines/TKL/stations/TKO/next-trains
```

默认监听 `127.0.0.1:3000`。以下保留了 2026-09-28 的深夜实际响应节选；当时往北角的末班车已开出。

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
| `GET` | `/api/mock/health` | Mock 可用性检查；需要先启用 |
| `GET` | `/api/mock/scenarios` | 可用模拟场景；需要先启用 |
| `GET` | `/api/mock/lines/status` | 模拟服务状态 |
| `GET` | `/api/mock/lines/{line}/stations/{station}/next-trains` | 模拟单线到站板 |
| `GET` | `/api/mock/stations/{station}/next-trains` | 模拟全站到站板 |

参数、响应字段、缓存、错误和示例见 [HTTP_API.md](HTTP_API.md)。

<a id="mock-api"></a>

## 使用模拟数据开发

需要随时测试延误、台风、末班车或上游故障时，可以启用 Mock API。它与正式接口共用响应 DTO，客户端只需把 `/api` 换成 `/api/mock`。

```bash
cargo run -- --mock-api
```

```bash
curl 'http://127.0.0.1:3000/api/mock/stations/ADM/next-trains?scenario=delayed&seed=7'
```

`scenario` 选择情境，不传则按权重随机选择。`seed` 选择一个模拟世界，固定它进行轮询，就能看到列车随时间到站和开出。响应通过 `x-mock-scenario` 和 `x-mock-seed` 给出实际值。[场景目录](HTTP_API.md#mock-scenarios) 列出了全部选项。Mock 路由默认关闭，未启用时返回 `404`。

<a id="freshness"></a>

## 缓存与数据新鲜度

| 数据 | 新鲜期 | 故障处理 |
| --- | --- | --- |
| 列车到站 | 通常 10 秒，限制在 2–15 秒 | 过期后可顶替 90 秒；失败后等待 5 秒再试 |
| 线路状态 | 每 30 秒轮询；新鲜期 33 秒 | 新鲜期之后可顶替 15 分钟 |
| 内置路网 | 随部署更新；max-age=86400 | 无需访问上游 |
| 开放数据 | 启动时及每 24 小时拉取；新鲜期 86430 秒 | 5 分钟后重试；过期后可顶替最多 30 天 |

可以用 `Cache-Control: public, max-age=N` 的剩余秒数安排轮询。旧数据响应使用 `no-cache`，复用前需要重新验证。缓存仅保存在单个进程中：重启会丢失，多个实例会增加上游请求量。详见[缓存设计](ARCHITECTURE.md#caching-and-freshness)。

<a id="configuration"></a>

## 配置

| 参数 | 环境变量 | 默认值 | 用途 |
| --- | --- | --- | --- |
| `--bind-address <ADDRESS>` | `DUT_BIND_ADDRESS` | `127.0.0.1:3000` | IP 和端口；不接受主机名 |
| `--log-level <LEVEL>` | `RUST_LOG` | `info,dut=debug,tower_http=debug` | 日志过滤规则 |
| `--log-file <PATH>` | `DUT_LOG_FILE` | 无 | 额外追加一份纯文本日志 |
| `--mock-api` | `DUT_MOCK_API` | 关闭 | 启用模拟路由 |

命令行参数优先于环境变量。`DUT_MOCK_API` 接受 `true/false`、`1/0`、`yes/no` 和 `on/off`。`dut --help` 列出选项，`dut --version` 显示版本。使用 Cargo 时，参数放在 `--` 后面。空值或格式错误会导致启动失败，退出码为 2。

```bash
cargo run -- --bind-address 0.0.0.0:3000 --log-level info
```

上游地址、超时及缓存策略使用 [config.rs](../../src/bootstrap/config.rs) 中的默认值。收到 `SIGTERM` 或 Ctrl-C 后，服务停止接收新请求，等待正在处理的请求完成。出站 HTTP 使用环境变量或 macOS 系统代理；macOS 的忽略列表不会生效，需要直连的主机请设置 `NO_PROXY`。测试会直接连接本地模拟上游。

<a id="docker"></a>

## 容器

Dockerfile 使用 Alpine 和 `xx` 交叉编译静态 musl 程序，再放入包含 CA 证书和非 root 用户的 distroless static 镜像。镜像没有 shell 或 curl，并设置了 `DUT_BIND_ADDRESS=0.0.0.0:3000`。

```bash
docker build -t dut .
docker run --rm -p 3000:3000 dut
```

```bash
docker buildx build --platform linux/amd64,linux/arm64 -t dut .
```

镜像默认使用 `dist` profile 构建。传入 `PROFILE=min` 可在同一基础镜像上构建体积最小的程序：

```bash
docker build --build-arg PROFILE=min -t dut:min .
```

健康检查由编排系统或负载均衡器请求 `GET /api/health`；镜像没有 `HEALTHCHECK`。

```bash
docker run --rm -p 3000:3000 ghcr.io/jimmyrice/dut:latest
```

<a id="releases"></a>

## 发布

修改 `Cargo.toml` 的 `[workspace.package].version`，提交后推送对应的 `v<version>` tag。版本不一致会在编译前失败。以下版本仅为示例：

```bash
git tag v0.6.0
git push origin v0.6.0
```

[release.yml](../../.github/workflows/release.yml) 发布六个平台的 `dist` 与 `min` 两种 profile 程序、Linux 的 UPX 压缩 `min` 程序及 `SHA256SUMS`；包含 `-` 的 tag 标为预发布。[docker.yml](../../.github/workflows/docker.yml) 推送版本号和次版本 tag，从 1.0 起增加主版本 tag；预发布不更新 `latest`。`master` 上相关代码或 Dockerfile 改动会更新 `edge`。每个 tag 都有带 `-min` 后缀的 `min` 版本，例如 `latest-min`。手动运行 Release 工作流只保存构建产物，不创建 release。

| 平台 | 架构 | 压缩包 |
| --- | --- | --- |
| Linux | x86-64 | `dut-x86_64-unknown-linux-musl.tar.gz` |
| Linux | arm64 | `dut-aarch64-unknown-linux-musl.tar.gz` |
| macOS | Apple Silicon | `dut-aarch64-apple-darwin.tar.gz` |
| macOS | Intel | `dut-x86_64-apple-darwin.tar.gz` |
| Windows | x86-64 | `dut-x86_64-pc-windows-msvc.zip` |
| Windows | arm64 | `dut-aarch64-pc-windows-msvc.zip` |

`min` 压缩包在扩展名前加 `-min`，例如 `dut-x86_64-unknown-linux-musl-min.tar.gz`；Linux 的 UPX 版本加 `-min-upx`。

`dist` 是默认版本，也是部署时应选的版本，兼顾体积与请求速度。`min` 将所有 crate 按体积优化，程序约小三分之一，但请求处理较慢。UPX 能进一步缩小 `min` 程序的磁盘体积，但每次启动都会解压到私有内存，运行时占用的内存反而更多。本地执行 `cargo build --profile min` 可得到 `target/min/dut`。

Linux 版本静态链接，不需要 glibc；自行使用精简容器时仍需安装 `ca-certificates`，证书位于其他位置时可设置 `SSL_CERT_FILE`。Windows 版本包含 C runtime。发布构建使用 fat LTO 并移除符号；panic 会终止进程，部署时请配置自动重启。本地执行 `cargo build --profile dist` 可得到同一 profile 的 `target/dist/dut`，编译时间比 release 更长。

仓库已公开，发布工作流会为后续发布的程序压缩包和容器镜像附上 GitHub 签署的构建来源证明。可以这样验证压缩包：

```bash
gh attestation verify dut-x86_64-unknown-linux-musl.tar.gz -R JimmyRice/Dut
```

<a id="logging"></a>

## 日志与运维

默认输出 Dut debug 日志。需要减少输出时可设置 `--log-level info`，也可使用 `dut=debug` 等 target 规则，按前缀匹配 `dut_api`、`dut_upstream` 等。无效规则和空值会导致启动失败。第一条 `logging started` 日志记录生效规则及文件路径。

终端将每个请求的日志归为一个彩色区块；`NO_COLOR=1` 关闭颜色。管道及日志文件每条记录占一行纯文本。`--log-file` 额外追加一份日志，父目录必须已存在。Dut 不轮转文件；使用 logrotate 时请设置 `copytruncate`，或在轮转后重启。容器通常收集 stdout 即可。

启动探测记录各上游是否可达，不阻塞监听。后台轮询记录来源、清洗条数、路网差异及健康状态转变（`failing` 或 `blind`）。监控事件包含收车等日常变化。首次成功数据仅用作基线；启动时的数据源健康状态变化仍可能产生事件。

<a id="development"></a>

## 开发

```bash
cargo fmt -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo deny check advisories bans sources
```

完成改动前，请在 workspace 根目录运行以上检查。`cargo deny` 需先执行 `cargo install cargo-deny --locked`，检查依赖的安全公告、被撤回的版本和未知来源，不检查许可证；Code quality workflow 会在每次 push 和 pull request 时运行以上四项检查。局部开发可加 `-p <crate>`。测试可重现且离线：时间行为使用暂停的 Tokio 时钟，上游使用 wiremock 和实际抓取样本。路由测试组装完整 App；macOS 会提高文件描述符上限，以支持并行测试服务器。

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

### 更新内置路网

每日轮询报告路网差异时，先启动服务，再运行这个仅使用 Python 标准库的脚本：

```bash
python3 scripts/sync-network.py
```

它比较 `/api/data/stations` 和 `/api/lines`。加 `--write` 会重建 `station.rs` 的 `STATIONS` 表，并保留马场等仅存在于内置数据的车站。线路顺序、支线及终点需要人工审核；脚本只打印 `codes![...]` 建议，不修改 `line.rs`。之后进行格式化、测试和 diff 检查。无差异退出码为 0，有差异为 1。

客户端契约见 [HTTP_API.md](HTTP_API.md)，业务代码使用的类型见 [RUST_API.md](RUST_API.md)，设计取舍见 [ARCHITECTURE.md](ARCHITECTURE.md)，开发规范见 [AGENTS.md](../../AGENTS.md)。

<a id="sources"></a>

## 数据来源

- [港铁 Next Train API](https://rt.data.gov.hk/v1/transport/mtr/getSchedule.php)
- [港铁线路状态](https://tnews.mtr.com.hk/alert/ryg_line_status.json)
- [港铁开放数据平台](https://opendata.mtr.com.hk/)
- [香港天文台警告信息](https://data.weather.gov.hk/weatherAPI/opendata/weather.php?dataType=warningInfo&lang=en)

天气警告每分钟轮询一次，用于监控事件；目前没有公开天气接口。
