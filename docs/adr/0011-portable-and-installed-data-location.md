# Distinct portable and installed data locations

Installed yt-lite builds will store user data in appropriate Windows AppData locations, while portable builds will use a dedicated data directory alongside the portable application. Users may also configure a custom data directory. This keeps a portable installation genuinely transportable without requiring all installations to rely on a writable executable directory.

**Consequences:** The application must detect read-only or inaccessible locations, refuse unsafe partial writes, keep database migrations and backups reliable when relocating, and avoid silently combining two distinct installations' data. Application updates must preserve the selected data location.
