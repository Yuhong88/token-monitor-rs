# Token Monitor RS v0.1.0 (Windows x64)

> 🙏 **致謝原作者**：本專案基於 **[Javis603/token-monitor](https://github.com/Javis603/token-monitor)** 架構進行輕量化改良，衷心感謝原作者 **Javis (Javis603)** 開發並維護這麼優秀的開源 AI Token 監測工具！（底層解析亦依賴 [junhoyeo/tokscale](https://github.com/junhoyeo/tokscale)）

---

## 版本亮點（What's Included）

- **單一輕量執行檔 `TokenMonitorRS.exe`（約 `3.86 MB`）**：原生 Rust (`eframe`/`egui` + Windows System Tray)，內嵌過濾橋接腳本，關閉視窗自動隱藏到系統匣（Win32 `SW_HIDE`）。
- **內建「遊戲模式」自動 0 I/O 休眠**：透過 Win32 `SHQueryUserNotificationState` 偵測全螢幕遊戲與自訂 `.exe` 名單，開遊戲自動凍結背景掃描。
- **`mtime` 增量指紋門檻 + 單次無頭採集**：日誌沒變動就跳過硬碟掃描；只有需要刷新時才靜默執行一次原作者採集器並立刻退出 Node.js，不常駐記憶體。
- **清爽版看板介面**：聚焦 4 個核心 KPI、即時額度與重置倒數、7 大主力工具（`OpenCode`、`Claude Code`、`Antigravity`、`Codex (GPT)`、`LM Studio`、`Strata`、`llama.cpp`）及近期對話。
- **內建免 AI 自動更新引擎**：可設定每 6 / 12 / 24 小時自動從 `Javis603/token-monitor` 拉取最新 commit，自動更新 `npm` 套件與 `tokscale.exe`。

## 與原版的優缺點比較

- **優點**：開遊戲 0 硬碟讀寫與 0 GPU 干擾、無 Electron/Node 常駐記憶體負擔、介面極簡清爽、可免手動跟隨原作者上游更新。
- **缺點與取捨**：僅支援 Windows；移除原版的多機 Hub / Cloudflare Worker / iCloud 同步、WSL 跨系統掃描、macOS 瀏海與小工具、毛玻璃特效、懸浮球、Edge Dock 與外掛系統；背景刷新與自動更新需本機已安裝 Node.js (`>= 22.15.0`) 與 Git。

## 附件檔案
- `TokenMonitorRS.exe` — 獨立執行檔（可直接放進任意空資料夾雙擊使用，首次啟動會自動拉取上游核心）
- `TokenMonitorRS-v0.1.0-win64.zip` — 包含執行檔與啟動/同步批次檔的完整發布包
