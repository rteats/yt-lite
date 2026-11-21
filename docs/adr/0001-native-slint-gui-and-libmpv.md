# Native Slint UI with libmpv playback

yt-lite will use a Rust-native Slint interface without Electron or an embedded WebView, with libmpv for playback rather than a custom decoding engine. This prioritizes a lightweight desktop experience and mature hardware-accelerated media functionality over pure-Rust dependencies or HTML-based interface tooling. Windows 10/11 comes first; Linux remains a future target.

**Consequences:** Video-surface integration, overlays, frame pacing, hardware decoding, and native-library redistribution must be validated early.
