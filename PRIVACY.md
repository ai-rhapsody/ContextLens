# ContextLens 隱私政策

最後更新：2026 年 10 月 28 日

ContextLens 是在你電腦上執行的桌面程式。它沒有自己的伺服器、不需要帳號，也不收集使用統計或當機報告。開發者收不到你的任何資料。

## 存在你電腦上的資料

| 資料 | 位置 |
| --- | --- |
| API 金鑰 | 作業系統的鑰匙圈（Windows 認證管理員、macOS 鑰匙圈、Linux Secret Service），不會寫進設定檔，也不會傳給程式的畫面 |
| 設定 | 使用者設定資料夾裡的 `settings.json` |
| 歷史紀錄（可關閉） | 使用者資料資料夾裡的 `history.json`，最多 1000 筆，包含原文、結果、圖片縮圖和追問內容 |

在「一般」頁可以關閉歷史紀錄，在「歷史紀錄」頁可以刪除單筆或全部。解除安裝並刪除上述資料夾，就會移除所有資料；金鑰可以先在「模型與金鑰」頁移除。

讀取選取的文字時，ContextLens 會暫時使用剪貼簿，讀完立刻還原原本的內容。

## 傳到外部的資料

只有在你按下熱鍵、點按鈕或開啟 Live 時，ContextLens 才會把內容送到**你自己設定的 AI 服務**，並使用**你自己的 API 金鑰**：

- **翻譯、解釋、搜尋**：選取的文字、你框選的截圖範圍、剪貼簿裡的圖片，或你拖進來的圖片、音訊、影片。
- **追問**：同一段對話的內容和之前的回答。
- **Live 即時翻譯**：執行期間持續傳送你選的來源，包括麥克風聲音、電腦播放的聲音，以及你選的螢幕範圍畫面。按「停止」、關閉字幕列，或閒置自動暫停後就停止傳送。
- **測試連線**：一句測試用的短句。

這些內容會直接從你的電腦送到該服務，由該服務依它的條款處理：

- Google Gemini：<https://policies.google.com/privacy>、<https://ai.google.dev/gemini-api/terms>
- OpenAI：<https://openai.com/policies/privacy-policy>
- Anthropic：<https://www.anthropic.com/legal/privacy>

使用「搜尋」時，上網查資料是由上述服務執行（例如 Gemini 的 Google 搜尋），ContextLens 不會自己連搜尋引擎。注意：部分服務的免費方案可能會把資料用於改進模型，請以服務商的條款為準。

## 其他連線

- **檢查更新**：每次啟動時，向 GitHub 下載 `https://github.com/ai-rhapsody/ContextLens/releases/latest/download/latest.json`。你按下更新時，也從 GitHub 下載安裝檔。GitHub 會看到你的 IP 位址和程式版本，依 [GitHub 隱私聲明](https://docs.github.com/site-policy/privacy-policies/github-general-privacy-statement)處理。
- **開啟連結**：點來源連結或搜尋關鍵字時，會用你的預設瀏覽器開啟該網頁（搜尋關鍵字會開 Google 搜尋）。

除此之外，ContextLens 不會連到其他地方。

## 聯絡

有問題請到 <https://github.com/ai-rhapsody/ContextLens/issues> 提出。

---

## Summary in English

ContextLens runs on your computer. It has no servers, no accounts, no telemetry and no crash reporting; the developers receive none of your data.

- **Stored locally:** API keys in the OS keychain; settings and (optional) history in your user folders. You can turn history off and delete it in the app.
- **Sent out, only when you act:** the selected text, the screen region you pick, files you add, follow-up questions, and, while Live runs, the microphone, system audio and screen region you chose. All of it goes straight to the AI provider you configured (Google Gemini, OpenAI or Anthropic) with your own API key, under that provider's terms.
- **Other connections:** an update check to GitHub at each start (`latest.json`) and the update download when you install one; links you click open in your browser.
