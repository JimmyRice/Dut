# Dut（嘟）

Dut 是一个用 Rust 编写的港铁（MTR）实时数据 API，也是 MTRGo App 的数据整合后端。它把港铁的开放数据整理成方便 App 直接使用的 JSON：线路和车站资料、全线路服务状态、每个车站接下来几班列车的到站时间，以及车费、轻铁和无障碍设施这些很少变动的资料。

- **按乘客的视角组织数据**：每个行车方向都附带月台指示牌上的终点站（例如"往 寶琳／康城"），站名同时提供中英文。列车到站只给绝对时间，倒计时由 App 在本地计算。
- **不给上游添负担**：上游数据缓存在进程内，同一份数据同一时间只会向港铁发一个请求。上游出故障时，先返回标记为 `stale` 的旧数据，不直接报错。
- **适合 Local First**：车费、轻铁、无障碍设施等资料每天从港铁开放数据平台拉取一次，清洗成结构化 JSON，也可以原样下载 CSV。App 比较索引里的 `revision`，或带 `If-None-Match` 请求，就只在资料变化时才重新下载。
- **方便排查问题**：每个响应都带 `x-request-id`，服务端日志按请求分组，同一个请求的日志排在一起。

## 接口

| 方法 | URL | 用途 |
|---|---|---|
| `GET` | `/api/lines` | 所有线路、车站、中英文站名、行车方向和线路颜色 |
| `GET` | `/api/lines/status` | 全线路服务状态（正常、延误、受阻等） |
| `GET` | `/api/lines/{line}/stations/{station}/next-trains` | 某条线在某个车站的下几班列车 |
| `GET` | `/api/stations/{station}/next-trains` | 途经某个车站的所有线路的下几班列车，适合换乘站 |
| `GET` | `/api/health` | 健康检查，只返回 `200`，不带响应体 |
| `GET` | `/api/data` | 开放数据索引：各数据集和原始文件的版本 |
| `GET` | `/api/data/sources/{file}` | 原样返回港铁开放数据的 CSV 文件，例如 `mtr_lines_fares.csv` |
| `GET` | `/api/data/stations` | 开放数据里的车站和各线路的行车路线 |
| `GET` | `/api/data/fares` | 重铁车费（港仙） |
| `GET` | `/api/data/airport-express-fares` | 机场快綫车费 |
| `GET` | `/api/data/light-rail` | 轻铁车站和路线 |
| `GET` | `/api/data/light-rail-fares` | 轻铁车费 |
| `GET` | `/api/data/accessibility` | 无障碍设施目录和各站设施 |
| `GET` | `/api/mock/...` | 开发用的 Mock 接口，按场景模拟线路状态和列车到站，默认关闭，见下方 [Mock 接口](#mock-接口) |

参数、字段说明、缓存行为和错误码见 [HTTP_API.md](HTTP_API.md)。

## Mock 接口

开发 App 时，很多情况很难等到：八号风球、线路暂停、尾班车、港铁故障。用 `--mock-api` 启动服务后，`/api/mock` 下的接口会按场景返回模拟数据，结构与正式接口完全相同，App 只需把路径前缀从 `/api` 换成 `/api/mock`：

```bash
cargo run -- --mock-api
```

```bash
curl 'http://127.0.0.1:3000/api/mock/lines/status?scenario=typhoon_signal'
```

```bash
curl 'http://127.0.0.1:3000/api/mock/stations/ADM/next-trains?scenario=delayed&seed=7'
```

- `scenario` 选择场景，例如 `peak`、`last_train`、`delayed`、`partial_outage`；不传或传 `random` 时按权重随机抽一个。`GET /api/mock/scenarios` 列出所有场景和说明。
- `seed` 决定模拟出哪一组数据。同一个 `seed` 的列车按真实时间运行，轮询时会逐渐接近、到站、离开。响应头 `x-mock-scenario` 和 `x-mock-seed` 写明实际用了哪个场景和 `seed`，可以据此重现。
- 模拟数据尽量贴近真实：月台编号、支线和中途折返班次（將軍澳綫往康城、觀塘綫往何文田、東鐵綫往大埔墟等）取自港铁实时数据，各线班距按繁忙、非繁忙、深夜区分，线路状态的说明文字仿照港铁通告。

完整说明见 [HTTP_API.md 的 Mock 接口](HTTP_API.md#mock-接口的共同行为)。乘客使用的正式部署不需要开启，这样 App 正式版万一误调 Mock 接口，只会得到 404。

## 快速开始

需要 Rust 1.85 或更新版本（项目使用 edition 2024）。

```bash
cargo run
```

服务默认监听 `http://127.0.0.1:3000`，只接受本机访问。查询将军澳綫将军澳站的列车：

```bash
curl http://127.0.0.1:3000/api/lines/TKL/stations/TKO/next-trains
```

响应（凌晨 01:09 的真实数据，每个方向只保留一班车；这时往北角的尾班车已经开出）：

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

## 缓存与数据新鲜度

| 数据 | 新鲜期 | 上游出故障时 |
|---|---|---|
| 列车到站 | 跟随港铁 CDN 的 `max-age`，通常 10 秒 | 90 秒内的旧数据仍会返回，标记为 `stale` |
| 线路状态 | 后台每 30 秒拉取一次，`max-age` 最多 33 秒 | 15 分钟内的旧数据仍会返回，标记为 `stale` |
| 线路与车站资料 | 编译在服务里，随部署更新 | 不访问上游 |
| 开放数据（车费、轻铁、无障碍设施等） | 启动时拉取，之后每天一次，`max-age` 最长约一天，带 `ETag` | 失败后 5 分钟重试；30 天内的旧数据仍会返回，标记为 `stale` |

响应头 `Cache-Control: public, max-age=N` 表示数据还有多少秒算新鲜，App 可以直接用它作为下次轮询的间隔。

缓存只存在于单个进程里。同时运行多个实例时，发往港铁的请求量会按实例数成倍增加。完整的缓存策略见 [ARCHITECTURE.md](ARCHITECTURE.md#caching-and-freshness)。

## 配置

启动参数可以写在命令行上，也可以用对应的环境变量设置，两者都有时以命令行为准。容器里一般用环境变量，在终端里手动运行时用命令行更方便。

| 命令行参数 | 环境变量 | 默认值 | 作用 |
|---|---|---|---|
| `--bind-address <ADDRESS>` | `DUT_BIND_ADDRESS` | `127.0.0.1:3000` | 监听的 IP 和端口，例如 `0.0.0.0:3000` 或 `[::]:3000`。只能写 IP，不能写主机名 |
| `--log-level <LEVEL>` | `RUST_LOG` | `info,dut=debug,tower_http=debug` | 输出哪些日志，见[日志](#日志) |
| `--log-file <PATH>` | `DUT_LOG_FILE` | 不写文件 | 把日志额外追加到这个文件，见[日志](#日志) |
| `--mock-api` | `DUT_MOCK_API` | 关闭 | 开启 `/api/mock` 下的 Mock 接口，见 [Mock 接口](#mock-接口)。环境变量接受 `true`/`false`、`1`/`0`、`yes`/`no`、`on`/`off` |

`dut --help` 列出所有参数，`dut --version` 输出版本号。用 `cargo run` 时，参数写在 `--` 后面：

```bash
cargo run -- --log-level info
```

参数值不合法或为空时（例如 `DUT_BIND_ADDRESS=localhost:3000`），服务不会启动，而是输出原因并以退出码 2 退出。

港铁接口地址、超时和缓存策略都是 [src/bootstrap/config.rs](src/bootstrap/config.rs) 里的默认值，修改后重新编译即可。

服务收到 `SIGTERM` 或 Ctrl-C 后不再接受新请求，等正在处理的请求完成后退出。

访问港铁等上游时，服务会使用 `HTTPS_PROXY`、`HTTP_PROXY` 环境变量或 macOS 系统设置里的代理。macOS 系统代理的"忽略这些主机与域"列表对它不生效，需要直连的主机要写进 `NO_PROXY`。测试不受代理影响，总是直连本机的模拟上游。

## Docker

镜像分两阶段构建：先在 Alpine 里编译出完全静态链接的 musl 程序，再放进只有 CA 证书和非 root 用户的 [distroless static](https://github.com/GoogleContainerTools/distroless) 基础镜像。整个镜像不到 10 MB（arm64 约 8 MB，amd64 约 9 MB），里面没有 shell。

```bash
docker build -t dut .
```

```bash
docker run --rm -p 3000:3000 dut
```

镜像里已经设置了 `DUT_BIND_ADDRESS=0.0.0.0:3000`。其他参数可以用 `-e` 设置环境变量，也可以直接写在镜像名后面，例如 `docker run --rm -p 3000:3000 dut --log-level info`。编译阶段始终在本机架构上运行，用 [xx](https://github.com/tonistiigi/xx) 交叉编译，所以在 Apple Silicon 上构建 amd64 镜像也不需要模拟 CPU：

```bash
docker buildx build --platform linux/amd64,linux/arm64 -t dut .
```

镜像里没有 `curl`，所以没有 `HEALTHCHECK`。要做健康检查，可以让编排系统或负载均衡请求 `GET /api/health`：它只返回 `200`、不带响应体，不访问上游，也不记入请求日志。

构建好的镜像发布在 GitHub Container Registry，同时提供 amd64 和 arm64 版本。仓库是私有的，拉取前需要先用有 `read:packages` 权限的 token 执行 `docker login ghcr.io`：

```bash
docker run --rm -p 3000:3000 ghcr.io/jimmyrice/dut:latest
```

## 发布

先把根目录 `Cargo.toml` 里 `[workspace.package]` 的 `version` 改成新版本并提交（所有 crate 共用这个版本号），再推送同名 tag：

```bash
git tag v0.2.0
```

```bash
git push origin v0.2.0
```

tag 必须是 `v` 加上 `Cargo.toml` 里的版本号，不一致时发布会在编译前失败。推送后 GitHub Actions 会做两件事：

- [release.yml](.github/workflows/release.yml)：编译 6 个平台的二进制文件，和 `SHA256SUMS` 一起发布到 GitHub Releases。tag 里带 `-`（例如 `v0.2.0-rc.1`）时标记为预发布版本。
- [docker.yml](.github/workflows/docker.yml)：构建镜像并推送到 `ghcr.io/jimmyrice/dut`，标签为 `0.2.0`、`0.2` 和 `latest`，1.0 之后还会加上主版本号标签，预发布版本不更新 `latest`。另外，`master` 上改动了代码或 Dockerfile 的提交会更新 `edge` 标签。

| 系统 | 架构 | 文件 |
|---|---|---|
| Linux（任何发行版） | x86-64 | `dut-x86_64-unknown-linux-musl.tar.gz` |
| Linux（任何发行版） | arm64 | `dut-aarch64-unknown-linux-musl.tar.gz` |
| macOS | Apple 芯片 | `dut-aarch64-apple-darwin.tar.gz` |
| macOS | Intel | `dut-x86_64-apple-darwin.tar.gz` |
| Windows | x86-64 | `dut-x86_64-pc-windows-msvc.zip` |
| Windows | arm64 | `dut-aarch64-pc-windows-msvc.zip` |

Linux 版本是完全静态链接的 musl 程序，不依赖 glibc，在正常安装的发行版上可以直接运行。只有一种情况需要注意：把它放进自己用精简基础镜像（例如 `debian:13-slim`）构建的容器里运行时，要先安装 `ca-certificates`。程序用系统自带的 CA 证书验证港铁接口的 HTTPS 连接，精简镜像里没有这些证书，启动时会报 `No CA certificates were loaded from the system` 并退出。项目自己的镜像已经带了 CA 证书，不受影响。证书不在标准位置时，可以用环境变量 `SSL_CERT_FILE` 指定证书文件。

Windows 版本静态链接了 C 运行时，不需要安装 Visual C++ Redistributable。

发布的二进制文件和镜像都用 `Cargo.toml` 里的 `dist` profile 编译：开启 fat LTO，把所有 crate 和依赖当作一个整体优化，并去掉符号表。访问上游的 HTTPS 客户端每次轮询才运行一次，按体积优化；处理请求的代码仍按速度优化。程序 panic 时直接退出而不是只结束出错的任务，部署时应让容器或服务管理器自动重启它。panic 信息会先写进日志；在容器里程序是 1 号进程，内核会忽略发给它的 `SIGABRT`，所以退出码是 139 而不是 134，看起来像段错误。本地想得到和发布版一样的程序，可以运行 `cargo build --profile dist`，产物在 `target/dist/dut`，编译时间比 `--release` 长很多。

在 Actions 页面手动运行 Release 工作流，会编译同样的 6 个文件，但只保存为工作流的 artifact，不创建 release，适合在打 tag 前检查工作流的改动。

仓库公开后，二进制文件和镜像会附带 GitHub 签名的构建来源证明（artifact attestation），可以这样验证：

```bash
gh attestation verify dut-x86_64-unknown-linux-musl.tar.gz -R JimmyRice/Dut
```

## 日志

- **日志级别**：用 `--log-level` 或 `RUST_LOG` 设置，默认值是 `info,dut=debug,tower_http=debug`，会输出缓存命中等调试信息。可以只写一个级别（`error`、`warn`、`info`、`debug`、`trace`，或用 `off` 关闭日志），也可以用 `RUST_LOG` 的语法给不同 crate 设置不同级别。target 按前缀匹配，所以 `dut` 同时覆盖 `dut_upstream`、`dut_api` 等所有 crate。想安静一些可以用 `cargo run -- --log-level info`。和 `RUST_LOG` 的一般用法不同，拼错的级别（例如 `debgu`）和空值会让服务拒绝启动，而不是悄悄地不再输出日志。启动后的第一条日志 `logging started` 会记录实际生效的过滤规则和日志文件路径。
- **写入文件**：`--log-file /var/log/dut.log` 会在终端或标准输出之外，把每条日志再以一行纯文本追加到这个文件，格式和输出到管道时相同。文件不存在时会自动创建，但所在目录必须已经存在，否则服务不会启动。服务本身不轮转日志文件；用 logrotate 轮转时要配置 `copytruncate`，否则服务会一直写入被改名的旧文件，直到重启。容器里一般不需要这个参数，直接收集标准输出即可。
- **在终端里运行时**：同一个请求的所有日志合成一块，开头是一行摘要，并带颜色。设置 `NO_COLOR=1` 可以关闭颜色。
- **输出到文件或管道时**：每条日志一行，不带颜色，方便 `grep` 和日志收集工具处理。
- **启动时的连通性检查**：开始监听后，服务会在后台向每个上游各发一个请求，检查能否连上并拿到 JSON（开放数据是 CSV），每个上游输出一行 `upstream reachable` 或 `upstream unreachable`（带耗时和错误原因），最后输出一行汇总：`every upstream is reachable` 或 `some upstreams are unreachable`。检查不会阻塞请求，也不会因为上游不通而退出，上游恢复后缓存会自动重试。
- **后台轮询与监控事件**：线路状态、天文台警告、各线抽查站的 Next Train 和港铁开放数据在后台定时拉取，日志带 `poll{source=...}`。开放数据每次拉取后会输出 `cleaned open data`（各数据集的条数）；开放数据与编译进服务的车站资料不一致时会输出 warn 日志，提示运行 `scripts/sync-network.py`。数据源连续失败时会输出 `source is failing` 或 `source is blind`。每一次数据变化（包括每晚收车时 `normal` 变 `non_service_hours`）都会输出一行 `event: ...`，例如 `event: line status changed`。服务启动后的第一次拉取只作为基线，不输出事件。

## 开发

提交前需要通过这三项检查：

```bash
cargo fmt -- --check
```

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

```bash
cargo test
```

项目是一个 Cargo workspace，在根目录运行这三条命令会覆盖所有 crate；只想检查某一个 crate 时加上 `-p <crate>`，例如 `cargo test -p dut-core`。

测试完全离线运行：路由测试在 `tests/api/`，用 `wiremock` 模拟港铁和天文台上游，上游样例数据在 `tests/fixtures/`，来自真实响应。每个路由测试都会启动完整的服务，约占十几个文件描述符，测试框架按 CPU 核数并行运行；macOS 终端默认的上限只有 256，所以路由测试启动时会自行把上限提高到系统允许的最大值。

```text
src/                 dut 程序本身：命令行参数、配置、组装依赖、启动服务
crates/
  dut-core/          线路、车站、列车到站等业务类型和用例，不依赖任何框架
  dut-telemetry/     日志输出
  dut-http/          共用的出站 HTTP 客户端
  dut-upstream/      港铁和天文台接口适配、开放数据 CSV 清洗、缓存、启动时的连通性检查
  dut-poll/          后台定时拉取，保存最新数据和数据源健康状态
  dut-monitor/       对比每次拉取的结果，把每个变化作为事件发布
  dut-api/           Axum 路由、响应 DTO、错误映射、请求追踪
tests/api/           路由级测试
tests/fixtures/      上游样例数据
scripts/             开发用脚本，例如同步车站资料的 sync-network.py
```

### 同步编译进服务的车站资料

列车到站依赖编译进服务的线路和车站资料（`crates/dut-core/src/domain/network/`）。港铁开放数据的车站列表与它不一致时（例如新车站开通或改名），服务每天拉取后会输出 warn 日志。这时先 `cargo run` 启动服务，再运行：

```bash
python3 scripts/sync-network.py
```

脚本读取运行中服务的 `/api/data/stations`（清洗后的开放数据）和 `/api/lines`（编译进服务的资料），列出新增、改名、缺失的车站，以及车站有变化的线路和它们在开放数据里的行车路线。加上 `--write` 会重新生成 `station.rs` 里的 `STATIONS` 表，开放数据里没有的车站（例如马场）原样保留。线路的车站顺序、支线和终点需要人工判断，脚本只打印 `codes![...]` 供参考，不会修改 `line.rs`。改完后运行 `cargo fmt && cargo test` 并检查 diff。脚本只用 Python 3 标准库；没有差异时退出码为 0，有差异时为 1。

- [ARCHITECTURE.md](ARCHITECTURE.md)：分层、依赖方向、缓存设计，以及新增功能的步骤
- [AGENTS.md](AGENTS.md)：开发规范，人和编码助手都需要遵守，包括"改接口必须同步更新 HTTP_API.md"
- [HTTP_API.md](HTTP_API.md)：面向客户端的接口文档和变更记录
- [RUST_API.md](RUST_API.md)：写业务逻辑时用到的类型、服务和 trait，例如订阅监控事件的 `Subscriber`、接入新数据源的 `Feed`

## 数据来源

- 列车到站：[港铁 Next Train API](https://rt.data.gov.hk/v1/transport/mtr/getSchedule.php)，由香港政府的资料一线通（DATA.GOV.HK）发布
- 线路状态：[港铁线路状态 JSON](https://tnews.mtr.com.hk/alert/ryg_line_status.json)
- 车站、车费、轻铁、无障碍设施：[港铁开放数据平台](https://opendata.mtr.com.hk/)的 CSV 文件，后台每天拉取一次
- 天气警告：[香港天文台天气警告资料](https://data.weather.gov.hk/weatherAPI/opendata/weather.php?dataType=warningInfo&lang=en)，后台每分钟拉取一次，目前只用于监控事件，没有接口直接返回
