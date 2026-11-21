# Local-first SQLite state with local profiles

yt-lite will store user data locally in SQLite and provide import/export, with an option to add synchronization later rather than requiring accounts or servers now. Multiple local profiles will isolate subscriptions, history, playlists, playback progress, feed preferences, and offline-library visibility. This favors offline availability, privacy, and low idle overhead over built-in cross-device synchronization.

**Consequences:** A profile's library entries are private to that profile, while underlying downloaded media can be shared without duplication. Profile switching saves the previous profile's playback position and stops playback before loading the next profile. No login is necessary.
