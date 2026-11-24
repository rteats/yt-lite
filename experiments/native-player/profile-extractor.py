"""Optional request-level profiler for an installed yt-dlp Python package.

Unlike the player CLI trace, this can time actual calls. Diagnostic only:
private hooks can change upstream; no monkeypatching the standalone exe.
Never prints URLs, headers, response bodies, arguments or exception text.
"""
import argparse
import json
import re
import sys
import time
from functools import wraps
from urllib.parse import urlsplit

START = time.perf_counter()
NEXT = 0


def category(value):
    try:
        u = urlsplit(value if isinstance(value, str) else value.url)
        host, path = (u.hostname or "").lower(), u.path
        if host.endswith(".googlevideo.com"):
            return "media_cdn"
        if host in ("youtube.com", "www.youtube.com", "m.youtube.com", "youtubei.googleapis.com"):
            if path.startswith("/youtubei/v1/player"):
                return "player_api"
            if path.startswith("/s/player/"):
                return "player_code"
            if path == "/watch":
                return "watch_page"
            if path.startswith("/youtubei/"):
                return "innertube_other"
            return "youtube_other"
        if host in ("github.com", "raw.githubusercontent.com", "registry.npmjs.org"):
            return "component_source"
    except (AttributeError, ValueError, TypeError):
        pass
    return "other"


def emit(event, **fields):
    print(json.dumps({"t_ms": round((time.perf_counter() - START) * 1000, 2),
                      "event": event, **fields}), flush=True)


def timed(kind, function, args, kwargs, **fields):
    global NEXT
    NEXT += 1
    ident = NEXT
    started = time.perf_counter()
    emit(kind + "_begin", id=ident, **fields)
    ok = False
    try:
        result = function(*args, **kwargs)
        ok = True
        return result
    finally:
        emit(kind + "_end", id=ident,
             elapsed_ms=round((time.perf_counter() - started) * 1000, 2),
             returned=ok, **fields)


def run():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("video_id", nargs="?")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        assert category("https://www.youtube.com/youtubei/v1/player?secret=x") == "player_api"
        assert category("https://www.youtube.com/s/player/abc/base.js") == "player_code"
        assert category("https://x.googlevideo.com/videoplayback?secret=x") == "media_cdn"
        assert category("https://example.test/?secret=x") == "other"
        assert timed("fixture", lambda: 1, (), {}) == 1
        print("request profiler self-test passed")
        return 0
    if not args.video_id or not re.fullmatch(r"[A-Za-z0-9_-]{11}", args.video_id):
        parser.error("provide an 11-character public video ID")
    try:
        import yt_dlp
        from yt_dlp.version import __version__
        from yt_dlp.extractor.common import InfoExtractor
        from yt_dlp.utils import Popen
        original_open = yt_dlp.YoutubeDL.urlopen
        original_read = InfoExtractor._webpage_read_content
        original_run = Popen.run
    except (ImportError, AttributeError):
        emit("unavailable", reason="package_or_hooks_missing")
        return 2
    emit("python_import_done", version=__version__)

    @wraps(original_open)
    def urlopen(self, request, *a, **kw):
        return timed("http_open", original_open, (self, request, *a), kw,
                     category=category(request))

    @wraps(original_read)
    def read(self, response, request, *a, **kw):
        return timed("http_body_decode", original_read,
                     (self, response, request, *a), kw, category=category(request))

    def popen_run(cls, *a, **kw):
        # Do not print any command argument, not even its executable path.
        argv = a[0] if a and isinstance(a[0], (tuple, list)) else ()
        exe = str(argv[0]).replace("\\", "/").rsplit("/", 1)[-1].lower() if argv else ""
        tool = exe.removesuffix(".exe")
        if tool not in ("deno", "node", "bun", "qjs", "ejs", "quickjs", "ffmpeg", "ffprobe"):
            tool = "other"
        mode = "version_probe" if "--version" in argv or "--help" in argv else "execution"
        return timed("subprocess", original_run, a, kw, tool=tool, mode=mode)

    yt_dlp.YoutubeDL.urlopen = urlopen
    InfoExtractor._webpage_read_content = read
    Popen.run = classmethod(popen_run)

    class QuietLogger:
        def debug(self, msg): pass
        def info(self, msg): pass
        def warning(self, msg): emit("extract_warning")
        def error(self, msg): emit("extract_error")

    try:
        opts = {"quiet": True, "no_warnings": False, "logger": QuietLogger(),
                "noplaylist": True, "skip_download": True, "socket_timeout": 10}
        with yt_dlp.YoutubeDL(opts) as ydl:
            info = timed("extract_info", ydl.extract_info,
                         ("https://www.youtube.com/watch?v=" + args.video_id,),
                         {"download": False})
            clean = timed("sanitize_info", ydl.sanitize_info, (info,), {})
            raw = timed("json_encode", json.dumps, (clean,), {})
            emit("result", media=len(clean.get("formats") or []),
                 output_bytes=len(raw.encode("utf-8")))
        return 0
    except Exception:
        emit("failed")
        return 1


if __name__ == "__main__":
    sys.exit(run())
