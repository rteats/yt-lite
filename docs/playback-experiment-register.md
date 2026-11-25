# Playback performance experiment register

Status at 2026-11-25, branch feat/youtube-search-07, issue [#30](https://github.com/rteats/yt-lite/issues/30). This is the measured-results register, distinct from the evolving [experiment instructions](extraction-latency.md). Historical observations are from user-supplied Windows logs and console output; raw logs are not checked in. Early values are transcribed from the previous investigation notes, not newly rerun. No signed CDN URLs, authentication headers or raw extractor JSON are retained here.

## Method

Windows native-player proof, Slint/femtovg + libmpv. Ryzen 7 7840HS/Radeon 780M, 16 GB installed RAM, 120 Hz display. Public reference video xu1wRfUHtKg (roughly 95 s). Synthetic control: 95 s H.264 640x360/30 fps GOP 90, AAC 48 kHz 128 kbps, two files. Loopback server binds 127.0.0.1. Default cache 8 s forward, 16 MiB packet budget, 4 MiB backwards, disk cache off; expanded 64/32 MiB. Network timeout 20 s. Normal timer 250 ms; benchmark 50 ms. Seek sequence 55%,55%,25%,25%,75%,75% with 2.5 s settling; report main-demuxer cache membership before each seek. Exact landing tolerance +/-350 ms. A/V readiness is observed software clocks, not audio or frame presentation measurements. Sample sizes small, no population/percentile claims. Separate stage times may not share a clock origin. Never silently drop failed attempts. Median is median of listed samples.

## Extraction

| ID | Input / comparison | Observations (ms) | Decision |
|---|---|---|---|
| E01 | Initial traced native extraction | 10116 total; retry gap 5022; JS solver launch to warnings ~1160; JSON parse ~2.8; later load to first audio ~2081 | Network retry and startup dominate; JSON parse not material |
| E02 | Standalone yt-dlp, three runs | 5403, 10287, 9293; Deno solver failure and HLS SSL EOF observed | Separate solver/HLS errors from playback |
| E03 | TV player variant, HLS on vs skipped | on 4819/4320/10750; skip 5095/4479/4039; 35 direct candidates, formats 399/251 | TV variant workaround and skip HLS opt-in; no isolated HLS speedup claim |
| E04 | 10 curl HTTP/1.1 page probes | all 200; median ~1.21 s, max 2.75 s | Intermittent 5 s failures not reproduced by curl |
| E05 | Randomized standard 10 s vs 2.5 s vs skip webpage | 10 s 9544/5075/4116; 2.5 s 4298/4425/3781; skip webpage failed 3/3 | Keep webpage; short timeout promising, first run no short-timeout event |
| E06 | Python yt-dlp 2026.10.26, Python 3.14.7, same video five times | cold 6174/4437/4374/8164/6913; reused 5692/2523/2650/2501/2711; init 93; 5/5 both; retry/timeout 1 each | Median 6174 -> 2650 (-57.1%); warm excluding first median 2586.5; persistent worker opt-in |
| E07 | Integrated persistent worker, cold startup two runs | 3893/3247; extract 3121/2725; wire minus extract 17/13; parse 3590/3013 us; no errors | IPC not material; live warm multi-video reuse still unvalidated |
| E08 | Successful combined optimized remote session | extraction 3169; media to audio 1509; no fallback, 1080p 399/251 | 4.678 s sum of measured stages, not complete wall clock; only one session |

E01-E06 implementation: 1d39094, 5d2488c. Persistent integration: f1f3614, b74d891. E07 logs baseline-av.log and preattach-av.log. E08 transport-remote.log, repeated later but not a second independent trial. Repeated same video does not establish general performance; persistent Python runtime packaging and fresh multi-video reuse remain pending.

## Playback startup and seeking

| ID | Comparison | Observations | Decision |
|---|---|---|---|
| P01 | Serial vs pre-attached 399/251 | first audio 2723 vs 1817 ms; FILE_LOADED 1500 vs 1503; PTS 0.927 vs 0.006 s | 906 ms improvement in one pair, startup not necessarily concurrent; retain experiment |
| P02 | Uncached remote exact seek | 45 us dispatch, 1804 ms recovery; earlier 1740 ms | Dispatch insignificant |
| P03 | Remote exact run 1 | A/V readiness 6751/668/720/566/10770/503 ms; landing errors +0.714/+0.697/+29.095/+29.079/+0.022/+5.470 s | Only 1/6 within later tolerance; old verdict was false |
| P04 | Remote exact run 2 | 860/320/2773/350/505/158 ms, all within 81 ms; outside median 860, inside 320 | Retain exact; remote variability unresolved |
| P05 | Keyframes run 1 | Startup failed before seek; serial retry 786 ms to first audio | Invalid seek comparison |
| P06 | Keyframes run 2 | restart 200/53/710/303/663/48; A/V 1114/2920/711/3273/664/818; early landings ~0.73–2.97 s | Reject global keyframe policy |
| P07 | Default vs expanded cache remote | extraction 3618/5316; failed preattach 7399/10367; failed serial 7196/6991; muxed 1074/1044 | Neither reached seeking; inconclusive cache comparison |
| P08 | Synthetic local split reference | 68/68/322/251/326/50 ms, 6/6; 624 ms synchronous pause getter, 796 ms timer gap | UI blocking independent of network |
| P09 | Same fixture synchronous vs observed | sync 51/68/336/244/303/247; observed 70/69/101/198/52/51 | median 245.5 -> 69.5 (-71.7%), 6/6 each |
| P10 | Normal loopback HTTP, observed | 100/70/145/141/151/141; outside median 145, inside 141 | 6/6 correct |
| P11 | 500 ms delay loopback HTTP | 1130/70/1138/142/1056/141; outside 1130, inside 141 | Network response delay causally affects uncached seeks |
| P12 | Timestamped repeat of P11 | 1077/69/1055/205/1140/100; outside 1077, inside 100; 6/6 | Confirm sequential response-header waits |

P01 9d74d93; P03-P06 6fd84d2, 1ec1627; P07/P08 1ec1627; P09 902894e, 082516c; P10-P11 fcdcfa5, c7d2906; P12 d2a423c, ae2f5ae.
Evidence filenames: seek-exact.log, seek-exact-2.log, seek-keyframes.log, seek-keyframes-2.log; seek-cache-default.log, seek-cache-expanded.log, seek-local-split.log; state-sync.log, state-observed.log; transport-local-http.log, transport-delayed-http.log; concurrency-player.log, concurrency-server.log. P09 sync 8 getter warnings >=10 ms, 7 >=100 ms, 7 UI ticks >=20 ms, max getter 480.845 ms, max tick 685.392 ms, one 788 ms timer gap; observed zero reported warnings at those thresholds, not proof of zero frame drops. P12 no reported stalls; cache membership refers to main demuxer only.

### P12 request timeline (server-relative ms)

| Phase | Video request -> headers | Audio request -> headers | Inter-header dependency |
|---|---|---|---|
| Startup probe 1 | 2134 -> 2635 | — | |
| Startup probe 2 | 2639 -> 3140 | — | |
| Startup audio probes | — | 3141 -> 3642; 3645 -> 4145 | |
| Startup data | 4260 -> 4761 | 4259 -> 4761 | concurrent start, 1 ms apart |
| Seek 1 | 7926 -> 8427 | 8464 -> 8965 | 37 ms after video headers |
| Seek 3 | 14248 -> 14748 | 14793 -> 15294 | 45 ms |
| Seek 5 | 20733 -> 21234 | 21289 -> 21790 | 55 ms |

All three uncached pairs serialize request-to-header waits. Body transfer can overlap; final duration includes backpressure. Startup first four probes serialize, last pair overlaps. Exact internal code dependency remains to be established. Synthetic MP4 metadata at end triggers tail reads; production YouTube formats may differ. Server and player trace origins differ. No custom transport warranted before testing existing mpv options.

## Recovery

| ID | Condition | Result | Decision |
|---|---|---|---|
| R01 | HTTP 503, old seek fixture | FileLoaded/PlaybackRestart with video only; no second audio request; closed before verdict | Require A/V, not just FILE_LOADED |
| R02 | Transient recovery, bad PowerShell paste | server not reliably started; general failures 4.006 s and ~4 s; Stop 8.011 s | Invalid transient test; preserve as invalid |
| R03 | Persistent 503, real recovery branch, synthetic resolver | first load t282, 503 t547, Refresh t5613, second load t5863, 503 t6143, Stop t11209; 10.926 s elapsed | Pass bounded termination; 2 audio 503s in server log |
| R04 | Transient 503 rerun | first load t369, 503/audio_error t634, Refresh t5699, second load t5951, FileLoaded t6202, ready t6727; total 6357 ms, second 776 ms; 48 kHz output | Pass player-observed recovery; server log only ready announcement, no independent request verification |

R03 evidence recovery-persistent.log, recovery-persistent-server.log. R04 recovery-transient(1).log and recovery-transient-server.log. At R04 ready position 0.233 s, audio 0.180 s; later 10.90/10.85 s. No audible-output confirmation from logs alone. R04 ~5065 ms elapsed from explicit error to Refresh; R03 ~5066 ms first and second. Projected ~1.3 s transient and <2 s persistent under same conditions if terminal failure recognized immediately; **not measured**. Neither fixture tests fresh YouTube extraction or actual muxed fallback. R03/R04 on ae2f5ae. Latest Windows tests: at least 35 Rust passes, one ignored, missing one result line; four Python passes. No raw logs committed.

## Decisions and unresolved work

Keep persistent extraction, exact seeks, observed-state and preattachment available; all remain opt-in until broader remote validation and packaging. Reject skip-webpage and default keyframe seeking. Expanded cache inconclusive. Real CDN error cause unresolved. Next experiment: explicit terminal audio-open signal, bounded retry without watchdog delay; compare mpv EDL unified cache vs external audio and front-of-file MP4 metadata vs original tail placement. Preserve existing watchdog for unknown failures. Evaluate startup and seek separately; don't compare mismatched cache membership. No unmeasured gain presented as fact. Track every new experiment here, including failed or inconclusive runs, source filenames, exact options, raw sample measurements, verdict and limitations. See issue #30 for historical commit notes.


## Planned experiments after R04 (not yet measured)

- R05: repeat transient and persistent 503 with exact current audio-open error shortcut. Expected no five-second wait if DLL emits matching terminal message; compare Refresh/Stop/ready times, keep watchdog for ambiguous cases.
- P13: external audio vs EDL loopback, original fixtures, 500 ms delay. Test if unified composition avoids sequential response waits; preserve all exact landings and A/V clocks. EDL may change cache semantics or fail entirely.
- P14: original vs faststart stream-copy MP4/M4A, external audio, 500 ms delay. Test if metadata relocation removes tail requests and startup round trips; compare request count and startup duration. No production media conversion.

Sources and reproducible instructions in extraction-latency.md. No native results claimed until Windows validation.


## Results: R05, P13, P14 (Windows, uploaded yt-lite.zip)

Implementation baseline 9efe0c7 (fast terminal recovery/EDL 9c8a139, experiment runner 9efe0c7). Archive contains all 15 network-{transient,persistent,external,edl,faststart}-{player,server,server-stderr}.log files. Five server stderr files are empty. Direct log capture worked; some concurrent server lines concatenate, but request IDs and completion fields remain readable. HEAD readiness probe is excluded from all playback GET counts. Each mode tested once; no separate cargo test output in archive. Native executable ran all modes; Rust test suite not reconfirmed.

### R05: immediate terminal audio-open failure recovery

| Scenario | Previous elapsed ms | New elapsed ms | Result |
|---|---:|---:|---|
| Transient audio 503 | 6357 | 1282 | Refresh once, synchronized A/V ready |
| Persistent audio 503 | 10926 | 788 | Refresh once, Stop; no false readiness |

Transient: load t397; explicit terminal audio-open failure t669; Refresh t670 (reported elapsed 272); second load t917; FILE_LOADED t1169; PlaybackRestart and readiness t1679; second attempt first audio PTS 761ms (reported). Decoder/output 48000 Hz; later video 10.80s/audio 10.751s. Server attempt 1 audio GET id4 status503; attempt 2 audio GET ids8,9,10 status206; no third attempt. Persistent: load t280; failure and Refresh t551 (271ms); second load t799; failure and Stop t1069 (reported total788ms); server audio GET ids4,8 status503, distinct generations. Neither persistent attempt declared ready. Elapsed reductions 79.8% and 92.8% respectively. Both provisional targets met. No slow-call/tick/gap/overflow markers in fault runs. Audio clock evidence is not audible-output verification. Synthetic resolution, not fresh YouTube extraction/muxed fallback. Retain opt-in shortcut; bundled DLL emits matching message in these cases.

### P13/P14: 500 ms delay, original external audio vs original EDL vs faststart external audio

All modes observed state, exact seek, same six targets and cache membership alternating outside/inside, 16/4 MiB and 8s forward. Faststart stream-copy remux, same file sizes (video 23316975 bytes, audio 1541755 bytes), different layout. One run each. Times relative to new http_fixture_load_requested marker.

| Mode | FileLoaded ms | PlaybackRestart ms | A/V ready samples ms | Outside median | Inside median | Correct | Startup GETs | Total GETs |
|---|---:|---:|---|---:|---:|---:|---:|---:|
| External original | 2151 | 2887 | 1056/70/1139/133/1059/140 | 1059 | 133 | 6/6 | 6 | 12 |
| EDL original | 2141 | 3356 | 1135/67/1131/100/1121/135 | 1131 | 100 | 6/6 | 6 | 12 |
| External faststart | 1130 | 1333 | 1078/68/1055/140/1060/52 | 1060 | 68 | 6/6 | 2 | 8 |

EDL: no seek improvement; outside median +72ms, startup restart +469ms. External original startup data pair request starts audio3031/video3033 (2ms apart), headers3533/3534; EDL corresponding audio3049 headers3551, video3554 headers4055, no header-wait overlap. Faststart eliminates tail/index and return-to-data GETs in this control, startup GETs 6 -> 2, observed restart -1554ms (-53.8%). No equivalent improvement claimed for remote formats. Cached latency small samples, not enough evidence of a systematic benefit. EDL not promoted.

Uncached request times (server-relative ms; exclude HEAD, body transfer duration not RTT):

| Mode | Seek | Video start -> headers | Audio start -> headers | Gap |
|---|---:|---|---|---:|
| External | 1 | 7276 -> 7776 | 7813 -> 8315 | 37 |
| External | 3 | 13597 -> 14098 | 14151 -> 14652 | 53 |
| External | 5 | 20083 -> 20585 | 20630 -> 21132 | 45 |
| EDL | 1 | 6820 -> 7322 | 7326 -> 7829 | 4 |
| EDL | 3 | 13244 -> 13746 | 13749 -> 14251 | 3 |
| EDL | 5 | 19698 -> 20200 | 20203 -> 20704 | 3 |
| Faststart | 1 | 4765 -> 5267 | 5303 -> 5804 | 36 |
| Faststart | 3 | 11090 -> 11592 | 11638 -> 12139 | 46 |
| Faststart | 5 | 17505 -> 18008 | 18054 -> 18555 | 46 |

All nine pairs serialize header waits. EDL changes cache composition, not request dependency. One 1440ms UI timer gap in external-original run at t6359 before first seek, no accompanying mpv_slow_call or ui_tick_slow warnings. EDL/faststart no reported stalls; gap is unresolved and cannot be attributed to getters or network. All 18 landings correct. No server stderr in any run.

### Next decision

No more broad EDL/cache flag experiments without evidence. Investigate mpv demuxer seek implementation and independent per-track fetch scheduling before selecting a concurrency architecture. A transport-layer cache may reduce latency if it can predict required byte ranges, but merely replacing HTTP transport cannot remove serial demand. Consider separately whether lightweight bounded audio-only buffering could remove one request wait; measure bandwidth/memory and long-video/seek correctness, do not download full video or make a full-audio download the default. Prefer a limited controlled proof before integration. Consolidation of validated optimizations, remote multi-video/warm worker validation, packaging and unexplained timer gap remain pending. No code/default changes from results review.


### Windows test confirmation

After R05/P13/P14 the user supplied full Cargo output: 39 passed, 0 failed, 1 ignored (DLL-dependent stale audio reply test); transport_recovery 8/8. Windows compile/test build 16.65 s. The earlier statement that the suite was unconfirmed is superseded.

## P15 planned: bounded audio-only prefetch control

Hypothesis: if external audio bytes are available locally, uncached seek needs only the video response wait; startup need not wait for a sequential audio origin fetch. Upstream mpv player/playloop.c queues main then selected external demuxer seeks; demux/demux.c demux_seek queues work and signals the demux thread, not necessarily synchronously waiting for HTTP. In player/loadfile.c external demuxers open using common mpctx->global and use the regular demux thread; documented cache-secs and demuxer-max-bytes configure packet read-ahead, not a separate readily exposed audio-only budget. Sources: https://github.com/mpv-player/mpv/blob/master/player/playloop.c ; https://github.com/mpv-player/mpv/blob/master/demux/demux.c ; https://github.com/mpv-player/mpv/blob/master/player/loadfile.c ; https://mpv.io/manual/stable/#cache . These are upstream master, not proven identical to bundled DLL; observed request serialization does not establish the exact lock/dependency.

Use the same original fixture and 500 ms delay, three modes External, BufferedAudio (4 MiB payload cap), BufferRejected (64 KiB cap). On first normal video GET, prefetch small synthetic audio in a worker using a real loopback HTTP range request to fixed /prefetch/audio.m4a; validate status, exact content range/length, retain complete bytes in memory only if within cap. The fixture knows immutable file size up front; oversized audio skips prefetch entirely. Subsequent audio range requests use retained bytes and bypass artificial upstream delay, video always uses ordinary delayed file retrieval. HEAD readiness does not start prefetch. If audio arrives before prefetch ready, wait at most 3 s then use ordinary path if absent. No fault injection or generation paths combined. This is a controlled simulated audio cache, not a production CDN proxy or independent packet-cache configuration. Full short audio is prefetched only in diagnostic mode, no video prefetch, no new Rust code or production defaults. Payload cap is not total Python RSS; real unknown-size/CDN validation, bandwidth budgeting and cancellation remain future work.

Expected: BufferedAudio 6/6 exact landings and outside median near 500–700ms vs ~1059ms, prefetch ready before audio demand if probing remains sequential, one prefetch origin GET, local audio hit labels, video request latency unchanged; BufferRejected behaves like baseline and records over_budget, no prefetch GET. Compare total startup and origin bytes, not just seek latency. If no seek gain despite audio hits, investigate decode or synchronization instead. If gain, assess bounded chunk cache and known-length policy for remote use before implementing production transport. All outcomes pending Windows validation.


### P15 preliminary HTTP-only validation (chat container, not mpv)

All five range-fixture unittest cases passed in 2.014s, including the two new cache cases. Test files checked against committed Git blobs (audio_prefetch.py 9010d52, range_fixture.py cf6b93c, test_range_fixture.py 9d8b1ff). Separate HTTP client control: synthetic 1,541,755-byte audio payload and 100,000-byte video payload; first video GET then two audio GETs, each 100 bytes, with 500ms injected response delay. Times measured from client request to response headers:

| Cap | Video | First audio | Subsequent audio | Prefetch |
|---|---:|---:|---:|---|
| Disabled | 502ms | 502ms | 501ms | None |
| 4MiB | 502ms | 2ms | 1ms | 1,541,755 bytes in 503ms, starts alongside video |
| 64KiB | 502ms | 501ms | 501ms | Rejected before fetch |

Range responses returned 206 and exact requested length. Demonstrates simulated upstream wait can be hidden and cap enforced; not a media seek result, not an estimate of real bandwidth (control requests only 100 bytes; buffering transfers substantially more), and not a Windows/Rust/remote integration test. Need actual mpv startup/seek timings from three runner cases before deciding on production design.


### P15 Windows playback results (2026-11-25, yt-lite(1).zip)

Archive contains all nine network-{external,bufferedaudio,bufferrejected}-{player,server,server-stderr}.log files. Runner baseline e7136f8/eec3297; player unchanged since 9c8a139. Three independent sessions, 500ms per origin response, same original 23,316,975-byte video and 1,541,755-byte audio, external audio, observed state, exact benchmark. Each completed six seeks, alternating main cache outside/inside, no incorrect landings. All server stderr files empty, no reported slow getter/tick/timer gap/queue overflow/event budget warnings. One run per mode.

| Metric | External | BufferedAudio 4MiB | BufferRejected 64KiB |
|---|---:|---:|---:|
| FileLoaded ms | 2126 | 1142 | 2152 |
| PlaybackRestart ms | 3262 | 1809 | 2835 |
| A/V ready samples ms | 1055/73/1136/100/1133/133 | 611/67/645/144/631/144 | 1058/70/1117/140/1112/132 |
| Outside median ms | 1133 | 631 | 1112 |
| Inside median ms | 100 | 144 | 132 |
| Max observed landing error ms | 116.7 | 116.7 | 116.7 |
| Correct | 6/6 | 6/6 | 6/6 |
| Video origin GETs | 6 | 6 | 6 |
| Audio origin GETs | 6 | 1 | 6 |
| Audio cache-hit GETs | 0 | 6 | 0 |
| Video origin bytes written | 34050407 | 34115943 | 34836839 |
| Audio origin bytes written | 4155196 | 1541755 | 4286268 |
| Audio cache-hit bytes written | 0 | 4482876 | 0 |

Playback startup measured from http_fixture_load_requested; restart is not independently verified audible output. Uncached median -502ms (-44.3%); observed startup restart -1453ms (-44.5%). Cached median +44ms, too few samples to establish systematic regression. All targets met except no strong evidence of equal cached latency. HEAD excluded from GET counts. Server sent counters count successful body writes, not network wire bytes, and may include overlapping/duplicate data or bytes not consumed before disconnect.

Prefetch first video GET t912, prefetch begin t916, internal audio GET t919, video headers t1417, prefetch audio headers t1422, complete/ready t1424 (508ms, exactly 1541755 retained payload bytes). First player audio GET t1924 (500ms after cache ready). Six player audio GETs all hits. Rejected case logs over_budget size1541755 cap65536 at t921; no internal prefetch GET; all six audio requests miss with ordinary delay.

| Buffered uncached seek | Video start -> headers (server ms) | Audio start -> headers (server ms) | Player A/V ready ms |
|---|---|---|---:|
| 1 | 5268 -> 5769 | 5806 -> 5807 | 611 |
| 3 | 11175 -> 11678 | 11735 -> 11736 | 645 |
| 5 | 17179 -> 17680 | 17728 -> 17729 | 631 |

Video/audio request initiation still sequential, second origin wait removed by serving cached bytes. In this repeated-seek synthetic workload, origin audio bytes written fell from 4155196 to 1541755 (-62.9%); not a general bandwidth saving guarantee: full-track prefetch may waste bandwidth for users who stop early and may compete with video on real constrained links. Startup buffer payload about 1.47MiB, cap4MiB; no measured total RSS. Long or unknown-size audio and cache miss on real CDN not covered. No new Cargo output; previous 39 pass/1 ignored still latest Rust evidence.

Decision: P15 passes controlled playback proof; investigate a production-capable opt-in audio cache, not unconditional full downloads. Prefer known size and small eligible direct HTTP audio, concurrent background prefetch with strict cap, timeout/cancellation/session isolation, ordinary streaming on miss, never delay initial playback waiting for prefetch beyond ordinary request need. Compare remote same-video alternating controls and diverse durations, bytes, first A/V readiness, exact seek landing, cache hit rate, RSS, failure/rapid replacement. Preserve 1080p and current recovery path; do not modify video transport. Need design/test seam for actual remote range handling before integration. No player/default change in this review.


## P16 planned: remote bounded audio cache

P15 established that available audio bytes remove the second origin wait. The extractor Stream schema currently omits exact filesize; use only known positive clen on direct HTTPS Googlevideo /videoplayback URLs, with actual response validation, not approximate bitrate or unknown size. Opt-in --audio-cache requires preattachment; no default/format changes. Local capability endpoint is per session, loopback only, random 128-bit token, no arbitrary target or proxy request. Starts speculative <=4MiB complete audio fetch during video initialization, serves complete cached byte ranges when ready; first demand while pending/failure permanently bypasses using 307 and cancels fetch, does not wait. Rejected size/protocol/origin uses direct URL immediately. One process-wide cache lease, 4 connection cap, 8s fetch timeout, cancellation on replacement/stop, no disk or video download. This is an explicitly approved small-audio-only experimental prefetch, not a required full-file download before playback (issue #30 prohibits full-file pre-download in normal path). Real CDN and libmpv redirect compatibility unvalidated; if first demand often wins, consider bounded partial range caching only after evidence. Local endpoint and signed origin never printed; existing sanitized diagnostics retained. Test actual HTTP seam plus eligibility/ranges; Windows build and A/B pending. Record actual cache-hit rate, startup and seeks, longer bypass, rapid switching and process RSS; compare at least two public videos alternating controls. No claims of remote speedup yet.

P16 implementation commits: 73a5537 adds remote cache and tests; follow-up limits experimental entry to one attempt per explicitly selected watch and provides remote A/B runner. Automatic provider/transport retries use original direct audio, even if lease already retired. No Cargo or Windows PowerShell execution available in chat; build and all new test results pending. The expected success condition is an actual audio_cache_end status=ready followed by audio_cache_request result=hit, not merely cache flag present. Bypass is valid and may be frequent on fast video startup or slow audio fetch; measure rather than claim a gain. Local HTTP redirect behavior and total memory remain validation dependencies.


### P16 Windows remote validation (yt-lite(2).zip, implementation 8f3f6cc)

Full Cargo output supplied: 42 passed, zero failed, one ignored (DLL-dependent test); three audio_cache unit tests passed in 90ms. Native player executed five sessions. Four benchmark sessions use same public reference video xu1wRfUHtKg, formats 399/251, exact seek, observed state, persistent extraction and preattached audio. ABBA chronological order supplied by runner instructions, not independently timestamped across logs. No second benchmark video; interactive second video uses 399/251-11. No RSS or transferred origin byte measurements.

| Metric | ref1 baseline | ref1 cache | ref2 baseline | ref2 cache |
|---|---:|---:|---:|---:|
| Persistent total ms | 3562 | 3261 | 3379 | 2948 |
| Extract ms | 2648 | 2706 | 2790 | 2329 |
| Cold worker queue ms | 896 | 537 | 572 | 593 |
| FILE_LOADED ms | 804 | 602 | 807 | 701 |
| First audio PTS since load ms | 1468 | 852 | 1050 | 926 |
| Completed seek observations | 0 | 2 | 6 | 4 |
| Correct landings | 0 | 2 | 2 | 4 |
| Terminal verdict | seek_timeout sample1 | seek_timeout sample3 | incorrect_landings (4) | seek_timeout sample5 |
| Cache hits | n/a | 0 | n/a | 0 |

Both experimental short-video sessions eligible (clen=1460954 bytes). ref1 cache begin t2978, demand t3257 (+279ms), cancelled t3285 (+306ms); ref2 begin t2636, demand t2967 (+331ms), cancelled t3044 (+407ms). Interactive begin t3004, demand t3452 (+448ms), cancelled t3511 (+507ms). All 3/3 eligible prefetches cancelled before ready. All subsequent requests redirected (3/4/12 across those sessions, 19 total), no hits. Initial audio and later correct seeks show redirect can work, not proof of identical request behavior or reliability to direct origin. No evidence of cache-derived startup or seek acceleration. Full prefetch cannot finish within observed 279–448ms demand window; actual progress/body bytes and reason for slow fetch unknown because cancellation interrupts it. Do not raise cap or insert unconditional startup waiting based on this result.

Seek observations (ms, first restart / A/V ready where available). '-' not attempted; timeout is 25s terminal, not a successful latency sample.

| Sample | ref1 baseline | ref1 cache | ref2 baseline | ref2 cache |
|---|---|---|---|---|
| 1 | 5028 / timeout25010 | 435 / 501 OK | 6181 / 6257 wrong +0.747s | 515 / 515 OK |
| 2 | - | 305 / 360 OK | 609 / 609 wrong +0.730s | 416 / 479 OK |
| 3 | - | 5025 / timeout25035 | 608 / 657 wrong +29.095s | 2508 / 3617 OK |
| 4 | - | - | 605 / 682 wrong +29.095s | 8089 / 9194 OK |
| 5 | - | - | 203 / 267 OK | 5045 / timeout25045 |
| 6 | - | - | 160 / 220 OK | - |

Main cache membership differs: ref2 baseline samples 3/4 inside, ref2 cache sample4 outside. Do not pool as matched cache A/B; wrong landings and timeouts invalidate performance comparison. Three timeout sessions show video progression with absent audio PTS and explicit audio clock stalled warning at 56.1/27.3/74.7s respectively. Restart around 5s in all three, no explicit HTTP/transport error classification in those logs. ref2 baseline wrong target 23.638 lands 52.733 twice, recurrence of P03. This is a remote seek reliability blocker present without audio cache, not established to be caused by redirect or CDN. Actual silence unverified; output rate still 48kHz does not guarantee audio continuity. Need distinguish mpv observed-state/event freshness vs actual wrong playback and demux/network failure. No slow getter/tick/timer-gap markers across four benchmark sessions.

Interactive: initial reference video startup 1852ms from dispatch to synchronized readiness. Multiple seek restart events without audio PTS, some recover later; user changed controls so not a controlled seek benchmark. One synchronous video-margin setter 42598us at replacement, no reported slow UI tick/gap. Session retired t64603 on replacement. Next persistent worker reused=true queue0 total2014ms extract1994ms (first interactive cold total3148ms extract2565ms queue565ms), first live reuse evidence on different video, not a controlled same-video speedup. Longer second audio rejected over_budget, startup ready1514ms. No automatic startup recovery actions. No proof of audible output or full long-video completion.

Decision: P16 fails cache-effectiveness criterion (0/3 hits), passes offline tests and observed eligibility/redirect/retire behavior. Leave opt-in, do not promote. Prioritize remote exact-seek reliability: reproduce direct baseline audio disappearance and incorrect landing with deterministic range fault after initial playback, check observation freshness against independently sampled mpv state outside hot UI thread, add bounded post-start seek failure recovery preserving target and pause intent only after evidence. For further cache work, first measure prefetch header/bytes/progress without stalling playback, evaluate continuing an independent background fetch or demand-driven bounded range cache; beware duplicated origin traffic and extra redirect per seek. Do not commit to more complex proxy until reliability and bandwidth tradeoffs understood. No production code/default changes from this review.


## P17 planned: post-start seek fault and observation cross-check

P16 had three 25s timeouts with absent audio PTS and one session 4/6 incorrect landings. Need a red-capable local playback reproducer before recovery changes. H1 real audio range error predicts post-start 503 can reproduce absent audio or timeout; H2 slow audio response predicts 5s delay causes similar restart/audio lag; H3 stale/coalesced observer predicts fresh async reads disagree materially with observed values after accounting for reply delay and playback advancement. If fresh readings agree with incorrect positions, investigate demux seek rather than treating it as a UI snapshot bug. New SeekControl/SeekAudioOnce/SeekAudioAlways/SeekAudioDelay cases explicitly arm on first benchmark dispatch; no startup injection, fixed paths, no video changes, 500ms ordinary delay. One extra 5s delay or 503 once/always. Compare server fault time to seek/restart and A/V. Fresh reads are independent of subscription cache, not independent hardware measurement. Async replies tagged sample/phase and bounded one batch; no synchronous getter fallback. Remote SeekProbe without cache repeats baseline. Keep incorrect/timeout results; no automatic seek recovery until fault reproduction and observation evidence. Python test added at HTTP seam; actual playback loop requires Windows. No new production defaults.
