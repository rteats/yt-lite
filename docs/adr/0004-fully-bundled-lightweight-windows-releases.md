# Fully bundled Windows releases with lightweight-first behavior

yt-lite will ship portable and installer-based Windows distributions with required runtime components bundled; end users should not need to install mpv, FFmpeg, Python, or extraction tools manually. Minimized idle memory, CPU, startup work, and background activity take priority over elaborate animations and always-on prefetching.

**Consequences:** Bundling increases packaging and licensing work. Performance budgets will be established through measurements, not invented targets. Closing the main window exits by default; continuing playback in the system tray is opt-in. Optional URL/protocol and command-line integration should not require a background service.
