# Token Monitor RS

> [!IMPORTANT]
> **致謝與專案聲明（Attribution & Acknowledgments）**
>
> 本專案是**基於原作者 [Javis (Javis603)](https://github.com/Javis603) 所開發的 [`Javis603/token-monitor`](https://github.com/Javis603/token-monitor) 原始架構進行輕量化改良的 Rust 桌面外殼與資源守護程式**（底層日誌解析亦依賴上游 [`junhoyeo/tokscale`](https://github.com/junhoyeo/tokscale)）。
>
> **特別感謝原作者 Javis 無私開源並持續高頻維護這麼強大、本地優先（Local-First）的 AI Token 與額度監測工具！**
> 本專案沒有重新發明輪子，而是保留了原作者精準的採集核心與各家 Provider 額度探測能力，僅針對 **Windows 遊戲電腦的記憶體、硬碟 I/O 與視覺簡潔度** 做輕量化包裝。
>
> 👉 如果你需要完整支援 54+ 種工具、macOS 瀏海互動面板、iCloud / 多設備 Hub 同步、年度熱力圖或外掛系統，請務必支持並使用原作者的正式版專案：**[https://github.com/Javis603/token-monitor](https://github.com/Javis603/token-monitor)**

---

## 為什麼會有這個改良版？

原版 `Javis603/token-monitor` 功能極其完整，但在 **Windows 遊戲主機** 上長期常駐時，容易遇到以下三個痛點：
1. **Electron 常駐記憶體與 GPU 合成開銷**：Chromium 與多視窗/懸浮視窗在背景或全螢幕遊戲時，容易佔用數百 MB 至 1 GB+ RAM，置頂視窗也可能影響 Windows DWM 全螢幕遊戲優化。
2. **即時檔案監聽（`fs.watch`）與頻繁掃描的硬碟 I/O**：當本地 AI 對話紀錄累積龐大後，即時掃描與拉起子進程容易在遊戲過程中造成硬碟突然滿載或幀率波動（1% Low FPS 掉幀）。
3. **畫面資訊量過大**：同時呈現數十種冷門工具與複雜圖表，日常只用主力幾款 AI 工具時顯得較為雜亂。

因此 **Token Monitor RS** 採用 **「100% 原生 Rust 常駐系統匣 ＋ 單次無頭呼叫原作者採集器」** 的混合架構：
- 平時常駐的是輕量原生 Rust (`eframe`/`egui` + Windows System Tray)，關閉視窗即以 Win32 `SW_HIDE` 徹底隱藏視窗停止重繪。
- 只在定時檢查、且目標工具日誌的 **`mtime`/大小確實有變動**、同時**沒有在玩遊戲**時，才在背景靜默觸發一次原作者採集橋接（`collect-filtered.cjs`），跑完立刻 `process.exit(0)` 釋放所有 Node.js / V8 記憶體。
- 內建**免 AI 全自動更新引擎**：可直接從 GitHub 同步原作者 [`Javis603/token-monitor`](https://github.com/Javis603/token-monitor) 的最新 commit、相依套件與 `tokscale.exe`，確保各家 CLI / Antigravity RPC / 額度介面改版時能自動跟上。

---

## 原版 vs. Rust 改良版：優缺點完整比較

| 比較項目 | 原版 [`Javis603/token-monitor`](https://github.com/Javis603/token-monitor) (Electron) | 本改良版 `token-monitor-rs` (Rust + 原作橋接核心) |
| :--- | :--- | :--- |
| **背景常駐架構** | Electron（Chromium 渲染層 + Node.js 主程序 + Watcher 子進程） | 原生 Rust 單一執行檔（`~3.86 MB`），Node.js 僅在刷新瞬間啟動並立即退出 |
| **打遊戲時的干擾** | 若未手動關閉，背景掃描或懸浮視窗可能搶佔硬碟 I/O 與 GPU 合成 | **內建遊戲模式自動 0 I/O 休眠**：透過 Win32 `SHQueryUserNotificationState` 偵測全螢幕遊戲與自訂遊戲 `.exe`，遊戲中完全凍結掃描 |
| **硬碟 I/O 策略** | 支援 3–5 秒即時 `fs.watch` 監聽與定期掃描 | **檔案 `mtime` 指紋門檻**：僅檢查少數主力工具目錄的中繼資料，日誌無變動直接跳過掃描（0 硬碟讀取） |
| **支援的工具數量** | **極豐富（54+ 種 AI 工具）** | **聚焦主力（7 大目標工具過濾）**：`OpenCode`、`Claude Code`、`Antigravity`、`Codex (GPT)`、`LM Studio`、`Strata`、`llama.cpp`（亦可自動隨上游啟用） |
| **視覺與介面風格** | 功能齊全：懸浮球、邊緣停靠欄、毛玻璃（Mica/Acrylic）、熱力圖、Projects | **極簡去雜亂**：4 個核心 KPI 卡片、即時額度進度條、7 大工具與模型明細、可收合近期對話 |
| **同步與生態功能** | **完整支援**：多設備 Self-hosted Hub / Cloudflare Worker、iCloud、WSL、外掛系統、Discord RPC | **已移除**（專注於單機 Windows 本地監看與最低資源消耗） |
| **跨平台支援** | Windows / macOS（含 Notch 瀏海與桌面 Widget）/ Linux | **僅限 Windows**（使用 Win32 API 實作系統匣隱藏與全螢幕遊戲偵測） |
| **跟隨原作者更新** | 官方內建更新器 / `git pull` | **內建 4 階段自動同步引擎**：定時/手動自動從 `Javis603/token-monitor` 拉取更新、自動 `npm ci` 與升級 `tokscale.exe` |

### 總結：優點與缺點取捨

#### ✅ 改良版優點（Pros）
1. **徹底解決遊戲卡頓與硬碟狂讀**：透過 `mtime` 門檻 + Win32 全螢幕 Direct3D 遊戲偵測 + 遊戲 `.exe` 進程偵測，開遊戲自動 0 I/O 休眠；縮小至系統匣時完全隱藏視窗、不吃 GPU。
2. **Node.js 零常駐記憶體**：只在需要更新的那幾秒鐘執行單次無頭採集，寫入 JSON 快照後立即結束進程釋放記憶體。
3. **看板清爽直覺**：去掉不常用的功能與多餘工具，一打開就能看懂今日/近7日/本月/總計花費、各家 5 小時與每週剩餘額度、各工具模型分佈。
4. **獨立打包 `.exe` 加上自動跟隨原作者更新**：`TokenMonitorRS.exe` 內嵌橋接腳本，且內建每 6 / 12 / 24 小時自動同步 `Javis603/token-monitor` 的機制，不怕上游 CLI/RPC 改版失效。

#### ⚠️ 改良版缺點與限制（Cons / Trade-offs）
1. **不支援多設備同步與擴充生態**：移除自建 Hub、Cloudflare Worker、iCloud 同步、WSL 跨系統掃描、外掛（Plugins）與 Discord RPC。
2. **無進階視覺特效與跨平台**：沒有原版漂亮的毛玻璃特效、懸浮球、螢幕邊緣 Dock 與 macOS 專屬功能，僅支援 Windows。
3. **執行環境需求**：雖然主程式只有一個 `TokenMonitorRS.exe`（約 `3.86 MB`），但背景單次採集與同步原作者更新時，電腦上仍需要安裝 **Node.js (`>= 22.15.0`)** 與 **Git**。
4. **關於 `Strata` 與 `llama.cpp`**：上游目前尚未內建這兩個獨立 client ID；本版會先嘗試從 session 的 provider/model 中自動比對歸類（如透過 LM Studio 執行的 `llama` 模型），若無紀錄則顯示為待命槽，未來只要原作者上游加入支援即可自動接軌。

---

## 快速開始（Quick Start）

### 系統需求
- **作業系統**：Windows 10 / 11 (x64)
- **執行期依賴**（供背景單次採集與自動同步原作者倉庫使用）：
  - [Node.js](https://nodejs.org/) `v22.15.0` 或更新版本
  - [Git for Windows](https://git-scm.com/)

### 使用方式
1. 從 **Releases** 下載 `TokenMonitorRS.exe`（或完整壓縮包 `TokenMonitorRS-v0.1.0-win64.zip`）放到任意獨立資料夾。
2. 直接雙擊執行 **`TokenMonitorRS.exe`**：
   - 首次啟動會在同目錄自動解壓橋接腳本 `bridge/collect-filtered.cjs`。
   - 自動從 `https://github.com/Javis603/token-monitor.git` 初始化並安裝 `upstream/` 採集核心。
   - 點擊右上角 **`×`** 或 **`縮到系統匣`** 即常駐於右下角托盤；雙擊托盤圖示可隨時喚醒看板。

---

## 自行編譯（Build from Source）

```powershell
# 確認已安裝 Rust stable 工具鏈
cargo test
cargo build --release
```

---

## 授權與版權聲明（License & Credits）

- 本專案採用 [MIT License](LICENSE) 開源釋出。
- 核心採集邏輯、Provider 額度探測規範與上游架構版權歸原作者 **[Javis (Javis603)](https://github.com/Javis603)** 所有：
  - 原專案儲存庫：**[https://github.com/Javis603/token-monitor](https://github.com/Javis603/token-monitor)**
- 底層 Rust Token 解析引擎來自：**[junhoyeo/tokscale](https://github.com/junhoyeo/tokscale)**
