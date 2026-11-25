import http.client
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
