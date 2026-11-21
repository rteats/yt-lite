# Local-first SQLite state with local profiles

yt-lite will store user data locally in SQLite and provide import/export, with an option to add synchronization later rather than requiring accounts or servers now. Multiple local profiles will isolate subscriptions, history, playlists, playback progress, and feed preferences. This favors offline availability, privacy, and low idle overhead over built-in cross-device synchronization.

**Consequences:** Downloaded media bytes may be shared between profiles to prevent duplicate storage, but the scope of each profile's offline-library visibility and the deletion rules still require a decision. No login is necessary.
