"""Standalone cold-process vs warm yt-dlp Python API benchmark.

No media is downloaded. Only timing, format counts and fixed error categories
are printed. Use a disposable virtual environment and the same yt-dlp version
as your executable; do not pass cookies or credentials.
"""
import argparse
import json
import random
import statistics
import subprocess
import sys
import time
from urllib.parse import urlparse, parse_qs


def validate_url(url):
    parsed = urlparse(url)
    video = parse_qs(parsed.query).get("v", [""])[0]
    if (parsed.scheme != "https" or parsed.netloc not in ("www.youtube.com", "youtube.com")
            or parsed.path != "/watch" or len(video) != 11
            or not all(c.isascii() and (c.isalnum() or c in "_-") for c in video)):
        raise ValueError("Use a public HTTPS YouTube watch URL")
    return "https://www.youtube.com/watch?v=" + video


class SafeLogger:
    def __init__(self):
        self.reset()

    def reset(self):
        self.retries = self.resets = self.timeouts = self.solver_failures = 0

    def debug(self, message):
        pass

    def warning(self, message):
        message = message.lower()
        self.retries += int("retrying" in message)
        self.resets += int("connectionreseterror" in message or "unexpected_eof" in message)
        self.timeouts += int("timed out" in message)
        self.solver_failures += int("challenge solving failed" in message)

    def error(self, message):
        self.warning(message)


def summarize(info, logger, elapsed, ok):
    direct = [f for f in (info or {}).get("formats", [])
              if f.get("url") and f.get("protocol") in ("http", "https")]
    def has_format(n):
        return any(str(f.get("format_id", "")).split("-")[0] == n for f in direct)
    has_video = any(f.get("vcodec") not in (None, "none") for f in direct)
    has_audio = any(f.get("acodec") not in (None, "none") for f in direct)
    return dict(ms=round(elapsed * 1000), ok=ok, direct=len(direct),
                direct_av=has_video and has_audio,
                format399=has_format("399"), format251=has_format("251"),
                retries=logger.retries, resets=logger.resets, timeouts=logger.timeouts,
                solver_failures=logger.solver_failures)


def options(logger):
    return dict(quiet=True, verbose=False, noplaylist=True, skip_download=True,
                socket_timeout=2.5, extractor_args={"youtube": {
                    "player_js_variant": ["tv"], "skip": ["hls"]}},
                logger=logger)


def run_one(worker, logger, url):
    logger.reset()
    started = time.perf_counter()
    try:
        info = worker.extract_info(url, download=False)
        return summarize(info, logger, time.perf_counter() - started, bool(info))
    except Exception:
        return summarize(None, logger, time.perf_counter() - started, False)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("urls", nargs="+")
    parser.add_argument("--runs", type=int, default=5)
    parser.add_argument("--child", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args()
    if not 1 <= args.runs <= 30:
        parser.error("--runs must be 1..30")
    try:
        urls = [validate_url(url) for url in args.urls]
    except ValueError as exc:
        parser.error(str(exc))
    from yt_dlp import YoutubeDL
    from yt_dlp.version import __version__
    logger = SafeLogger()
    if args.child:
        with YoutubeDL(options(logger)) as worker:
            print(json.dumps(run_one(worker, logger, urls[0])))
        return
    print("yt-dlp", __version__, "Python", sys.version.split()[0], flush=True)
    samples = {"cold": [], "warm": []}
    init = time.perf_counter()
    with YoutubeDL(options(logger)) as worker:
        print("worker_init_ms", round((time.perf_counter() - init) * 1000), flush=True)
        rng = random.Random(7)
        for i in range(args.runs):
            url = urls[i % len(urls)]
            modes = ["cold", "warm"]
            rng.shuffle(modes)
            for mode in modes:
                if mode == "warm":
                    result = run_one(worker, logger, url)
                else:
                    started = time.perf_counter()
                    try:
                        proc = subprocess.run(
                            [sys.executable, __file__, "--child", url],
                            capture_output=True, text=True, timeout=60, check=True)
                        result = json.loads(proc.stdout)
                    except (subprocess.SubprocessError, ValueError) as exc:
                        # Do not reuse counters from the preceding warm sample.
                        cold_logger = SafeLogger()
                        cold_logger.timeouts = int(isinstance(exc, subprocess.TimeoutExpired))
                        result = summarize(None, cold_logger, 0, False)
                    result["ms"] = round((time.perf_counter() - started) * 1000)
                samples[mode].append(result)
                print(json.dumps(dict(run=i + 1, mode=mode, **result)), flush=True)
    for mode, rows in samples.items():
        good = [r["ms"] for r in rows if r["ok"] and r["direct_av"]]
        steady = [r["ms"] for r in rows[1:] if r["ok"] and r["direct_av"]]
        print(json.dumps(dict(mode=mode, direct_av_candidates=len(good), total=len(rows),
                              median_ms=statistics.median(good) if good else None,
                              median_after_first_ms=statistics.median(steady) if steady else None,
                              max_ms=max(good) if good else None)), flush=True)


if __name__ == "__main__":
    main()
