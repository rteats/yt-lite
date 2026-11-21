# Single running instance for a data directory

yt-lite will route an external YouTube URL or second launch to an already-running instance rather than opening a second independent player and database owner. This preserves the one-playback-session model, avoids duplicate resources, and prevents unsafe simultaneous writes to the same local profile database.

**Consequences:** Launch requests should focus the existing window and open URLs in a browsing tab, without restarting current playback. Distinct portable or custom data directories must not accidentally share storage locks; the precise single-instance and interprocess-communication implementation must be tested on Windows.
