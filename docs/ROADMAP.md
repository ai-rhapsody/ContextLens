# Roadmap

## Done
- [x] Global hotkeys with a source (selection, screen capture, clipboard image) and a tutor switch each
- [x] Screen capture overlay: region, window, full screen
- [x] Images, audio and video by drop, file picker or paste
- [x] Per-model capability table and one-click fallback to another provider
- [x] History, searchable, stored locally
- [x] Thinking skeleton with a random aurora
- [x] In-app updates (signed, via GitHub releases)

## Next
- [ ] **Live mode** (Gemini Live models only): stream microphone or system audio, and optionally screen frames, for real-time interpretation. Needs a WebSocket session in Rust (keys stay in Rust) and audio capture in the webview.
- [ ] Follow-up questions in the popup ("ask about this")
- [ ] Pronunciation (text to speech) for the source text
- [ ] Vocabulary notebook: star history items, export to Anki CSV
- [ ] Custom actions: user-written prompts per hotkey (summarise, rewrite, explain code)
- [ ] Launch at login
- [ ] Screen recording clips (video capture of a region)
- [ ] Wayland: global shortcuts through the XDG portal
