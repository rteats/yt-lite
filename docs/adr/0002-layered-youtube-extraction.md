# Local-first layered YouTube extraction

yt-lite will prefer local Rust-based YouTube extraction, use yt-dlp as a fallback, and allow optional Invidious/Piped backends. Extraction methods are replaceable rather than coupled to the GUI or player, because YouTube's access behavior can change and any single extractor may fail. Users can control provider priority and third-party requests; no remote backend is mandatory.

Fallback is capability-specific: search, metadata, streaming, subtitles, chapters, and other functions can choose working providers independently, while respecting configured provider order and privacy controls. Expired media URLs can be refreshed and automatic-mode playback may downgrade quality or change codec to recover from buffering; explicitly locked manual quality is not silently overridden.

**Consequences:** Automatic recovery must respect privacy settings. External extraction proxies are disabled by default; this does not prohibit separately configured SponsorBlock and Return YouTube Dislike metadata requests. Restricted videos requiring credentials are unsupported initially; show a clear error and offer opening in a browser. Each bundled or ported component requires a license review.
