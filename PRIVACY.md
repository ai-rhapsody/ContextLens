# ContextLens Privacy Policy

Last updated: October 28, 2026

ContextLens is a desktop program that runs on your computer. It has no servers of its own, needs no account, and collects no usage statistics or crash reports. The developers receive none of your data.

## Data stored on your computer

| Data | Where |
| --- | --- |
| API keys | Your operating system's keychain (Windows Credential Manager, macOS Keychain, Linux Secret Service). They are never written to the settings file or passed to the app's interface. |
| Settings | `settings.json` in your user settings folder |
| History (can be turned off) | `history.json` in your user data folder: up to 1,000 entries, holding the original text, the result, image thumbnails and follow-up questions |

History can be turned off on the General page (一般) and deleted, one entry at a time or all at once, on the History page (歷史紀錄). Uninstalling and deleting those folders removes all of the data; remove your keys on the Models and Keys page (模型與金鑰) first if you like.

To read the selected text, ContextLens briefly uses the clipboard and restores what was on it right after.

## Data sent out

Only when you press a hotkey, click a button or start Live does ContextLens send content to **the AI provider you set up**, using **your own API key**:

- **Translate, Explain, Search**: the selected text, the screen region you picked, the image on the clipboard, or the images, audio and video you added.
- **Follow-up questions**: the conversation so far and the earlier answers.
- **Live translation**: while it runs, the sources you chose are sent continuously: the microphone, the sound your computer plays, and the screen region you picked. Sending stops when you press Stop, close the caption bar, or Live pauses itself after a quiet spell.
- **Connection test**: one short test sentence.

This content goes straight from your computer to that provider, which handles it under its own terms:

- Google Gemini: <https://policies.google.com/privacy>, <https://ai.google.dev/gemini-api/terms>
- OpenAI: <https://openai.com/policies/privacy-policy>
- Anthropic: <https://www.anthropic.com/legal/privacy>

When you use Search, the web search is run by that provider (Gemini's Google Search, for example); ContextLens never contacts a search engine itself. Note that some providers' free tiers may use your data to improve their models; their terms decide.

## Other connections

- **Update check**: at every start, ContextLens downloads `https://github.com/ai-rhapsody/ContextLens/releases/latest/download/latest.json` from GitHub, and when you choose to update, it downloads the installer from GitHub too. GitHub sees your IP address and the app version, and handles them under the [GitHub Privacy Statement](https://docs.github.com/site-policy/privacy-policies/github-general-privacy-statement).
- **Opening links**: clicking a source link or a search term opens it in your default browser (search terms open a Google search).

ContextLens connects to nothing else.

## Contact

Questions are welcome at <https://github.com/ai-rhapsody/ContextLens/issues>.
