# One playback session shared across browsing contexts

yt-lite will allow optional independent browsing tabs, but maintain exactly one active playback session across tabs, watch pages, mini-player, fullscreen, and a detached player window. This avoids per-tab decoding and unnecessary memory usage while retaining navigation state. On restart, browsing state and playback position may be restored but playback never starts automatically.

**Consequences:** Tab navigation must not recreate the decoder or reset the queue. A normal click on another video replaces the active video; explicit queue actions enqueue; merely opening a new browsing tab does not interrupt playback. Switching local profiles saves progress and stops playback. Inactive tabs must remain resource-conscious.
