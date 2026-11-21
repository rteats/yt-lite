# Native Slint UI with libmpv playback

yt-lite will use a Rust-native Slint interface without Electron or an embedded WebView, with libmpv for playback rather than a custom decoding engine. This prioritizes a lightweight desktop experience and mature hardware-accelerated media functionality over pure-Rust dependencies or HTML-based interface tooling. Windows 10/11 x64 comes first; Linux remains a future target.

**Consequences:** The first technical milestone is a minimal Slint + libmpv proof of concept, before building the full app shell. It must verify video-surface integration, hardware acceleration, overlay controls, window resizing, seeking, fullscreen, and playback frame pacing on Windows. Native-library redistribution must also be reviewed. Accessibility, keyboard operation, contrast, scaling, and reduced-motion support are part of UI component acceptance criteria.
