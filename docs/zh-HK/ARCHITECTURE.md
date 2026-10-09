# 架構

[English](../../ARCHITECTURE.md) · [繁體粵語](ARCHITECTURE.md) · [简体中文](../zh-CN/ARCHITECTURE.md)

Dut 係一個 Cargo workspace，只有一個依賴組裝入口。依賴指向 domain 同 application：HTTP、適配器同背景任務用業務合約，業務程式就保持獨立，唔綁傳輸框架或 I/O。各 crate 嘅 manifest 會限制呢個邊界。

## 目錄

- [進程狀態可以重建](#stateless-first)
- [Crate 同邊界](#crates)
- [請求同背景流程](#flows)
- [啟動時設定](#configuration)
- [快取同新鮮度](#caching-and-freshness)
- [Mock API](#mock-api)
- [監控同投遞](#monitor)
- [新增功能](#adding-a-feature)
- [規劃：推送通知](#planned-push)
- [部署方向](#deployment)

客戶端合約請睇 [HTTP_API.md](HTTP_API.md)；業務程式用嘅型別請睇 [RUST_API.md](RUST_API.md)；設計取捨喺 [ARCHITECTURE.md](ARCHITECTURE.md)，開發規範喺 [AGENTS.md](../../AGENTS.md)。

<a id="stateless-first"></a>

## 進程狀態可以重建

快取、feed 快照、開放數據同監控基線都放喺記憶體。重啟後由上游重建，冇資料庫、本機持久化或者必需嘅 volume。可選日誌檔只係輸出，唔會讀返。如果啟動嗰刻上游故障，相關接口會回傳 `502`，直到第一次取得可用資料。

未來功能先考慮唔使跨重啟保存狀態嘅設計，再用外部服務嘅冪等鍵等機制。真係有具體功能需要先加共用儲存，唔用本機磁碟。早期保存通知歷史嘅構想以呢條規則為準；重啟可以失去冷卻時間同去重視窗。

<a id="crates"></a>

## Crate 同邊界

| Crate | 職責 | Workspace 依賴 |
| --- | --- | --- |
| `dut-core` | 純業務型別、差異比較、應用服務、port 同快照 | 無 |
| `dut-telemetry` | 日誌過濾、輸出、結構化欄位同請求區塊 | 無 |
| `dut-http` | 共用出站 HTTP 同新鮮度解析 | telemetry |
| `dut-upstream` | 港鐵／天文台適配、CSV 清洗、快取同啟動探測 | core, http, telemetry |
| `dut-mock` | Mock API 嘅純模擬資料 | core |
| `dut-poll` | 定時 feed、最新值同來源健康 | core, telemetry |
| `dut-monitor` | Feed 監視同事件投遞 | core, poll |
| `dut-api` | Axum 路由、DTO、中介層、錯誤同 AppState | core, mock, telemetry |
| `dut` | 設定、依賴組裝同進程生命週期 | 所有 crate |

`dut-core` 唔依賴 Axum、Reqwest、Serde、Tokio、資料庫、檔案系統或網絡。工作流程放喺 `application`，業務規則放喺 `domain`。`dut-api` 透過 `AppState` 接收泛型資料來源，唔會匯入上游適配器。正式環境嘅具體依賴喺 `src/bootstrap` 建立；`main.rs` 只啟動 runtime。

模擬時刻表係近似值，所以唔放入面向乘客嘅 core。輪詢同監控分開 crate，方便日後按部署角色分工。目前 bootstrap 會喺同一進程啟動兩者；只跑 API 嘅角色選擇係未來改動，唔係現有命令列開關。

<a id="flows"></a>

## 請求同背景流程

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

每個 `lib.rs` 同 `mod.rs` 都只做索引：文件、模組宣告同重新匯出。實作檔按用途命名。只匯出其他 crate 要用嘅項目，其餘用 private 或 `pub(crate)`；`unreachable_pub` 協助檢查。`dut-core` 公開 domain 同 application，根 library 只公開啟動同路由測試入口。

根目錄嘅 Cargo 命令會覆蓋所有預設 workspace 成員。版本同 lint 由 workspace 繼承。`dist` profile 用 fat LTO、單一 codegen unit、移除符號同 panic abort；部分啟動同 HTTPS 依賴按體積優化，請求處理保持以速度為主嘅預設。`min` profile 沿用呢啲設定，但所有 crate 都按體積優化，用部分速度換取最細嘅程式。

<a id="configuration"></a>

## 啟動時設定

發佈預設值定義上游 URL、逾時、快取策略同輪詢排程。`CommandLine` 用 clap 喺日誌啟動之前解析每次執行嘅選項，轉成有型別嘅值。每個選項都有長參數同環境變數，參數優先；除咗慣用嘅 `RUST_LOG`，變數都用 `DUT_`。

相關選項放喺 `clap::Args` struct，再 flatten 入 `CommandLine`。解析時驗證格式，互相依賴或衝突用 `requires`／`conflicts_with`。格式錯誤或空值退出碼係 2。Crate 只接收設定值，唔自己讀參數或環境。測試解析時排除選項變數，唔修改進程環境。

日後加入憑證時，用環境變數傳入，help 隱藏值，`Debug` 要遮罩，敏感 header 要標記。唔可以記錄秘密。目前冇推送憑證選項；詳見下面嘅規劃。

<a id="caching-and-freshness"></a>

## 快取同新鮮度

快取絕對資料，喺邊界計相對值。保存 `arrival_at`，丟棄上游倒數。`BoardView` 每次讀取都會濾走到站時間已過超過 30 秒嘅車（`DEPARTED_GRACE`）。不可變資料用 `Arc` 共用，DTO 盡量借用。

Next Train 用 `dut-upstream` 私有嘅泛型 `RefreshingCache<K, V>`。Key 係經內置路網驗證嘅路綫／車站組合，客戶端輸入唔會令快取無限增長。並行讀取共用一次刷新。新鮮期跟上游 `max-age - Age`，限制喺 2–15 秒，預設 10 秒。唔啟用 stale-while-revalidate；stale-if-error 係過期後 90 秒，失敗後退避 5 秒。

到站板先取英文，站名用內置路網；只有特別安排通告先額外取繁體中文。所有出站請求都經共用 `OutboundHttpClient::fetch`，用結構化日誌記開始、完成同失敗。快取日誌涵蓋 miss、過期、刷新、合併等候、舊資料頂替、背景重新驗證同退避。

<a id="polled-feeds"></a>

### 輪詢 Feed

| Feed | 間隔／首次輪詢 | 新鮮期／舊資料視窗 | 失明門檻 |
| --- | --- | --- | --- |
| 路綫狀態 | 30 s / 0 s | 33 s / 15 min | 2 min |
| 天文台警告 | 60 s / 0 s | 65 s / 15 min | 5 min |
| 抽樣列車訊號 | 60 s / 60 s | 63 s / 15 min | 5 min |
| 港鐵開放數據 | 24 h / 0 s | 86430 s / 30 d | 48 h |

`dut_poll::spawn` 每個 feed 跑一個 Tokio task。`MissedTickBehavior::Delay` 避免慢輪詢之後集中補請求。開放數據 5 分鐘後重試，其餘維持正常間隔。首次輪詢未完成時，讀取會喺排程嘅啟動等候期限內等候。失敗會保留上次成功值；新鮮期同舊資料視窗用盡就不可用。來源健康分 `Starting`、`Healthy`、`Failing` 同 `Blind`。

路綫狀態唔用上游五秒 TTL：發佈時間只喺狀態變化時更新，唔係心跳。HTTP 新鮮度由輪詢排程決定。新鮮回應用 `public, max-age=N`，舊資料用 `no-cache`；支援嘅回應按協商用 gzip。

<a id="open-data"></a>

### 開放數據共用一份快照

`MtrOpenDataFeed` 用可重用連線順序讀七個 CSV。全部下載同清洗成功先發佈，否則保留上一份完整資料。適配器轉換車站 ID、將車費解析成整數港仙、修正已知文字問題、記錄未能對應而略過嘅行，並拒絕格式錯誤嘅值。原始檔保留原本位元組。

清洗結果同原始位元組各自有穩定版本。數據集 ETag 另加服務版本，原始檔就唔加。`/api/data` 列出版本，數據集同檔案支援 `If-None-Match`。每份抓取狀態嘅 JSON／CSV 同最高級別 gzip 喺 blocking pool 編碼一次，再共用。編碼 key 包括 `fetched_at` 同 `stale`，因為呢啲欄位可以喺版本不變時改變。回應附有 `Content-Length` 同 `Vary: Accept-Encoding`。

Next Train 仍然以內置路網為準。輪詢會報公開路網差異，`scripts/sync-network.py` 產生改動供審核。啟動探測另外檢查實際適配器 URL 同文件格式；探測失敗只報問題，唔會停止監聽。

<a id="mock-api"></a>

## Mock API

要先啟用嘅 `/api/mock` 路由共用正式 DTO 同應用層驗證。`dut-mock` 提供純模擬來源，冇輪詢、快取或 I/O。場景同 seed 決定各個可重現選擇；絕對時間推進時刻表，唔使保存請求狀態。月台同中途折返參考實際抓取資料，班距仍然係近似值。每次請求每綫每方向最多模擬 64 班車。

<a id="monitor"></a>

## 監控同投遞

監控用 domain 嘅 `changes_since` 比較連續成功值，發佈每個觀察到嘅變化：路綫狀態或訊息、天文台警告生命週期、抽樣到站板延誤／通告，以及來源健康。事件講事實；嚴重程度、門檻、跨來源關聯同通知策略由訂閱者決定。輪詢失敗唔代表服務恢復或者警告取消。

首次成功資料建立基線，唔發資料變化事件；健康狀態轉變仍可能發佈。Bootstrap 即刻掛上 `Subscriber`，每個喺自己 task 順序處理事件，用可變狀態而唔使鎖。Broadcast 保留 256 個事件，唔重播歷史。落後嘅訂閱者會收到 `on_lagged(missed)`；漏事件有影響就要重新同步。事件日誌喺 watcher 開始前已經掛好。

Next Train 監控每分鐘抽查每條綫一個中段車站，經乘客共用嘅快取服務；未計額外本地化請求，每分鐘最多增加十次到站板抓取。呢個唔係全網事故偵測。運輸署自由文字交通消息仍待分類方案，暫未接入。

<a id="adding-a-feature"></a>

## 新增功能

1. 喺 `dut-core/domain` 加業務概念同有測試嘅規則。
2. 工作流程放喺 `application`；具體用例需要外部資料先加 port。
3. 喺 `dut-upstream` 實作適配器，使用共用 HTTP client。按請求抓取嘅到站板用 key 快取，整份文件定時讀就用 `Feed`。
4. 喺 bootstrap 組裝具體依賴，透過 `AppState` 傳服務。
5. 喺 `dut-api` 加 DTO 同薄 handler；回應改動要同步 Mock。
6. 用離線樣本測試業務決策，以及路由狀態、content type 同 body。
7. 更新 `HTTP_API.md` 合約同變更記錄、`RUST_API.md` 業務接口，以及全部語言版本。跑 [AGENTS.md](../../AGENTS.md) 嘅三項必要檢查。

<a id="planned-push"></a>

## 規劃：推送通知

`dut-push` 仲未存在。預計會依賴 core、http 同 telemetry，實作 `Subscriber`，用供應商適配器轉換業務通知。目前候選係 OneSignal；日後有 APNs 或 FCM 第二個供應商先抽象接口。聊天頻道唔喺呢個規劃範圍。

策略保持純函式，門檻、冷卻同去重要獨立於發送測試。通知用業務受眾、`Localized` 標題同內容，以及由事件產生嘅 collapse／冪等鍵。供應商 tag 會按路綫代碼定受眾，唔保存用戶資料；發佈前同 App 定好 tag 名。依靠冪等鍵處理重啟或部署重疊之前，要確認供應商語義同保留時間。

擬議選項係 `--onesignal-app-id`／`DUT_ONESIGNAL_APP_ID` 同 `--onesignal-api-key`／`DUT_ONESIGNAL_API_KEY`。兩個都冇就關推送，兩個非空就開，缺一或者空值就啟動失敗。呢啲只係設計記錄，現有 CLI 唔接受。

<a id="deployment"></a>

## 部署方向

目前服務係一個程式，用進程內 channel 連接。多個實例各自重建快取同輪詢，上游負載隨實例數增加，監控亦會重複發事件。擴展之前，要加設定將 monitor 同 push 限定喺一個角色，並用冪等機制處理替換重疊。有跨進程消費者先將事件搬去 Redis 或 NATS；可序列化事件 DTO 要放喺 `dut-core` 以外。
