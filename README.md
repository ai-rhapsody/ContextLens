# ContextLens

跨平台桌面應用（Tauri 2 + React）。反白文字、按熱鍵，在游標旁彈出視窗即時顯示翻譯；開啟教學模式還會說明句子結構、用法與延伸知識。用你自己的 Gemini、ChatGPT 或 Claude API 金鑰。

[![Ko-fi](https://img.shields.io/badge/Ko--fi-%E8%AB%8B%E6%88%91%E5%96%9D%E6%9D%AF%E5%92%96%E5%95%A1-ff5e5b?logo=kofi&logoColor=white)](https://ko-fi.com/airhapsody)

## 下載

到 [Releases](https://github.com/ai-rhapsody/ContextLens/releases/latest) 下載：

- **Windows**：`ContextLens_<版本>_x64-setup.exe`
- **Linux**：`.AppImage`（可自動更新）、`.deb` 或 `.rpm`
- **macOS**：暫時沒有正式版。可以用 Actions 的 `build` 工作流程自己建置沒有簽章的 `.dmg`；第一次打開若顯示「已損毀」，在終端機執行 `xattr -cr /Applications/ContextLens.app`

安裝後在「更新」頁可以線上更新。資料怎麼處理，請看[隱私政策](PRIVACY.md)。

## 功能

- **熱鍵**：可新增多組，每組選「來源」和「動作」
  - 來源：**截圖**（按下後直接框選區域、點選視窗或全螢幕，不必先另外截圖）、選取的文字、剪貼簿圖片、Live 開／關
  - 動作：**翻譯**（可附教學）、**解釋**（圖表、示意圖、迷因、程式碼、題目）、**搜尋**（辨識並上網查，附來源與搜尋關鍵字）
  - **自訂動作**：寫你自己的指令，例如「把內容潤飾成正式的英文 email」「摘要成三點」，在翻譯視窗裡會多一個分頁
- **詞彙表**：固定歌名、人名、作品名的譯法，翻譯、解釋、搜尋和 Live 都會照著寫
- **翻譯視窗**：在選取的文字下方串流顯示，不會蓋住要讀的段落；頂部可切換動作重新分析。剛跳出時點外面就關，操作過就保留；可調整大小、固定位置
- **圖片、音訊、影片**：拖放、選檔或 `Ctrl+V` 貼上
- **Live 即時翻譯**：畫面底部的字幕列
  - 只聽聲音、沒開註解時，Gemini 會用專門口譯的 `gemini-3.5-live-translate-preview`，邊講邊翻；開註解或同時看螢幕時用即時模型
  - 聽麥克風、系統聲音（目前限 Windows）需要 Gemini 或 ChatGPT 的即時模型
  - 只看螢幕時，任何能讀圖的模型都可以（包含 Claude），畫面停住才送出
  - 服務可自動選或手動指定
  - 按「CC」進入字幕模式：背景透明、一次只顯示一句、滑鼠移開按鈕就隱藏，可以直接疊在影片上當字幕；「Aa」可以調整字幕大小、顏色、背景透明度，或關掉原文只看譯文
  - 字幕可以匯出成文字檔或 SRT 字幕檔
- **模型能力對照**：依服務商與模型名稱判斷能做什麼（圖片、音訊、影片、Live、上網），做不到時一鍵改用其他有金鑰的服務
- **歷史紀錄**：存在本機，可搜尋、刪除，可關閉；搜尋結果保留來源連結
- **單字本**：按 ☆ 收藏句子，匯出成 Excel 表格或 Anki 單字卡
- **伺服器忙碌時自動重試**一次，錯誤訊息附「重試」按鈕
- **診斷資訊**：「更新」頁可以複製版本、設定和本機記錄，回報問題時附上（記錄只存在你電腦上，不含翻譯內容和金鑰）
- **介面語言**：繁體中文、English、日本語、한국어、Español，預設跟著系統語言，可以在「一般」頁切換
- **思考中動畫**、應用內線上更新（簽章驗證）

| 服務 | 文字 | 圖片 | 音訊 | 影片 | Live 聽聲音 | 上網搜尋 |
| --- | --- | --- | --- | --- | --- | --- |
| Gemini | ✓ | ✓ | ✓ | ✓ | ✓ Live API | ✓ Google Search |
| ChatGPT（OpenAI） | ✓ | ✓ | | | ✓ Realtime API | 只有 search 模型 |
| Claude（Anthropic） | ✓ | ✓ | | | | ✓ |

實際能力依模型名稱判斷，規則在 `crates/core/src/caps.rs`。模型用下拉選單挑選（清單在 `src/lib/models.ts`，2026 年 10 月整理），也可以選「自訂…」輸入任何名稱。

預設模型：Gemini `gemini-3.5-flash-lite`（即時 `gemini-3.8-live`）、ChatGPT `gpt-6-luna`（即時 `gpt-realtime-2.1-mini`）、Claude `claude-opus-5-5`。舊版的預設值會自動升級，自己選過的模型不變。

## 支持這個專案

ContextLens 免費、開源，沒有廣告也不收集資料。如果它幫上你的忙，歡迎到 [Ko-fi](https://ko-fi.com/airhapsody) 贊助開發，不用註冊帳號。

回報問題、提出建議也很有幫助，請到 [Issues](https://github.com/ai-rhapsody/ContextLens/issues)。

## 開發

需要 Node 22+、Rust stable，以及 [Tauri 的系統依賴](https://v2.tauri.app/start/prerequisites/)。

```bash
npm install
npm run tauri dev      # 開發
npm test               # 前端單元測試
cargo test -p contextlens-core   # 供應商串流、提示詞、設定的測試（不需要 GUI 函式庫）
npm run tauri build    # 打包
```

Linux 另需 `libxdo-dev`（模擬複製按鍵）、`libpipewire-0.3-dev`、`libclang-dev`（螢幕擷取）、`libasound2-dev`（Live 錄音）。

### 架構

| 位置 | 內容 |
| --- | --- |
| `crates/core` | 無 GUI 依賴：設定、提示詞、三家供應商的請求與 SSE 解析 |
| `src-tauri` | 視窗、熱鍵、剪貼簿擷取、鑰匙圈、系統匣、更新外掛 |
| `src/popup` | 翻譯彈出視窗 |
| `src/settings` | 設定視窗（一般、模型與金鑰、熱鍵、更新） |

擷取選取文字的方式：記住剪貼簿 → 模擬複製 → 讀取 → 還原剪貼簿（Linux 先讀 PRIMARY 選取）。Windows 上會透過 UI Automation 取得選取範圍的位置，讓翻譯視窗開在選取範圍下方。

macOS 第一次使用需要在「系統設定 → 隱私權與安全性」允許 ContextLens 的「輔助使用」與「螢幕錄製」。Wayland 不允許程式註冊全域熱鍵，「熱鍵」頁會列出 `--hotkey N` 指令，綁到桌面的自訂快捷鍵即可。

## 線上更新

應用內「更新」頁會檢查 GitHub release 的 `latest.json`，下載、驗簽、安裝後重新啟動。

**簽章金鑰**：`tauri.conf.json` 裡的公鑰，對應的私鑰是開發時產生的（沒有密碼）。要讓 `release` 工作流程能簽章，必須把私鑰存成 GitHub Secret：

1. repo 的 Settings → Secrets and variables → Actions → New repository secret
2. 名稱 `TAURI_SIGNING_PRIVATE_KEY`，內容是私鑰檔的完整文字（沒有密碼，所以不需要 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`）
3. **另外備份一份私鑰**：Secret 存進去之後無法再讀回來。遺失私鑰，就無法再發佈能被舊版接受的更新，使用者得重新安裝。
4. 想換成自己的金鑰：`npx tauri signer generate -w ~/.tauri/contextlens.key`，把新公鑰貼到 `plugins.updater.pubkey`，再更新 Secret。換金鑰後，舊版無法自動更新。

**發佈**：到 Actions → `release` → Run workflow。版本留空會自動以最新的 tag 加一號（v0.1.11 → 0.1.12），也可以自己填，例如 0.2.0。預設建置 Windows 和 Linux，選 `windows` 只建 Windows。tag `v<版本>` 會自動建立。推 tag 也會觸發發佈。只想先試用、不發佈時，執行 `build`，不需要任何 Secret。

**macOS 簽章與公證**（選用，等 macOS 回到正式版時用）：在 repo 的 Actions Secrets 加上：
- `APPLE_CERTIFICATE`：Developer ID Application 憑證匯出的 .p12，轉成 base64
- `APPLE_CERTIFICATE_PASSWORD`
- `APPLE_SIGNING_IDENTITY`，例如 `Developer ID Application: 名字 (TEAMID)`
- `APPLE_ID`
- `APPLE_PASSWORD`：App 專用密碼
- `APPLE_TEAM_ID`

沒有設定時會改用 ad-hoc 簽章，並在 log 提醒。

更新端點是 `https://github.com/ai-rhapsody/ContextLens/releases/latest/download/latest.json`，**repo 必須是公開的**，否則已安裝的 app 無法匿名下載。若要保持私有，把 `latest.json` 與安裝檔放在別處（例如 S3、Cloudflare R2）並改 `endpoints`。

## 隱私

ContextLens 沒有自己的伺服器，也不收集使用統計。你選取的文字、截圖、檔案和 Live 的聲音與畫面，只在你操作時直接送到你自己設定的 AI 服務。詳見[隱私政策](PRIVACY.md)。

## Code signing policy

Windows 安裝檔的簽章計畫由 [SignPath.io](https://about.signpath.io/) 免費提供，憑證由 [SignPath Foundation](https://signpath.org/) 核發（申請通過後生效）。

Free code signing provided by [SignPath.io](https://about.signpath.io/), certificate by [SignPath Foundation](https://signpath.org/).

- **Committers and reviewers:** [ai-rhapsody](https://github.com/ai-rhapsody)
- **Approvers:** [ai-rhapsody](https://github.com/ai-rhapsody)

只有從本 repo 原始碼、經 GitHub Actions 的 `release` 工作流程建置出來的檔案才會簽章；每次發版都由 Approver 手動核准。macOS 版回到正式版時，會由開發者自己的 Apple Developer ID 憑證簽章，並經 Apple 公證（notarization）。

Privacy: this program will not transfer any information to other networked systems unless specifically requested by the user or the person installing or operating it. See the [privacy policy](PRIVACY.md).

## 授權

[GPL-3.0-or-later](LICENSE)。可以自由使用、修改和分享；把修改過的版本發布出去時，必須同樣以 GPL 授權公開完整原始碼。
