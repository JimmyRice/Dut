# Dut（嘟）

Dut 是一个用 Rust 编写的港铁（MTR）实时数据 API，也是 MTRGo App 的数据整合后端。它把港铁的开放数据整理成方便 App 直接使用的 JSON：线路和车站资料、全线路服务状态，以及每个车站接下来几班列车的到站时间。

- **按乘客的视角组织数据**：每个行车方向都附带月台指示牌上的终点站（例如"往 寶琳／康城"），站名同时提供中英文。列车到站只给绝对时间，倒计时由 App 在本地计算。
- **不给上游添负担**：上游数据缓存在进程内，同一份数据同一时间只会向港铁发一个请求。上游出故障时，先返回标记为 `stale` 的旧数据，不直接报错。
- **方便排查问题**：每个响应都带 `x-request-id`，服务端日志按请求分组，同一个请求的日志排在一起。

## 接口

| 方法 | URL | 用途 |
|---|---|---|
| `GET` | `/api/lines` | 所有线路、车站、中英文站名、行车方向和线路颜色 |
| `GET` | `/api/lines/status` | 全线路服务状态（正常、延误、受阻等） |
| `GET` | `/api/lines/{line}/stations/{station}/next-trains` | 某条线在某个车站的下几班列车 |
| `GET` | `/api/stations/{station}/next-trains` | 途经某个车站的所有线路的下几班列车，适合换乘站 |

参数、字段说明、缓存行为和错误码见 [api.md](api.md)。

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
| 线路状态 | 30 秒，过期后在后台刷新 | 15 分钟内的旧数据仍会返回，标记为 `stale` |
| 线路与车站资料 | 编译在服务里，随部署更新 | 不访问上游 |

响应头 `Cache-Control: public, max-age=N` 表示数据还有多少秒算新鲜，App 可以直接用它作为下次轮询的间隔。

缓存只存在于单个进程里。同时运行多个实例时，发往港铁的请求量会按实例数成倍增加。完整的缓存策略见 [ARCHITECTURE.md](ARCHITECTURE.md#caching-and-freshness)。

## 配置

| 环境变量 | 默认值 | 作用 |
|---|---|---|
| `DUT_BIND_ADDRESS` | `127.0.0.1:3000` | 监听的 IP 和端口，例如 `0.0.0.0:3000` 或 `[::]:3000`。只能写 IP，不能写主机名 |

港铁接口地址、超时和缓存策略都是 [src/bootstrap/config.rs](src/bootstrap/config.rs) 里的默认值，修改后重新编译即可。

服务收到 `SIGTERM` 或 Ctrl-C 后不再接受新请求，等正在处理的请求完成后退出。

## Docker

镜像分两阶段构建：先在 Alpine 里编译出完全静态链接的 musl 程序，再放进只有 CA 证书和非 root 用户的 [distroless static](https://github.com/GoogleContainerTools/distroless) 基础镜像。整个镜像约 10 MB，里面没有 shell。

```bash
docker build -t dut .
```

```bash
docker run --rm -p 3000:3000 dut
```

镜像里已经设置了 `DUT_BIND_ADDRESS=0.0.0.0:3000`。编译阶段始终在本机架构上运行，用 [xx](https://github.com/tonistiigi/xx) 交叉编译，所以在 Apple Silicon 上构建 amd64 镜像也不需要模拟 CPU：

```bash
docker buildx build --platform linux/amd64,linux/arm64 -t dut .
```

镜像里没有 `curl`，所以没有 `HEALTHCHECK`。要做健康检查，可以让编排系统直接请求 `/api/lines`，这个接口不访问上游。

构建好的镜像发布在 GitHub Container Registry，同时提供 amd64 和 arm64 版本。仓库是私有的，拉取前需要先用有 `read:packages` 权限的 token 执行 `docker login ghcr.io`：

```bash
docker run --rm -p 3000:3000 ghcr.io/jimmyrice/dut:latest
```

## 发布

先把 `Cargo.toml` 里的 `version` 改成新版本并提交，再推送同名 tag：

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

在 Actions 页面手动运行 Release 工作流，会编译同样的 6 个文件，但只保存为工作流的 artifact，不创建 release，适合在打 tag 前检查工作流的改动。

仓库公开后，二进制文件和镜像会附带 GitHub 签名的构建来源证明（artifact attestation），可以这样验证：

```bash
gh attestation verify dut-x86_64-unknown-linux-musl.tar.gz -R JimmyRice/Dut
```

## 日志

- **日志级别**：用 `RUST_LOG` 设置，默认值是 `info,dut=debug,tower_http=debug`，会输出缓存命中等调试信息。想安静一些可以用 `RUST_LOG=info cargo run`。
- **在终端里运行时**：同一个请求的所有日志合成一块，开头是一行摘要，并带颜色。设置 `NO_COLOR=1` 可以关闭颜色。
- **输出到文件或管道时**：每条日志一行，不带颜色，方便 `grep` 和日志收集工具处理。

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

测试完全离线运行：路由测试在 `tests/api/`，用 `wiremock` 模拟港铁上游，上游样例数据在 `tests/fixtures/`，来自真实响应。

```text
src/
  domain/          线路、车站、列车到站等业务类型，不依赖任何框架
  application/     用例：组合数据源、隐藏已开出的列车
  infrastructure/  出站 HTTP 客户端、缓存、港铁接口适配
  api/             Axum 路由、响应 DTO、错误映射、请求追踪
  bootstrap/       配置、组装依赖、启动服务
  telemetry/       日志输出
tests/api/         路由级测试
```

- [ARCHITECTURE.md](ARCHITECTURE.md)：分层、依赖方向、缓存设计，以及新增功能的步骤
- [AGENTS.md](AGENTS.md)：开发规范，人和编码助手都需要遵守，包括"改接口必须同步更新 api.md"
- [api.md](api.md)：面向客户端的接口文档和变更记录

## 数据来源

- 列车到站：[港铁 Next Train API](https://rt.data.gov.hk/v1/transport/mtr/getSchedule.php)，由香港政府的资料一线通（DATA.GOV.HK）发布
- 线路状态：[港铁线路状态 JSON](https://tnews.mtr.com.hk/alert/ryg_line_status.json)
