# yt-lite — Implementation Specification

**Status:** Approved product direction; implementation readiness subject to prototype, licensing and performance validation.

**Source:** Approved Q1–Q95 design interview; domain glossary and ADR-0001 through ADR-0012. The consolidated design review remains the authoritative record of user decisions where this spec intentionally scopes the first public beta.

## Problem Statement

A desktop user wants a responsive, fully native, low-overhead alternative YouTube client for Windows 10 that can browse and play videos, preserve local subscriptions/history/playlists, present chapters and subtitles correctly, and work with downloaded videos. Browser-based clients and Electron-based alternatives add unwanted runtime overhead or unwanted browser dependencies. Reliance on one unofficial YouTube extraction technique also makes standalone clients unreliable when upstream behavior changes. The user wants a maintainable Rust open-source project, not a disposable one-off player.

## Solution

Deliver yt-lite as a local-first, account-free Rust desktop application using Slint for the GUI, libmpv for playback, SQLite for state, and interchangeable extraction providers. The primary extraction route is local Rust code, with bundled yt-dlp fallback and optional user-enabled proxy providers. A single playback session survives browsing navigation. Profiles isolate personal information; offline media bytes are deduplicated independently of profile visibility. The Windows 10/11 x64 portable and installer releases bundle required runtime components.

Deliver iteratively: prove embedded playback and frame pacing before building the full shell; then add one verifiable end-to-end user journey at a time. Target a daily-driver public beta, then expand toward the full product feature set.

## User Stories

### Basic application and navigation
1. As a Windows 10 x64 user, I want to launch yt-lite without installing Python, mpv, FFmpeg, or other runtime dependencies, so that installation is straightforward.
2. As a portable user, I want all application data in a predictable portable folder or chosen directory, so that I can move installations safely.
3. As a desktop user, I want a native Slint interface with light, dark and system themes, UI scaling and reduced-motion behavior, so that it fits my workflow.
4. As a keyboard user, I want accessible navigation, familiar playback shortcuts, configurable extra shortcuts and media-key support, so that mouse use is optional.
5. As a viewer, I want a collapsible sidebar and adaptable watch-page panels, so that the interface uses desktop screen space effectively.
6. As a viewer, I want optional browsing tabs that remember navigation state, so that I can explore without losing my place.
7. As a viewer, I want a second invocation or external link to open in my existing window, so that I do not accidentally run duplicate players.
8. As a returning user, I want my browsing state and resume position restored without video autoplay, so that startup is predictable.

### Discovery and browsing
9. As a viewer, I want to paste YouTube video/channel/playlist URLs or search online, so that finding content is simple.
10. As a viewer, I want results filtered by type and supported date/duration criteria, so that I can narrow a search.
11. As a viewer, I want a separate search scope for my local history, subscriptions, playlists and downloads, so that it can work offline.
12. As a viewer, I want configurable home sections for supported recommendations, trending, subscriptions and continue-watching, so that the home page is useful.
13. As a viewer, I want available local home sections to remain usable when remote recommendations fail, so that failures are contained.
14. As a subscriber, I want a chronological local subscription feed, channel grouping and content filters, so that I can manage followed channels.
15. As a user with multiple viewing contexts, I want local profiles with independent subscriptions, history, queues, playlists, progress and feeds, so that activities do not mix.
16. As a viewer, I want advanced filters for Shorts, live content, watched videos, channels, keywords and duration, so that irrelevant content is hidden.
17. As a user, I want subscribed content to refresh when opened, subject to cache freshness and rate limits, so that the app avoids constant polling.
18. As a viewer, I want channel pages, video descriptions, related videos and read-only comments, so that I can browse without a Google account.
19. As a viewer, I want external actions to open YouTube in my browser when authentication is required, so that the app does not need cookies.
20. As a viewer, I want a dedicated Shorts browsing experience and livestream playback with on-demand live chat/replay where supported, so that those formats remain usable.

### Playback
21. As a viewer, I want playback to start, seek, pause and fullscreen within the native window, so that watching feels like a full desktop player.
22. As a viewer, I want up to 4K60 where formats and hardware permit, with automatic quality and configurable resolution/codec caps, so that playback matches my hardware.
23. As a viewer, I want explicitly selected quality preserved, but automatic quality to downgrade if necessary, so that playback balances quality and smoothness.
24. As a viewer, I want hardware decoding where available and a visible, controllable software-decoding fallback when it fails, so that failures are understandable.
25. As a viewer, I want selectable subtitles, styles, language preferences and accurately seekable chapters, so that content is navigable and accessible.
26. As a viewer, I want a searchable, timestamped transcript and click-to-seek, so that I can locate spoken content.
27. As a viewer, I want speed, volume, audio-track, aspect ratio, A–B repeat, frame stepping and playback statistics without cluttered default controls, so that power-user functions remain accessible.
28. As a viewer, I want a persistent single playback session while navigating, minimizing, detaching or entering fullscreen, so that playback is not restarted.
29. As a viewer, I want normal clicks to replace playback, explicit queue actions to enqueue, and new browsing tabs not to interrupt playback, so that navigation behavior is predictable.
30. As a viewer, I want profile-specific queues to survive application restarts without autoplay, so that I can continue later.
31. As a viewer, I want configurable SponsorBlock skipping/segment markers and estimated Return YouTube Dislike counts, so that optional playback enhancements are available.
32. As a privacy-conscious viewer, I want those third-party integrations separately switchable, so that I control external metadata requests.

### Local data, privacy and migrations
33. As a viewer, I want searchable and removable history with watched percentages, timestamps and resume positions, so that I can find and continue videos.
34. As a viewer, I want temporarily private sessions that do not save history/progress/recommendation signals, so that occasional private viewing stays unrecorded.
35. As a private-session user, I want explicit confirmation before saving a subscription, playlist edit or download to a local profile, so that permanent actions are intentional.
36. As a profile user, I want switching profiles to save the prior position and stop current playback, so that histories remain isolated.
37. As a returning viewer, I want unavailable videos retained in history and playlists with known metadata and clear labels, so that remote deletions do not erase local records.
38. As a migrant, I want importers for available FreeTube/NewPipe/YouTube Takeout formats with preview, deduplication, merge and backup, so that old local data is preserved.
39. As a user, I want manual backup/restore and backups before destructive migrations, so that upgrades and imports cannot silently destroy local data.
40. As a user, I want configurable cache limits and expiry, so that metadata and thumbnails do not grow indefinitely.
41. As a user, I want no unsolicited analytics or telemetry, and only redacted locally exportable logs, so that activity is not collected by the application operator.

### Downloading and offline use
42. As a viewer, I want to download a video or audio in an available quality, preserving available chapters, subtitles, thumbnail and descriptive metadata, so that I can watch offline.
43. As a viewer, I want downloaded videos to open in the same player without network access, so that offline mode is reliable.
44. As a downloader, I want a persisted queue, progress reporting, retries and resumable transfers where supported, so that interruptions do not discard work.
45. As a profile user, I want each profile's library contents private while identical stored media is shared safely, so that isolation does not waste disk space.
46. As a downloader, I want multiple quality variants to coexist and identical bytes deduplicated, so that I can keep a large and small copy when useful.
47. As a user, I want storage-space reporting, relocation, deletion recovery and eventual cleanup of unreferenced media, so that disk usage stays manageable.
48. As a user, I want downloads to continue across profile switches while the application runs, so that switching views does not cancel transfers.
49. As an offline user, I want explicit offline mode and automatic handling of lost connectivity, so that local content always remains available.

### Reliability, packaging and maintenance
50. As a viewer, I want provider-specific retry and fallback for search, metadata, streams, chapters and subtitles, so that failure in one extraction feature does not disable the whole application.
51. As a viewer, I want streaming URL expiry handled with resumed playback position when possible, so that network interruptions do not force a restart.
52. As an operator, I want independently verified, revertible extractor updates, so that YouTube changes can be repaired without replacing the whole app.
53. As a desktop user, I want portable and installer releases plus optional, approved application updates, so that I can choose my maintenance method.
54. As a troubleshooter, I want actionable errors, retry options and a local redacted diagnostics export, so that I can report problems safely.
55. As an accessibility user, I want keyboard and screen-reader support wherever platform and Slint permit, with contrast and reduced motion, so that the UI is operable.
56. As a contributor, I want deterministic CI, separate real-network tests and reproducible benchmark results, so that regressions can be diagnosed without flaky pull requests.
57. As a contributor, I want a clear license and provenance record for borrowed code and bundled native libraries, so that published binaries satisfy redistribution obligations.

## Implementation Decisions

### Core topology and seams
- Keep the native UI as a presenter of application state, not an owner of decoding, extraction or persistent data.
- Use a single Rust application-service boundary for user-visible operations (open/play/search/subscribe/save/download/switch profile/import). Test externally observable state transitions against that boundary using replaceable adapters for media engine, extraction, persistence, clock/filesystem and network.
- A single process owns an active data directory. Incoming URL/protocol/CLI requests to the same directory route to the existing process.
- Model tabs as browsing contexts; the one playback session is not owned by a tab. Detaching or minimizing the player does not create a second decoder. Profile switching terminates the current playback session after saving progress.
- Persist profile-owned information separately from globally deduplicated stored-media entities. Reference-based deletion with a bounded recovery window guards against deletion across profiles.
- Use SQLite with versioned migrations, validated imports and recoverable backups. Keep secrets/credentials out of the app's initial account-free design.
- Keep the extractor capability-oriented. Not all providers support the same metadata or features. Never silently route to a third-party proxy that was not enabled. Separate the failure of an upstream integration from local feature availability.
- Treat SponsorBlock and Return YouTube Dislike as externally requested metadata integrations, distinct from extraction proxies. Both are enabled by default with clear notices and separate toggles.
- Include user-controllable automatic quality caps and decoder fallback; do not silently override explicitly locked manual quality.
- Use portable-local data, installed-AppData data, or an explicitly configured data directory, with data-locking and filesystem error handling.
- Deliver Windows 10/11 x64 first; Linux, ARM64, synced accounts and a general extension API are later opportunities.

### Release boundaries
- First proof is embedded Slint + libmpv Windows playback (rendering, overlay, seek, resize, fullscreen, pacing, decoder behavior).
- First usable vertical slices prove entering a URL, metadata extraction, and playback; then add search, local state and other browsing paths.
- First public beta contains a reliable native player, search/browsing, chapters/subtitles, history, subscriptions, playlists, profiles/settings and basic downloaded offline playback.
- Advanced download scheduling and auto-subscription downloads, cross-device sync, third-party plugins, and authenticated interactions are deferred. Other advanced features (Shorts, chat replay, transcript search, content filtering, advanced playback controls) remain in product scope, and must receive explicit rollout tickets after beta prerequisites.
- Application update checks are opt-in or user-configurable with approval for installing; extractor updates require integrity verification, compatibility/rollback handling and explicit control.

## Testing Decisions

- Prefer behavior-focused tests from the public application-service seam: a user action produces observable UI state, persistent state, playback state, or a deterministic error. Avoid tests that depend on private module structure.
- Use recorded/mocked provider fixtures for deterministic search, metadata, captions, chapter, network-error, expired-URL and unavailable-video cases. Live-provider testing is separately scheduled or manually triggered.
- Validate playback controls, seek and lifecycle with a controlled local test clip before live YouTube playback. Run hardware/decoder and frame-pacing checks on real Windows machines.
- Test profile isolation, private mode and persistent actions; closing/restarting; second-instance routing; portable vs installed directories; locked databases, interrupted migrations, backups and import recovery.
- Test download job persistence, interruption/retry, deduplication, multi-quality coexistence, shared references, cancellation, deletion recovery and file relocation.
- Test explicit offline mode with all network adapters blocked; missing remote sections must not block local history or offline playback.
- Test packaging by executing installer/portable artifacts on clean Windows systems without manually installed playback/extraction tools.
- Benchmark cold/warm startup, idle CPU and memory, scroll responsiveness, UI frame times, dropped video frames and GPU load; establish numeric acceptance thresholds only after baseline measurements on development and lower-end hardware.
- Review accessibility and native UI interactions manually and through available automated checks; ensure diagnostics exclude secrets.
- There is no existing implementation or automated-test precedent in this repo; initial slices establish the testing conventions.

## Out of Scope

- Electron, Tauri/WebView, HTML-first interface, custom Rust decoder, or mandatory online backend.
- Google login, browser-cookie import, commenting/liking through authenticated APIs, or mechanisms to bypass access restrictions.
- A guarantee that every YouTube video can be extracted, or that every GPU supports 4K60/AV1/HDR.
- Automatic background telemetry, mandatory cloud sync, always-running polling processes, or automatic unapproved updates.
- Built-in plugin marketplace, fully automated subscription downloading and high-complexity workflows in the first public beta.
- macOS, Linux and Windows ARM64 delivery in the initial Windows x64 beta.

## Further Notes

- ADR-0001 through ADR-0012 and the glossary define terminology and hard architectural boundaries. The approved design-review records the 95 choices.
- Technical uncertainties remain: Slint/libmpv compositing and frame pacing, sustainable direct-Rust extraction, yt-dlp helper bundling/update, and AGPL/native dependency license compatibility.
- Verify source code provenance and legal compatibility before incorporating material from FreeTube, NewPipe, Invidious, Piped or other references. Porting code is still reuse subject to its license.
- Since no issue-tracker workflow / triage-label mapping has been configured for the AI Hero skills, the spec is first published as a reviewable repository document. Tracker publishing and ticket issue creation follow tracker setup and ticket-breakdown approval.
