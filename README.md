# ContextLens

跨平台桌面應用（Tauri 2 + React）。反白文字、按熱鍵，在游標旁彈出視窗即時顯示翻譯；開啟教學模式還會說明句子結構、用法與延伸知識。用你自己的 Gemini、ChatGPT 或 Claude API 金鑰。

## 功能（MVP）

- 可自訂、可新增的全域熱鍵，每個熱鍵綁一個動作（翻譯 / 翻譯並教學）
- 翻譯視窗在游標旁串流顯示，Esc 關閉，可固定
- 教學模式：**翻譯 → 結構 → 用法 → 延伸知識**
- Gemini、OpenAI、Anthropic 三選一，模型名稱可自訂
- API 金鑰存在系統鑰匙圈（macOS Keychain、Windows Credential Manager、Linux Secret Service）
- 介面語言與譯文語言預設繁體中文
- 應用內線上更新（簽章驗證）
- 圖片、影片片段擷取：規劃中，見 [docs/ROADMAP.md](docs/ROADMAP.md)

## 開發

需要 Node 22+、Rust stable，以及 [Tauri 的系統依賴](https://v2.tauri.app/start/prerequisites/)。

```bash
npm install
npm run tauri dev      # 開發
npm test               # 前端單元測試
cargo test -p contextlens-core   # 供應商串流、提示詞、設定的測試（不需要 GUI 函式庫）
npm run tauri build    # 打包
```

Linux 另需 `libxdo-dev`（模擬複製按鍵）。

### 架構

| 位置 | 內容 |
| --- | --- |
| `crates/core` | 無 GUI 依賴：設定、提示詞、三家供應商的請求與 SSE 解析 |
| `src-tauri` | 視窗、熱鍵、剪貼簿擷取、鑰匙圈、系統匣、更新外掛 |
| `src/popup` | 翻譯彈出視窗 |
| `src/settings` | 設定視窗（一般、模型與金鑰、熱鍵、更新） |

擷取選取文字的方式：記住剪貼簿 → 模擬複製 → 讀取 → 還原剪貼簿。macOS 第一次使用需要在「系統設定 → 隱私權與安全性 → 輔助使用」允許 ContextLens。Wayland 目前不支援全域熱鍵與按鍵模擬。

## 線上更新

應用內「更新」頁會檢查 GitHub release 的 `latest.json`，下載、驗簽、安裝後重新啟動。

**簽章金鑰**：`tauri.conf.json` 裡的公鑰，對應的私鑰是開發時產生的（沒有密碼）。要讓 `release` 工作流程能簽章，必須把私鑰存成 GitHub Secret：

1. repo 的 Settings → Secrets and variables → Actions → New repository secret
2. 名稱 `TAURI_SIGNING_PRIVATE_KEY`，內容是私鑰檔的完整文字（沒有密碼，所以不需要 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`）
3. **另外備份一份私鑰**：Secret 存進去之後無法再讀回來。遺失私鑰，就無法再發佈能被舊版接受的更新，使用者得重新安裝。
4. 想換成自己的金鑰：`npx tauri signer generate -w ~/.tauri/contextlens.key`，把新公鑰貼到 `plugins.updater.pubkey`，再更新 Secret。換金鑰後，舊版無法自動更新。

發佈：改好 `tauri.conf.json`、`src-tauri/Cargo.toml`、`package.json` 的版本號，然後 `git tag v0.2.0 && git push --tags`，`release` 工作流程會建置四個平台並上傳 `latest.json`。只想先試用、不發佈時，到 Actions 手動執行 `build`，不需要任何 Secret。

更新端點是 `https://github.com/dalof41014/contextlens/releases/latest/download/latest.json`，**repo 必須是公開的**，否則已安裝的 app 無法匿名下載。若要保持私有，把 `latest.json` 與安裝檔放在別處（例如 S3、Cloudflare R2）並改 `endpoints`。
