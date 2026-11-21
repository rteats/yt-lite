# Issue #3 — extraction feasibility log

## Routes and scope

| Route | Implementation | Metadata | Streams | Captions | Chapters |
| --- | --- | --- | --- | --- | --- |
| `rust` | `rusty_ytdl` 0.7.4, Rust library | Yes | Format/URL list | Not exposed by this crate; `unknown` | Provided when source exposes them |
| `yt-dlp` | External `yt-dlp` command with `--dump-single-json` | Yes | Format/URL list | Manual and automatic tracks | Provided when source exposes them |
| `auto` | Rust, retry whole extraction with yt-dlp on failure or missing streams | Yes | Yes | Depends on winning provider | Depends on winning provider |

All live capabilities depend on upstream public video accessibility. No account/cookies/proxies are configured; restricted, paid, private, geo-blocked and DRM media are not bypassed.

## Deterministic checks

`tests/fixtures/` contains hand-reduced, nonsecret, **synthetic** provider payloads (schema examples, not evidence of successful live requests). `cargo test --all-targets` covers typed results, stream selection metadata, captions, chapters, missing-vs-unknown capabilities, invalid JSON, returned-ID mismatch, and video URL validation. CI runs offline on Ubuntu and Windows.

## Pending live validation (not claimed as complete)

On Windows 10 x64 with current `yt-dlp`, run each provider on:

1. Public single video with captions and chapters.
2. Public single video without chapters.
3. Live stream and Shorts video.
4. Unavailable or access-restricted video (record error codes, **not** signed URLs).

Record date, extractor version, pass/fail, metadata/format count, captions, chapters, runtime dependency, failure type, and whether fallback helped. Verify against actual video UI. Never commit full provider JSON containing signed URLs.

### Windows 10 live-test procedure (manual; not part of CI)

Build the feasibility harness and install a current **standalone Windows x64** `yt-dlp.exe`
from its official release, or supply a trusted local installation. The helper is
**not** bundled or downloaded by this prototype.

```powershell
cargo build --release
.\target\release\yt-lite-extraction-proof.exe "https://www.youtube.com/watch?v=dQw4w9WgXcQ" --provider rust
.\target\release\yt-lite-extraction-proof.exe "https://www.youtube.com/watch?v=dQw4w9WgXcQ" --provider yt-dlp --yt-dlp "C:\tools\yt-dlp.exe"
.\target\release\yt-lite-extraction-proof.exe "https://www.youtube.com/watch?v=dQw4w9WgXcQ" --provider auto --yt-dlp "C:\tools\yt-dlp.exe"
& "C:\tools\yt-dlp.exe" --version
```

Replace the sample URL with each of the four representative cases listed above;
use the same URL with `rust`, `yt-dlp` and `auto`. The default JSON report is
safe to share: it counts streams (including audio-only), caption tracks, and
chapters without exposing signed URLs. `unknown` and `unavailable` must not
be conflated. Note that `auto` selects one provider's *whole* result, so it may
lack captions even when the fallback could have provided them independently.

Record a compact table of real results (no stream URLs):

| Test date | Video type | Provider | Result/error code | Streams / video streams | Captions (manual/auto) | Chapters | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Not run | Public single video | rust / yt-dlp / auto | Pending | Pending | Pending | Pending | Use video with known captions + chapters |
| Not run | Public without chapters | rust / yt-dlp / auto | Pending | Pending | Pending | Pending | Verify absence versus unknown |
| Not run | Shorts and live | rust / yt-dlp / auto | Pending | Pending | Pending | Pending | Verify formats against YouTube UI |
| Not run | Unavailable/restricted | rust / yt-dlp / auto | Pending | Pending | Pending | Pending | Error codes only |

Include local Rust toolchain, `yt-dlp --version`, and whether each helper is
found without a manual install. Success requires **real nonempty playable stream
URLs** (check privately via `--raw-json` only when needed), not just metadata.
Do not post `--raw-json` output: it contains signed stream/caption URLs.

## Dependencies and provenance

- `rusty_ytdl` 0.7.4 is MIT OR Apache-2.0 per docs.rs. Its real extraction compatibility is **unverified**.
- `yt-dlp` is distributed under the Unlicense; recheck the official license and current release before packaging. The harness expects a separately supplied binary, performs no automatic download, and captures only provider JSON.
- All harness source code was written for yt-lite. The fixtures are synthetic.
- Final AGPLv3 licensing and binary redistribution obligations are separate issue #4.

## Known limits

- The Rust route cannot currently expose captions; it returns `unknown`.
- `auto` handles extraction failure/missing streams, not per-capability recovery or expired URLs (issue #22).
- Offline Windows/Linux Cargo checks passed in GitHub Actions. Live extraction, signed-URL validity, and playback integration have not been verified.
- The Slint/libmpv playback proof is issue #2, outside this harness.
