# yt-lite

yt-lite is a local-first desktop client for browsing and playing YouTube content without requiring a Google login.

## People and content

**Local profile**:
An identity inside yt-lite with its own locally managed subscriptions and feed. A local profile is not a Google account.
_Avoid_: Google account, login

**Subscription**:
A local decision to follow a YouTube channel and include its uploads in subscription views. It does not imply subscribing through a Google account.
_Avoid_: Account subscription

**Home feed**:
A configurable collection of content sections, such as recommendations, subscriptions, trending videos, and continue watching.

**Watch history entry**:
A local record of a viewed video and its viewing progress.

## Playback and downloads

**Playback session**:
The single active video playback state, retained when the player moves between the watch page, mini-player, detached window, and fullscreen.

**Playback queue**:
An ordered list of videos awaiting playback, managed independently of download jobs.
_Avoid_: Download queue

**Download job**:
A tracked request to save a video or audio item for offline use.
_Avoid_: Playback queue

**Offline library item**:
A deliberately downloaded, retained media item available for playback without fetching it again.

**Cache entry**:
Disposable, size-bounded stored content such as a thumbnail or video metadata, distinct from an offline library item.

## Data access

**Extraction provider**:
A selectable method of obtaining YouTube metadata or playable media information, such as direct Rust extraction, yt-dlp, or an optional Invidious/Piped backend.

**Fallback**:
An alternate extraction provider used when the preferred method fails, subject to the configured provider and privacy policy.
