import http.client
import contextlib
import io
import tempfile
import threading
import unittest
from pathlib import Path
from http.server import ThreadingHTTPServer
from range_fixture import byte_range, make_handler

class RangeFixtureTest(unittest.TestCase):
    def test_ranges(self):
        self.assertEqual(byte_range("bytes=2-4", 10), (2, 4, 206))
        self.assertEqual(byte_range("bytes=-3", 10), (7, 9, 206))
        self.assertEqual(byte_range("bytes=5-", 10), (5, 9, 206))
        with self.assertRaises(ValueError):
            byte_range("bytes=10-", 10)

    def test_real_http_failure_then_range(self):
        with tempfile.TemporaryDirectory() as d:
            path = Path(d) / "media"
            path.write_bytes(b"0123456789")
            with ThreadingHTTPServer(("127.0.0.1", 0), make_handler({"video": path, "audio": path}, fail_once="audio")) as server:
                thread = threading.Thread(target=server.serve_forever, daemon=True)
                thread.start()
                conn = http.client.HTTPConnection("127.0.0.1", server.server_port, timeout=2)
                try:
                    conn.request("GET", "/attempt/1/audio.m4a", headers={"Range": "bytes=2-4"})
                    r = conn.getresponse()
                    self.assertEqual(r.status, 503)
                    self.assertEqual(r.read(), b"")
                    conn.request("GET", "/attempt/2/audio.m4a", headers={"Range": "bytes=2-4"})
                    r = conn.getresponse()
                    self.assertEqual(r.status, 206)
                    self.assertEqual(r.getheader("Content-Range"), "bytes 2-4/10")
                    self.assertEqual(r.read(), b"234")
                    conn.request("GET", "/video.mp4", headers={"Range": "bytes=99-"})
                    r = conn.getresponse()
                    self.assertEqual(r.status, 416)
                    r.read()
                finally:
                    conn.close()
                    server.shutdown()
                    thread.join()


    def test_persistent_audio_failure_across_both_generations(self):
        with tempfile.TemporaryDirectory() as d:
            path = Path(d) / "media"
            path.write_bytes(b"0123456789")
            with ThreadingHTTPServer(("127.0.0.1", 0), make_handler({"video": path, "audio": path}, fail_always="audio")) as server:
                thread = threading.Thread(target=server.serve_forever, daemon=True)
                thread.start()
                conn = http.client.HTTPConnection("127.0.0.1", server.server_port, timeout=2)
                try:
                    for attempt in (1, 2):
                        conn.request("GET", f"/attempt/{attempt}/audio.m4a")
                        response = conn.getresponse()
                        self.assertEqual(response.status, 503)
                        self.assertEqual(response.read(), b"")
                    conn.request("GET", "/attempt/2/video.mp4", headers={"Range": "bytes=0-1"})
                    response = conn.getresponse()
                    self.assertEqual(response.status, 206)
                    self.assertEqual(response.read(), b"01")
                    conn.request("GET", "/attempt/3/video.mp4")
                    response = conn.getresponse()
                    self.assertEqual(response.status, 404)
                    response.read()
                finally:
                    conn.close()
                    server.shutdown()
                    thread.join()


    def test_prefetched_audio_serves_exact_ranges_and_video_stays_origin(self):
        self.check_audio_buffer(16, "audio_hit", "status=ready", True)

    def test_oversized_audio_uses_origin_without_prefetch(self):
        self.check_audio_buffer(4, "audio_miss", "status=over_budget", False)

    def check_audio_buffer(self, cap, expected_cache, expected_status, expected_fetch):
        with tempfile.TemporaryDirectory() as d, contextlib.redirect_stdout(io.StringIO()) as log:
            video = Path(d) / "video.mp4"
            audio = Path(d) / "audio.m4a"
            video.write_bytes(b"ABCDEFGHIJ")
            audio.write_bytes(b"0123456789")
            with ThreadingHTTPServer(("127.0.0.1", 0), make_handler({"video": video, "audio": audio}, delay_ms=5, audio_buffer_bytes=cap)) as server:
                thread = threading.Thread(target=server.serve_forever, daemon=True)
                thread.start()
                conn = http.client.HTTPConnection("127.0.0.1", server.server_port, timeout=4)
                try:
                    conn.request("GET", "/video.mp4", headers={"Range": "bytes=0-2"})
                    response = conn.getresponse()
                    self.assertEqual(response.status, 206)
                    self.assertEqual(response.read(), b"ABC")
                    for header, expected in [("bytes=2-4", b"234"), ("bytes=-3", b"789")]:
                        conn.request("GET", "/audio.m4a", headers={"Range": header})
                        response = conn.getresponse()
                        self.assertEqual(response.status, 206)
                        self.assertEqual(response.read(), expected)
                    conn.request("GET", "/audio.m4a", headers={"Range": "bytes=99-"})
                    response = conn.getresponse()
                    self.assertEqual(response.status, 416)
                    self.assertEqual(response.read(), b"")
                finally:
                    conn.close()
                    server.shutdown()
                    thread.join()
            output = log.getvalue()
            self.assertIn(expected_status, output)
            self.assertIn("cache=" + expected_cache, output)
            self.assertIn("stream=video method=GET status=206", output)
            self.assertEqual("attempt=prefetch" in output, expected_fetch)


    def test_seek_fault_arms_only_after_marker_and_never_fails_video(self):
        for fault in ("once", "always"):
            with self.subTest(fault=fault), tempfile.TemporaryDirectory() as d:
                path = Path(d) / "media"
                path.write_bytes(b"0123456789")
                marker = Path(d) / "armed"
                with ThreadingHTTPServer(("127.0.0.1", 0), make_handler(
                        {"video": path, "audio": path}, seek_marker=marker, seek_fault=fault)) as server:
                    thread = threading.Thread(target=server.serve_forever, daemon=True)
                    thread.start()
                    conn = http.client.HTTPConnection("127.0.0.1", server.server_port, timeout=2)
                    def get(kind, method="GET"):
                        conn.request(method, f"/{kind}", headers={"Range": "bytes=2-4"})
                        response = conn.getresponse()
                        return response.status, response.read()
                    try:
                        self.assertEqual(get("audio.m4a"), (206, b"234"))
                        marker.touch()
                        self.assertEqual(get("audio.m4a", "HEAD"), (206, b""))
                        self.assertEqual(get("video.mp4"), (206, b"234"))
                        self.assertEqual(get("audio.m4a"), (503, b""))
                        self.assertEqual(get("audio.m4a"), (206, b"234") if fault == "once" else (503, b""))
                    finally:
                        conn.close()
                        server.shutdown()
                        thread.join()
