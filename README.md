# ContextLens

A cross-platform desktop app (Tauri 2 + React). Select text, press a hotkey, and a popup next to the cursor streams the translation; turn on tutor mode and it also explains the sentence structure, usage and background. It uses your own Gemini, ChatGPT or Claude API key.

The app's interface is in Traditional Chinese; this README gives the Chinese label in parentheses where it helps you find something in the app.

## Download

Get it from [Releases](https://github.com/ai-rhapsody/ContextLens/releases/latest):

- **Windows**: `ContextLens_<version>_x64-setup.exe`
- **Linux**: `.AppImage` (updates itself), `.deb` or `.rpm`
- **macOS**: not in releases for now. The `build` workflow can still produce an unsigned `.dmg`; if macOS then says the app is damaged, run `xattr -cr /Applications/ContextLens.app` in Terminal.

Once installed, the Updates page (更新) updates the app online. For how your data is handled, see the [privacy policy](PRIVACY.md).

## Features

- **Hotkeys**: add as many as you like, each with a source and an action
  - Sources: **screen capture** (pick a region, a window or the full screen right after pressing it, with no separate screenshot first), the selected text, an image on the clipboard, Live on/off
  - Actions: **Translate** (翻譯, optionally with tutor notes), **Explain** (解釋: charts, diagrams, memes, code, exam questions), **Search** (搜尋: identifies the subject and looks it up on the web, with sources and the search terms used)
- **Translation popup**: streams next to the cursor; switch the action at the top to analyse again. Clicking outside closes it while it is fresh, but once you have used it, it stays. It can be resized and pinned in place.
- **Images, audio and video**: drop them in, pick a file, or paste with `Ctrl+V`
- **Live translation**: a caption bar at the bottom of the screen.
  - When it only listens and tutor notes are off, Gemini uses its dedicated interpreter model `gemini-3.5-live-translate-preview`, which translates while people are still talking. With tutor notes on, or while watching the screen, it uses the real-time model.
  - Listening to the microphone or system audio (Windows only for now) needs a Gemini or ChatGPT real-time model.
  - Watching only the screen works with any model that reads images, Claude included; a frame is sent once the screen stops changing.
  - The provider can be chosen automatically or by hand.
- **Model capabilities**: what a model can do (images, audio, video, Live, web search) is worked out from the provider and the model name. When it cannot do something, one click switches to another provider you have a key for.
- **History**: stored on your computer, searchable, deletable, and can be turned off; search results keep their source links
- **Thinking animation**, and in-app updates (signature-checked)

| Provider | Text | Images | Audio | Video | Live listening | Web search |
| --- | --- | --- | --- | --- | --- | --- |
| Gemini | ✓ | ✓ | ✓ | ✓ | ✓ Live API | ✓ Google Search |
| ChatGPT (OpenAI) | ✓ | ✓ | | | ✓ Realtime API | search models only |
| Claude (Anthropic) | ✓ | ✓ | | | | ✓ |

Actual capabilities are worked out from the model name; the rules are in `crates/core/src/caps.rs`. Models are picked from a drop-down list (in `src/lib/models.ts`, last reviewed October 2026), or choose "Custom…" (自訂…) to enter any name.

Default models: Gemini `gemini-3.5-flash-lite` (real-time `gemini-3.8-live`), ChatGPT `gpt-6-luna` (real-time `gpt-realtime-2.1-mini`), Claude `claude-opus-5-5`. Defaults from older versions are upgraded automatically; models you chose yourself are kept.

## Development

You need Node 22+, stable Rust, and [Tauri's system dependencies](https://v2.tauri.app/start/prerequisites/).

```bash
npm install
npm run tauri dev      # run in development
npm test               # frontend unit tests
cargo test -p contextlens-core   # provider streaming, prompt and settings tests (no GUI libraries needed)
npm run tauri build    # package
```

Linux also needs `libxdo-dev` (simulating the Copy key), `libpipewire-0.3-dev` and `libclang-dev` (screen capture), and `libasound2-dev` (Live recording).

### Layout

| Path | Contents |
| --- | --- |
| `crates/core` | No GUI dependencies: settings, prompts, requests and SSE parsing for the three providers |
| `src-tauri` | Windows, hotkeys, clipboard capture, keychain, tray, update plugin |
| `src/popup` | The translation popup |
| `src/settings` | The settings window (general, models and keys, hotkeys, updates) |

How the selected text is captured: remember the clipboard → simulate Copy → read it → restore the clipboard (on Linux the PRIMARY selection is read first). On Windows the popup asks UI Automation where the selection is, so it opens beneath it rather than over it.

On macOS, the first use needs ContextLens allowed under System Settings → Privacy & Security, for both Accessibility and Screen Recording. Wayland does not let apps register global hotkeys: the Hotkeys page (熱鍵) lists a `--hotkey N` command for each one, which you can bind to a custom shortcut in your desktop settings.

## Updates

The in-app Updates page checks `latest.json` on the GitHub release, then downloads, verifies, installs and restarts.

**Signing key**: the public key is in `tauri.conf.json`; the matching private key was generated during development (no password). For the `release` workflow to sign updates, the private key must be stored as a GitHub secret:

1. In the repo, go to Settings → Secrets and variables → Actions → New repository secret
2. Name it `TAURI_SIGNING_PRIVATE_KEY` and paste the full text of the private key file (it has no password, so `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` is not needed)
3. **Keep a separate backup of the private key**: a secret cannot be read back once saved. If the key is lost, no update can be published that older versions will accept, and users have to reinstall.
4. To use your own key: run `npx tauri signer generate -w ~/.tauri/contextlens.key`, paste the new public key into `plugins.updater.pubkey`, and update the secret. After a key change, older versions cannot update themselves.

**Releasing**: go to Actions → `release` → Run workflow. Leave the version empty to release the latest tag plus one patch (v0.1.11 → 0.1.12), or type one such as 0.2.0. It builds Windows and Linux; choose `windows` to build Windows only. The `v<version>` tag is created automatically. Pushing a tag also starts a release. To try a build without publishing it, run `build`, which needs no secrets.

**macOS signing and notarization** (optional, for when macOS returns to releases): add these Actions secrets to the repo:
- `APPLE_CERTIFICATE`: the Developer ID Application certificate exported as a .p12, in base64
- `APPLE_CERTIFICATE_PASSWORD`
- `APPLE_SIGNING_IDENTITY`, for example `Developer ID Application: Name (TEAMID)`
- `APPLE_ID`
- `APPLE_PASSWORD`: an app-specific password
- `APPLE_TEAM_ID`

Without them, the app is ad-hoc signed, and the log says so.

The update endpoint is `https://github.com/ai-rhapsody/ContextLens/releases/latest/download/latest.json`, so **the repo must be public**, or installed apps cannot download it anonymously. To keep it private, host `latest.json` and the installers elsewhere (S3, Cloudflare R2 and so on) and change `endpoints`.

## Privacy

ContextLens has no servers of its own and collects no usage statistics. The text you select, your screenshots and files, and Live's audio and screen go straight to the AI provider you set up, and only when you act. See the [privacy policy](PRIVACY.md).

## Code signing policy

Free code signing provided by [SignPath.io](https://about.signpath.io/), certificate by [SignPath Foundation](https://signpath.org/) (takes effect once the application is approved).

- **Committers and reviewers:** [ai-rhapsody](https://github.com/ai-rhapsody)
- **Approvers:** [ai-rhapsody](https://github.com/ai-rhapsody)

Only files built from this repo's source by the `release` workflow on GitHub Actions are signed, and every release is approved by hand by an approver. macOS builds, when they return to releases, are signed with the developer's own Apple Developer ID certificate and notarized by Apple.

Privacy: this program will not transfer any information to other networked systems unless specifically requested by the user or the person installing or operating it. See the [privacy policy](PRIVACY.md).

## License

[GPL-3.0-or-later](LICENSE). You may use, change and share it freely. If you publish a modified version, you must release its complete source under the GPL as well.
