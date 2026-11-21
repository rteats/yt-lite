# yt-lite — Proposed Tracer-Bullet Tickets

**Status: draft for granularity/dependency review.** These are **not** published GitHub Issues, and no `ready-for-agent` labels have been applied. They derive from the approved product design and the implementation specification.

Work is organized as small, independently demoable vertical slices, except early feasibility proofs and the final release verification gate. "Blocked by" means a dependency that must actually be completed before the ticket is started; unrelated work may proceed in parallel. The graph is acyclic.

## At a glance

| # | Ticket | Blocked by |
|---|---|---|
| 01 | Prove native Slint/libmpv rendering | None |
| 02 | Prove YouTube extraction capabilities | None |
| 03 | Validate bundled runtime and license compatibility | 01, 02 |
| 04 | Launch accessible native watch shell | 01 |
| 05 | Open a YouTube URL and play it | 02, 04 |
| 06 | Search YouTube and open a result | 05 |
| 07 | Navigate channels and read-only video information | 06 |
| 08 | Control chapters, subtitles and transcripts | 05 |
| 09 | Select quality and recover video decoding | 05 |
| 10 | Save and restore local watch history | 05 |
| 11 | Keep one player across tabs and window states | 04, 10 |
| 12 | Create isolated local profiles | 10 |
| 13 | Subscribe locally and browse channel feeds | 07, 12 |
| 14 | Create local playlists | 07, 12 |
| 15 | Compose a resilient home feed | 10, 13 |
| 16 | Persist a per-profile playback queue | 11, 12 |
| 17 | Browse privately without recording progress | 12 |
| 18 | Download and play one offline video | 05, 12 |
| 19 | Resume downloads and deduplicate media variants | 18 |
| 20 | Import/export and recover local data | 12, 13, 14 |
| 21 | Recover failed extraction capabilities | 02, 05 |
| 22 | Apply SponsorBlock and dislike metadata | 05, 08 |
| 23 | Operate offline and search the local library | 10, 18 |
| 24 | Package and release the daily-driver Windows beta | 03, 06, 08, 09, 11, 13, 14, 15, 16, 17, 19, 20, 21, 22, 23 |

## Ticket briefs

### 01. Prove native Slint/libmpv rendering

**What to build:** A Windows x64 prototype plays a local test clip inside a native Slint window, with video controls and a responsive overlay.

**Blocked by:** None (can start immediately).

**Acceptance criteria:**
- [ ] Seek, pause, resize and fullscreen work without detaching/recreating playback unexpectedly.
- [ ] Record hardware decode, overlay/compositing and frame-pacing observations on Windows 10; document blockers and fallback choices.

### 02. Prove YouTube extraction capabilities

**What to build:** A small test harness obtains video metadata, selectable streams, captions and chapters from representative YouTube URLs using candidate Rust extraction with yt-dlp fallback.

**Blocked by:** None (can start immediately).

**Acceptance criteria:**
- [ ] Return typed capability results and actionable failures from at least two distinct extraction routes where available.
- [ ] Record cases that fail, external helper requirements and source/license provenance; fixture-based tests work offline.

### 03. Validate bundled runtime and license compatibility

**What to build:** A reproducible prototype package starts on clean Windows x64 without separately installed playback or extraction tools.

**Blocked by:** 01, 02.

**Acceptance criteria:**
- [ ] Verify all bundled executable/library/helper dependencies and their source, license and redistribution obligations.
- [ ] Demonstrate a startup/playback smoke test on a clean environment, documenting any unsolved legal or technical blocker.

### 04. Launch accessible native watch shell

**What to build:** yt-lite launches into a native window with a local test video, standard player controls, navigation placeholders and theme support.

**Blocked by:** 01.

**Acceptance criteria:**
- [ ] Keyboard operation, scaling, light/dark mode and reduced motion behave sensibly.
- [ ] Native UI responds during playback without blocking on decoding work.

### 05. Open a YouTube URL and play it

**What to build:** Pasting a supported YouTube URL resolves its metadata and plays it in the embedded video surface.

**Blocked by:** 02, 04.

**Acceptance criteria:**
- [ ] Title, thumbnail, duration and playback/error status are visible from the viewer's perspective.
- [ ] A failing or restricted URL shows an actionable error and offers an external browser action.

### 06. Search YouTube and open a result

**What to build:** Viewer searches for videos and opens a returned result for playback.

**Blocked by:** 05.

**Acceptance criteria:**
- [ ] Search results show meaningful video metadata and supported filters.
- [ ] Loading/error/empty states do not freeze the player; provider fixtures keep CI deterministic.

### 07. Navigate channels and read-only video information

**What to build:** View channel information, videos, descriptions and available related content without Google login.

**Blocked by:** 06.

**Acceptance criteria:**
- [ ] User can navigate into a channel/video and back without losing browsing position.
- [ ] Missing data or read-only comments have clear states; authenticated actions open externally.

### 08. Control chapters, subtitles and transcripts

**What to build:** Viewer selects captions, seeks to chapter marks and opens a timestamped searchable transcript.

**Blocked by:** 05.

**Acceptance criteria:**
- [ ] Available caption language and chapters display correctly and remain synchronized on seeking.
- [ ] Unavailable transcript/chapters degrade gracefully; UI controls are keyboard accessible.

### 09. Select quality and recover video decoding

**What to build:** Viewer sets automatic quality limits or locks a selected format; decoder fallback is visible and configurable.

**Blocked by:** 05.

**Acceptance criteria:**
- [ ] Automatic selection respects configured caps and can downgrade on stalls; manually locked quality is not silently changed.
- [ ] Hardware decode failure attempts a user-controllable software fallback and reports the result.

### 10. Save and restore local watch history

**What to build:** Watching videos records local history and resume progress; app restart restores the last position without autoplay.

**Blocked by:** 05.

**Acceptance criteria:**
- [ ] History is searchable/removable, shows viewing progress, and preserves unavailable entries.
- [ ] Interrupted save and restart do not corrupt persistent state; startup does not play automatically.

### 11. Keep one player across tabs and window states

**What to build:** Viewer browses in optional tabs while playback moves between watch, mini-player, detached and fullscreen states.

**Blocked by:** 04, 10.

**Acceptance criteria:**
- [ ] Opening a browsing tab preserves playback position and does not create a second decoder.
- [ ] Startup restores tab state without auto-playing; window close exits unless tray mode is enabled.

### 12. Create isolated local profiles

**What to build:** Viewer creates and switches profiles with independent history and resume progress.

**Blocked by:** 10.

**Acceptance criteria:**
- [ ] Profile switching saves old progress and stops playback; no cross-profile history leakage.
- [ ] The storage model allows later queues, subscriptions, playlists and offline-library visibility to attach to profiles.

### 13. Subscribe locally and browse channel feeds

**What to build:** Viewer subscribes to channels and sees filtered chronological updates in the active profile.

**Blocked by:** 07, 12.

**Acceptance criteria:**
- [ ] Subscriptions persist per profile and can be organized into groups.
- [ ] Feed refresh respects freshness, throttling and manual retry instead of polling continually.

### 14. Create local playlists

**What to build:** Viewer creates playlists, adds/removes videos, reorders entries and opens items.

**Blocked by:** 07, 12.

**Acceptance criteria:**
- [ ] Playlist changes persist independently per local profile.
- [ ] Unavailable videos retain locally known metadata and are visibly marked.

### 15. Compose a resilient home feed

**What to build:** Configurable home displays subscriptions, continue-watching, and available remote recommendation/trending sections.

**Blocked by:** 10, 13.

**Acceptance criteria:**
- [ ] User can reorder/show/hide supported sections and apply content filters.
- [ ] A failed online section does not hide local feed/history; clear retry controls are provided.

### 16. Persist a per-profile playback queue

**What to build:** Viewer adds/reorders/plays queued videos, with context-sensitive click behavior and queue restoration.

**Blocked by:** 11, 12.

**Acceptance criteria:**
- [ ] Clicking normal video replaces playback; explicit enqueue does not; tab opening is non-interrupting.
- [ ] Queue belongs to active profile and never autoplays merely because the app restarted.

### 17. Browse privately without recording progress

**What to build:** Viewer activates private mode, watches without storing history/progress, and confirms any permanent action.

**Blocked by:** 12.

**Acceptance criteria:**
- [ ] No private watch state leaks into persistent profile history or recommendations.
- [ ] Subscribe, playlist edits and downloads display explicit persistence confirmation.

### 18. Download and play one offline video

**What to build:** Viewer saves a supported video with retained metadata and plays it without network access.

**Blocked by:** 05, 12.

**Acceptance criteria:**
- [ ] Selected video, available thumbnail/chapters/subtitles and relevant metadata appear as a profile-owned offline item.
- [ ] Blocking every network route still permits offline playback and history within the local library.

### 19. Resume downloads and deduplicate media variants

**What to build:** Viewer manages persisted download jobs that retry/resume, stores multiple qualities and safely shares identical media across profiles.

**Blocked by:** 18.

**Acceptance criteria:**
- [ ] Interrupted transfers survive restart where source support permits; profile switching does not cancel running jobs.
- [ ] Deleting one profile's library item cannot delete files still referenced elsewhere; undo window and storage usage are visible.

### 20. Import/export and recover local data

**What to build:** Viewer backs up/restores data, previews and merges a compatible external-client export, and recovers from a destructive migration.

**Blocked by:** 12, 13, 14.

**Acceptance criteria:**
- [ ] Imports deduplicate compatible entries and back up before changes; malformed exports leave prior data intact.
- [ ] A simulated interrupted migration can restore a known-good database.

### 21. Recover failed extraction capabilities

**What to build:** Viewer keeps playback and browsing functional when an extractor fails or a media URL expires, using configured fallback providers.

**Blocked by:** 02, 05.

**Acceptance criteria:**
- [ ] Provider selection is per capability, respects explicit privacy preferences, and exposes actionable diagnostics.
- [ ] Automatic recovery preserves playback position where practical; disabled proxies are never silently used.

### 22. Apply SponsorBlock and dislike metadata

**What to build:** Viewer sees estimated dislike counts and optionally skips configured SponsorBlock segments.

**Blocked by:** 05, 08.

**Acceptance criteria:**
- [ ] Both services are enabled by default but independently disableable, with request behavior disclosed.
- [ ] Third-party service failures never block video playback; segments can be revisited.

### 23. Operate offline and search the local library

**What to build:** Viewer enables explicit offline mode or loses connectivity and can still browse/downloaded media and local search scopes.

**Blocked by:** 10, 18.

**Acceptance criteria:**
- [ ] Offline mode causes no unintended network requests; local history, playlists and downloads remain accessible.
- [ ] Restoring connectivity has explicit retry/reconnect behavior rather than forcing a restart.

### 24. Package and release the daily-driver Windows beta

**What to build:** Produce an installed and portable Windows 10/11 x64 beta with bundled dependencies, local diagnostics and optional application updates.

**Blocked by:** 03, 06, 08, 09, 11, 13, 14, 15, 16, 17, 19, 20, 21, 22, 23.

**Acceptance criteria:**
- [ ] Clean-system installer/portable smoke tests pass, with separate data directories, URL single-instance routing and no prerequisite downloads.
- [ ] Deterministic PR CI and separate live YouTube tests pass; local diagnostics are redacted; license notices and measured performance baselines are documented.

## Explicitly deferred work

The following remain in the approved long-term product scope but are **not prerequisites** for initial public beta; separate tickets should be added after feasibility and beta scope are validated: dedicated Shorts UI and configurable next/previous behavior; on-demand livestream chat and replay; full advanced playback tools (A–B repeat, frame step, audio normalization and expanded statistics); extensive download management/scheduling and auto-subscription downloads; deeper local recommendation behavior; broader importer coverage; optional public extension API; Linux and Windows ARM64 packaging; cross-device synchronization and optional Google-account capabilities.

## Ticket publishing gate

1. Confirm that each ticket is small enough for one fresh agent session and delivers a testable user behavior.
2. Confirm the blocking graph, especially the long final beta gate; split any ticket that proves too broad.
3. Configure the repository's preferred issue tracker and triage label vocabulary with the AI Hero setup workflow (GitHub Issues is the natural default for this GitHub repo).
4. Only then publish approved tickets in dependency order with actual issue references and the `ready-for-agent` label.
