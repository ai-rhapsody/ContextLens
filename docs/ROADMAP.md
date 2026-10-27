# Roadmap

## MVP (this repo today)
- [x] Global hotkeys, add / remove / rebind, each bound to an action
- [x] Capture selected text, translate in a floating window next to the cursor, streamed live
- [x] Tutor mode: structure, usage and background, switchable per popup and per hotkey
- [x] Gemini, ChatGPT (OpenAI) and Claude (Anthropic) with your own API key in the OS keychain
- [x] Target language setting, default 繁體中文
- [x] In-app updates (signed, via GitHub releases)

## Next
- [ ] **Image capture**: region screenshot hotkey; send to a vision model to translate or explain text in the image. Add `Action::CaptureImage` in `crates/core/src/settings.rs`, a `capture/image.rs` beside `capture.rs`, and an image part in each provider's request in `provider.rs`.
- [ ] **Video clip capture**: record a short region clip, sample frames (and audio when the provider supports it), then explain.
- [ ] History of past translations, searchable, exportable to flashcards.
- [ ] Per-hotkey target language and provider.
- [ ] Wayland: global shortcuts through the XDG portal.
