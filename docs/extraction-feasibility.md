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
deno --version
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

## First Windows PowerShell live result — 2026-11-21

A user ran the release build against a public watch URL. The build completed.
The original Rust probe returned one stream and marked captions unknown,
chapters unavailable, and duration null. The yt-dlp probe returned
`helper_missing` because the documentation's example path
`C:\tools\yt-dlp.exe` did not exist. `auto` used the Rust result because
it had a nonempty stream list; **yt-dlp fallback was not exercised**.

**Root cause confirmed for a subset of the results:** `rusty_ytdl` 0.7.4
uses `lengthSeconds`, `ownerChannelName`, `hasVideo`, and `hasAudio`
in its serialized schema, whereas the prototype parser expected different
keys. This caused duration to be dropped and stream audio/video flags
potentially misclassified. The fix is covered by
`tests/rust_provider_schema.rs` and its fixture based on the upstream
published structs. The user's first live summary predates this fix;
its stream classification and missing duration are **not valid evidence**
of limitations in the Rust upstream extractor.

Next: rebuild and repeat the Rust probe. Then install the helper and Deno,
run both extraction routes with real URLs, compare audio/video flags and
confirm the stream URL can actually be played. No raw signed URL data was
included in the supplied test log.

### Windows helper setup (rather than the illustrative C:\tools path)

```powershell
winget install --source winget --id yt-dlp.yt-dlp
winget install DenoLand.Deno
# Open a NEW PowerShell window so PATH changes take effect:
Get-Command yt-dlp, deno
yt-dlp --version
deno --version
# From the yt-lite repository root after rebuilding:
cargo build --release
$exe = ".\target\release\yt-lite-extraction-proof.exe"
$url = "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
& $exe $url --provider rust
& $exe $url --provider yt-dlp --yt-dlp yt-dlp
& $exe $url --provider auto --yt-dlp yt-dlp
```

The `winget` command is documented by the official yt-dlp installation
guide; Deno setup is documented in the official Deno installation guide.
If `Get-Command yt-dlp` fails, use the *actual installed* `yt-dlp.exe`
path in `--yt-dlp`. Never assume `C:\tools\yt-dlp.exe` exists.

## Dependencies and provenance

- `rusty_ytdl` 0.7.4 is MIT OR Apache-2.0 per docs.rs. Its real extraction compatibility is **unverified**.
- `yt-dlp` source is Unlicense, but its official PyInstaller Windows executable bundles additional dependencies and the **combined executable is GPLv3+** under its own release documentation. Audit this distinction, the EJS components, and any Deno/libmpv redistribution with the final application's AGPL obligations before packaging (issue #4). The harness neither downloads nor bundles them.
- YouTube's current challenge-solving path requires [yt-dlp-ejs](https://github.com/yt-dlp/ejs) and a supported JavaScript runtime. According to the [yt-dlp EJS setup guide](https://github.com/yt-dlp/yt-dlp/wiki/EJS), the official Windows `yt-dlp.exe` already embeds EJS scripts, while Deno (recommended, enabled by default) must be present separately on `PATH` or next to `yt-dlp.exe`. The referenced guide lists Deno minimum 2.3.0. Check `deno --version` during live validation; a missing runtime can mean incomplete stream availability even if metadata succeeds. Neither the harness nor yt-dlp's standalone executable bundles Deno here. Any future standalone release must decide how to bundle a compatible runtime and comply with its licenses (issue #4).
- All harness source code was written for yt-lite. The fixtures are synthetic.
- Final AGPLv3 licensing and binary redistribution obligations are separate issue #4.

## Known limits

- The Rust route cannot currently expose captions; it returns `unknown`.
- `auto` handles extraction failure/missing streams, not per-capability recovery or expired URLs (issue #22).
- Offline Windows/Linux Cargo checks passed in GitHub Actions. Live extraction, signed-URL validity, and playback integration have not been verified.
- The Slint/libmpv playback proof is issue #2, outside this harness.
