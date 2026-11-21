# Profile-owned offline library with deduplicated media

Each local profile will own its own offline-library entries, while multiple entries may reference the same stored media bytes on disk. Deleting one profile's entry must not remove bytes needed by another profile; when the final reference is deleted, unreferenced media can be reclaimed. This avoids cross-profile disclosure through library visibility without duplicating large video files.

**Consequences:** Media lifecycle must be explicitly managed, and downloaded items should preserve available video details, thumbnails, chapters, subtitles, and playback progress for offline use. Storage management should expose location, free-space checks, cleanup, and library moves without requiring separate media directories per profile.
