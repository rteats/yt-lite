# yt-lite — Design Review (Awaiting Sign-off)

This is a synthesis of user decisions Q1–Q95, not an implementation specification or proof that proposed dependencies work. Confirm this review before converting it into a final spec and implementation tickets.

## Goal and constraints

- Public open-source Rust desktop YouTube alternative and personal daily driver, initially for Windows 10/11 x64; Linux later.
- Native Slint GUI, no Electron and no embedded WebView. libmpv handles playback; do not build a custom decoder.
- Lightweight-first: minimize idle RAM/CPU, startup work, background traffic, and decorative effects. Measure performance on development and lower-end reference hardware.
- Account-free by default: local SQLite profiles, no Google login, no cookies for restricted content initially.
- Fully bundled portable and installer distributions. No manual installation of playback or extraction dependencies for ordinary users.
- AGPLv3-oriented licensing direction; actual license compatibility, notices, redistribution and sources must be verified before shipping.

## Product behavior

### Browsing and discovery
- Native desktop adaptation of familiar YouTube navigation: collapsible sidebar, configurable home, optional resource-conscious browsing tabs, configurable video-page panels, adaptive light/dark/system themes and scaling.
- Separate YouTube search and local library search, with filters. Home combines available signed-out recommendations, subscriptions, trending, and continue watching. Failed online sections do not block local content.
- Local profile isolation for subscriptions, playlists, history, progress, playback queue, feed preferences and offline-library visibility.
- Advanced content filters, separate local profiles, import from supported FreeTube/NewPipe/Takeout exports with preview/merge/deduplication/backup. Retain records of unavailable videos.
- English-first strings, but localization-ready. Keyboard and screen-reader accessibility where supported, sufficient contrast and reduced-motion controls.

### Playback
- One active libmpv playback session shared by the watch page, mini-player, detached window, fullscreen and tabs. Normal click replaces playing video; explicit queue action enqueues; opening a browsing tab alone does not interrupt.
- Persist per-profile playback queues, history and resume positions without autoplay on restart. Switching profile saves progress and stops playback. Private sessions do not store viewing history or progress; permanent actions require confirmation.
- Hardware-accelerated playback targeting 4K60 when supported; adjustable automatic quality/codec/resolution limits, manual overrides, controlled quality downgrade and optional software decode fallback with notification.
- Chapters, subtitles, styling, searchable seekable transcripts, playback statistics and advanced controls in unobtrusive menus.
- Dedicated Shorts browsing and livestream playback, on-demand chat and replay where supported. SponsorBlock and Return YouTube Dislike on by default, with independent toggles.

### Extraction and network
- Direct Rust extraction preferred; yt-dlp fallback; optional Invidious/Piped providers disabled by default. Per-capability fallback for metadata, streams, subtitles, search and chapters, respecting user privacy preferences.
- Retry with backoff, recover expired URLs and position, provide actionable error diagnostics. Restricted videos requiring credentials open externally rather than importing cookies initially.
- Extractor component updates can be independently verified, user controlled and rolled back; application update checks are opt-in and approval-based.
- Explicit offline mode forbids network-dependent operations; automatic offline handling keeps locally saved media available. Local logs are user-exportable; no telemetry.

### Downloads and lifecycle
- Offline library with preserved available metadata, chapters, subtitles and thumbnails, and basic downloads in the first public beta.
- Profile-specific visible library entries can reference shared stored media to avoid duplicate bytes. Different qualities/formats of the same video can coexist; identical assets can be deduplicated.
- Downloads persist and resume where possible after restart/network failure and continue across profile switches while yt-lite is running. Managed library location, disk space reporting, relocation and cleanup.
- Deletions have a temporary recovery/undo period before reclaiming unreferenced media; do not delete another profile's shared bytes.
- Installed builds use AppData; portable builds use data alongside executable; custom data location supported. One running instance owns a given data directory; second launch sends requests to it.
- By default closing the window exits; optional tray mode allows continued playback. Optional Windows URL/protocol/CLI integration.

## First public beta target

A daily-driver release should include a reliable native application shell, search and core browsing, video playback, chapters, subtitles, detailed local history, subscriptions, playlists, profiles, settings, and basic integrated offline downloads/playback. It should expose useful errors and diagnostics. The first public beta need not include every advanced feature specified for the eventual product.

Features such as advanced automated downloading, full plugin ecosystem, deeper authenticated functions, cross-device synchronization, macOS, ARM64, and extensive customization are not first-beta prerequisites. Other advanced features (such as Shorts feed, live chat/replay, transcript panel, SponsorBlock and RYD) remain in the intended product scope; final beta cutoff should be ticketed based on dependencies and implementation evidence, not silently presumed complete.

## Execution and proof

1. **Proof of concept:** Minimal Windows x64 Slint + libmpv prototype verifying embedded rendering, overlays, seeking, resize, fullscreen, hardware decoding fallback and smooth frame pacing.
2. **Extraction proof:** Representative live videos, captions/chapters, URL expiry, format selection and automatic provider fallback, with offline mocked fixtures.
3. **Vertical slices:** Working application path from opening a URL/search through playback, then history, subscriptions, queues, profiles and offline downloads.
4. **Release preparation:** Portable and installed Windows builds bundling all runtime dependencies, dependency license audit, updates, data-migration backups, crash/restart tests, and installer/portable isolation.
5. **Verification:** Deterministic PR checks, separate live YouTube checks, Windows playback tests, measurable performance baselines and human UI tests.

Use feature branches and pull requests, with CI before merging to main.

## Technical assumptions to validate, not user questions

- The selected Slint rendering backend can support satisfactory native libmpv video embedding, overlays and frame pacing, without excessive CPU/GPU cost on Windows 10.
- An acceptable direct-Rust extraction library exists or can be maintained; fallback works for distinct YouTube capabilities, and metadata/stream behavior remains inherently subject to upstream changes.
- The yt-dlp fallback and all required helper runtimes/components can be safely bundled and independently updated with integrity checks, provenance and rollback.
- Video formats, hardware decode support, streaming quality, HDR and codec capabilities vary by GPU/driver; 4K60 is a supported goal, not a guarantee for every machine or video.
- All intended dependency/ported-code licenses can be reconciled with the AGPLv3-oriented plan. Slint and native library license terms require specific verification.
- Direct service access, optional proxy requests, default SponsorBlock/RYD requests and truly offline behavior must be transparently distinguished.
- Import/export adapters must be tested against real current formats; no guarantee that every origin stores or exports full watch history.
- Profile-owned data plus shared bytes requires transactional ownership/recovery rules, and restoring after crashes must not produce orphaned or accidentally deleted files.

## Remaining review points before final specification

- Validate technical feasibility, licensing and packaging via the prototype and focused primary-source research.
- Establish measured performance budgets rather than invent numerical thresholds.
- Translate the first-beta scope into dependency-ordered, independently testable tickets and define which optional advanced features are postponed.
- Obtain explicit sign-off on this consolidated direction before treating the review as final.
