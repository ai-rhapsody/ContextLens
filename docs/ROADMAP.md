# Roadmap

## Done
- [x] Global hotkeys with a source (selection, screen capture, clipboard image) and a tutor switch each
- [x] Screen capture overlay: region, window, full screen
- [x] Images, audio and video by drop, file picker or paste
- [x] Per-model capability table and one-click fallback to another provider
- [x] History, searchable, stored locally
- [x] Thinking skeleton with a random aurora
- [x] In-app updates (signed, via GitHub releases)

- [x] Live mode, step 1 and 2: microphone and system audio (Windows loopback) to Gemini Live, caption bar, tutor notes, idle auto-pause, session resumption, history

## Next
- [ ] Live mode, step 3: screen frames (about one per second) alongside the audio
- [ ] Live: system audio on macOS (ScreenCaptureKit) and Linux (PulseAudio monitor)
- [ ] Live: honour HTTPS proxies for the WebSocket connection
- [ ] Follow-up questions in the popup ("ask about this")
- [ ] Pronunciation (text to speech) for the source text
- [ ] Vocabulary notebook: star history items, export to Anki CSV
- [ ] Custom actions: user-written prompts per hotkey (summarise, rewrite, explain code)
- [ ] Launch at login
- [ ] Screen recording clips (video capture of a region)
- [ ] Wayland: global shortcuts through the XDG portal
