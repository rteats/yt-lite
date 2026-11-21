# Local-first layered YouTube extraction

yt-lite will prefer local Rust-based YouTube extraction, use yt-dlp as a fallback, and allow optional Invidious/Piped backends. Extraction methods are replaceable rather than coupled to the GUI or player, because YouTube's access behavior can change and any single extractor may fail. Users can control provider priority and third-party requests; no remote backend is mandatory.

**Consequences:** Automatic recovery must respect privacy settings, and each bundled or ported component requires a license review.
