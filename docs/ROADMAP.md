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
- [x] Live mode, step 3: screen frames from a picked region, window or monitor; context for speech, or a screen reader on its own; only changed frames are sent; the caption bar is excluded from capture

- [x] Follow-up questions in the popup and a conversation window
- [x] Launch at login
- [x] Custom actions: a user-written instruction per hotkey
- [x] Glossary of fixed renderings, sent with requests and Live
- [x] Vocabulary: star history items, export to CSV or Anki
- [x] Live: caption size, export to TXT or SRT
- [x] Local diagnostics log and a copy-diagnostics button

## Next
- [ ] Live: follow a window when it moves (today the picked rectangle is fixed)
- [ ] Live: system audio on macOS (ScreenCaptureKit) and Linux (PulseAudio monitor)
- [ ] Live: honour HTTPS proxies for the WebSocket connection
- [ ] Pronunciation (text to speech) for the source text
- [ ] Screen recording clips (video capture of a region)
- [ ] Wayland: global shortcuts through the XDG portal
