"""Loopback-only HTTP range fixture. Serves two explicit synthetic media files."""
import argparse
import re
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from audio_prefetch import AudioPrefetch

def byte_range(header, size):
    if header is None:
        return 0, size - 1, 200
    m = re.fullmatch(r"bytes=(\d*)-(\d*)", header)
    if not m or not (m[1] or m[2]):
        raise ValueError("invalid range")
    if not m[1]:
        suffix = int(m[2])
        if suffix <= 0:
            raise ValueError("invalid suffix")
        return max(0, size - suffix), size - 1, 206
    start = int(m[1])
    end = min(size - 1, int(m[2])) if m[2] else size - 1
    if start >= size or start > end:
        raise ValueError("unsatisfiable")
    return start, end, 206

def make_handler(files, delay_ms=0, fail_once=None, fail_always=None, audio_buffer_bytes=0, seek_marker=None, seek_fault=None):
    if (seek_marker is None) != (seek_fault is None) or seek_fault not in (None, "once", "always", "delay"):
        raise ValueError("invalid seek fault configuration")
    if seek_fault and (fail_once or fail_always or audio_buffer_bytes):
        raise ValueError("seek fault must be isolated")
    if not 0 <= audio_buffer_bytes <= 4 * 1024 * 1024:
        raise ValueError("invalid audio buffer cap")
    if audio_buffer_bytes and (fail_once or fail_always):
        raise ValueError("buffer experiment must not be combined with fault injection")
    lock = threading.Lock()
    counter = 0
    failed = False
    seek_injected = False
    origin = time.monotonic_ns()

    def now_ms():
        return (time.monotonic_ns() - origin) // 1_000_000

    prefetch = AudioPrefetch(audio_buffer_bytes, files["audio"].stat().st_size, now_ms)

    class Handler(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def log_message(self, *args):
            pass  # do not print arbitrary request paths/headers

        def handle(self):
            # Windows may abort/reset an idle keep-alive connection, outside
            # serve_media. Expected client disconnects are not server crashes.
            try:
                super().handle()
            except (ConnectionError, TimeoutError):
                print(f"range-fixture connection_closed t_ms={now_ms()}", flush=True)

        def do_HEAD(self):
            self.serve_media(False)

        def do_GET(self):
            self.serve_media(True)

        def serve_media(self, body):
            nonlocal counter, failed, seek_injected
            start_time = time.monotonic()
            started_ms = now_ms()
            match = re.fullmatch(r"/(?:attempt/([12])/)?(video\.mp4|audio\.m4a)", self.path)
            internal = audio_buffer_bytes > 0 and self.path == "/prefetch/audio.m4a"
            kind = "audio" if internal else (("video" if match[2] == "video.mp4" else "audio") if match else None)
            attempt = "prefetch" if internal else ((match[1] or "0") if match else "unknown")
            with lock:
                counter += 1
                rid = counter
                inject = body and kind is not None and (kind == fail_always or (kind == fail_once and not failed))
                if inject:
                    failed = True
                seek_action = None
                # Only new audio GETs after the player explicitly arms the
                # first seek. Existing response bodies are never modified.
                if body and kind == "audio" and attempt == "0" and seek_marker is not None and seek_marker.is_file():
                    if seek_fault == "always" or not seek_injected:
                        seek_action = seek_fault
                        seek_injected = True
            print(f"range-fixture event=request id={rid} stream={kind or 'unknown'} attempt={attempt} t_ms={started_ms}", flush=True)
            status, sent, lo, hi = 404, 0, None, None
            headers_ms = first_byte_ms = None
            outcome = "complete"
            cache = "origin"
            try:
                if kind is None:
                    self.send_response(404)
                    self.send_header("Content-Length", "0")
                    self.end_headers()
                    headers_ms = now_ms()
                    return
                path = files[kind]
                size = path.stat().st_size
                if body and attempt == "0" and kind == "video":
                    prefetch.start(self.server.server_port)
                cached = None
                if body and attempt == "0" and kind == "audio" and audio_buffer_bytes:
                    cached = prefetch.available(self.server.server_port)
                    if cached is not None and len(cached) == size:
                        cache = "audio_hit"
                    else:
                        cached = None
                        cache = "audio_miss"
                if delay_ms and cached is None:
                    time.sleep(delay_ms / 1000)
                if seek_action:
                    print(f"range-fixture event=seek_fault id={rid} action={seek_action} t_ms={now_ms()}", flush=True)
                    if seek_action == "delay":
                        time.sleep(5)
                if inject or seek_action in ("once", "always"):
                    status = 503
                    self.send_response(status)
                    self.send_header("Content-Length", "0")
                    self.end_headers()
                    headers_ms = now_ms()
                    return
                try:
                    lo, hi, status = byte_range(self.headers.get("Range"), size)
                except ValueError:
                    status = 416
                    self.send_response(status)
                    self.send_header("Content-Range", f"bytes */{size}")
                    self.send_header("Content-Length", "0")
                    self.end_headers()
                    headers_ms = now_ms()
                    return
                self.send_response(status)
                self.send_header("Accept-Ranges", "bytes")
                self.send_header("Content-Type", "video/mp4" if kind == "video" else "audio/mp4")
                self.send_header("Content-Length", str(hi - lo + 1))
                if status == 206:
                    self.send_header("Content-Range", f"bytes {lo}-{hi}/{size}")
                self.end_headers()
                headers_ms = now_ms()
                print(f"range-fixture event=headers id={rid} stream={kind} attempt={attempt} t_ms={headers_ms} status={status}", flush=True)
                if body and cached is not None:
                    # A view avoids copying the whole retained audio object for a range.
                    view = memoryview(cached)
                    for offset in range(lo, hi + 1, 65536):
                        data = view[offset:min(hi+1, offset+65536)]
                        self.wfile.write(data)
                        if first_byte_ms is None:
                            first_byte_ms = now_ms()
                            print(f"range-fixture event=first_write id={rid} stream={kind} attempt={attempt} t_ms={first_byte_ms}", flush=True)
                        sent += len(data)
                elif body:
                    with path.open("rb") as stream:
                        stream.seek(lo)
                        remaining = hi - lo + 1
                        while remaining:
                            data = stream.read(min(remaining, 65536))
                            if not data:
                                break
                            self.wfile.write(data)
                            if first_byte_ms is None:
                                first_byte_ms = now_ms()
                                print(f"range-fixture event=first_write id={rid} stream={kind} attempt={attempt} t_ms={first_byte_ms}", flush=True)
                            sent += len(data)
                            remaining -= len(data)
            except (ConnectionError, TimeoutError):
                outcome = "client_disconnected"
                self.close_connection = True
            finally:
                print(f"range-fixture id={rid} stream={kind or 'unknown'} method={'GET' if body else 'HEAD'} "
                      f"status={status} start={lo} end={hi} sent={sent} "
                      f"elapsed_ms={int((time.monotonic()-start_time)*1000)} attempt={attempt} "
                      f"started_ms={started_ms} headers_ms={headers_ms} first_write_ms={first_byte_ms} "
                      f"ended_ms={now_ms()} outcome={outcome} cache={cache} seek_fault={seek_action}", flush=True)

    return Handler

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--directory", type=Path, required=True)
    p.add_argument("--port", type=int, default=8765)
    p.add_argument("--delay-ms", type=int, default=0)
    p.add_argument("--audio-buffer-bytes", type=int, default=0, help="loopback audio-only prefetch cap, 0 disables; max 4194304")
    fault = p.add_mutually_exclusive_group()
    fault.add_argument("--fail-once", choices=["video", "audio"])
    fault.add_argument("--fail-always", choices=["video", "audio"])
    p.add_argument("--seek-marker", type=Path)
    p.add_argument("--seek-fault", choices=["once", "always", "delay"])
    a = p.parse_args()
    if (a.seek_marker is None) != (a.seek_fault is None) or (a.seek_fault and (a.fail_once or a.fail_always or a.audio_buffer_bytes)):
        p.error("seek marker and isolated seek fault required together")
    if a.seek_marker is not None and a.seek_marker.exists():
        p.error("seek marker already exists")
    if not 1 <= a.port <= 65535 or not 0 <= a.delay_ms <= 10000:
        p.error("invalid port or delay")
    if not 0 <= a.audio_buffer_bytes <= 4 * 1024 * 1024 or (a.audio_buffer_bytes and (a.fail_once or a.fail_always)):
        p.error("invalid audio buffer cap or combined fault injection")
    files = {"video": a.directory / "video.mp4", "audio": a.directory / "audio.m4a"}
    if any(not x.is_file() or x.stat().st_size <= 0 for x in files.values()):
        p.error("nonempty video.mp4 and audio.m4a required")
    with ThreadingHTTPServer(("127.0.0.1", a.port), make_handler(files, a.delay_ms, a.fail_once, a.fail_always, a.audio_buffer_bytes, a.seek_marker, a.seek_fault)) as server:
        print(f"range-fixture ready port={a.port} delay_ms={a.delay_ms} fail_once={a.fail_once} fail_always={a.fail_always} audio_buffer_bytes={a.audio_buffer_bytes}", flush=True)
        try:
            server.serve_forever()
        except KeyboardInterrupt:
            pass

if __name__ == "__main__":
    main()
