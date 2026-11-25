"""Loopback-only HTTP range fixture. Serves two explicit synthetic media files."""
import argparse
import re
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

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

def make_handler(files, delay_ms=0, fail_once=None):
    lock = threading.Lock()
    counter = 0
    failed = False

    class Handler(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def log_message(self, *args):
            pass  # do not print arbitrary request paths/headers

        def do_HEAD(self):
            self.serve_media(False)

        def do_GET(self):
            self.serve_media(True)

        def serve_media(self, body):
            nonlocal counter, failed
            start_time = time.monotonic()
            kind = {"/video.mp4": "video", "/audio.m4a": "audio"}.get(self.path)
            with lock:
                counter += 1
                rid = counter
                inject = body and kind is not None and kind == fail_once and not failed
                if inject:
                    failed = True
            status, sent, lo, hi = 404, 0, None, None
            try:
                if kind is None:
                    self.send_response(404)
                    self.send_header("Content-Length", "0")
                    self.end_headers()
                    return
                path = files[kind]
                size = path.stat().st_size
                if delay_ms:
                    time.sleep(delay_ms / 1000)
                if inject:
                    status = 503
                    self.send_response(status)
                    self.send_header("Content-Length", "0")
                    self.end_headers()
                    return
                try:
                    lo, hi, status = byte_range(self.headers.get("Range"), size)
                except ValueError:
                    status = 416
                    self.send_response(status)
                    self.send_header("Content-Range", f"bytes */{size}")
                    self.send_header("Content-Length", "0")
                    self.end_headers()
                    return
                self.send_response(status)
                self.send_header("Accept-Ranges", "bytes")
                self.send_header("Content-Type", "video/mp4" if kind == "video" else "audio/mp4")
                self.send_header("Content-Length", str(hi - lo + 1))
                if status == 206:
                    self.send_header("Content-Range", f"bytes {lo}-{hi}/{size}")
                self.end_headers()
                if body:
                    with path.open("rb") as stream:
                        stream.seek(lo)
                        remaining = hi - lo + 1
                        while remaining:
                            data = stream.read(min(remaining, 65536))
                            if not data:
                                break
                            self.wfile.write(data)
                            sent += len(data)
                            remaining -= len(data)
            except (BrokenPipeError, ConnectionResetError, TimeoutError):
                pass
            finally:
                print(f"range-fixture id={rid} stream={kind or 'unknown'} method={'GET' if body else 'HEAD'} "
                      f"status={status} start={lo} end={hi} sent={sent} "
                      f"elapsed_ms={int((time.monotonic()-start_time)*1000)}", flush=True)

    return Handler

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--directory", type=Path, required=True)
    p.add_argument("--port", type=int, default=8765)
    p.add_argument("--delay-ms", type=int, default=0)
    p.add_argument("--fail-once", choices=["video", "audio"])
    a = p.parse_args()
    if not 1 <= a.port <= 65535 or not 0 <= a.delay_ms <= 10000:
        p.error("invalid port or delay")
    files = {"video": a.directory / "video.mp4", "audio": a.directory / "audio.m4a"}
    if any(not x.is_file() or x.stat().st_size <= 0 for x in files.values()):
        p.error("nonempty video.mp4 and audio.m4a required")
    with ThreadingHTTPServer(("127.0.0.1", a.port), make_handler(files, a.delay_ms, a.fail_once)) as server:
        print(f"range-fixture ready port={a.port} delay_ms={a.delay_ms} fail_once={a.fail_once}", flush=True)
        try:
            server.serve_forever()
        except KeyboardInterrupt:
            pass

if __name__ == "__main__":
    main()
