# Independently verified extractor updates

yt-lite will allow its bundled extraction components to update separately from application releases, with optional update checks, user control, integrity/authenticity verification, and rollback to a known working version. YouTube changes can break playback or metadata access independently of the GUI, so requiring a complete application release for every extraction fix would harm reliability.

**Consequences:** Updates cannot silently replace unverified executables; shipped component versions and their provenance need to be tracked. Update security, compatibility checks, and rollback behavior must be tested before enabling this mechanism.
