"""Offline tests for the worker's public line protocol."""
import io
import json
import unittest

from benchmark import SafeLogger
from worker import serve


class FakeExtractor:
    def __init__(self):
        self.calls = []

    def extract_info(self, url, download):
        self.calls.append((url, download))
        video = url.rsplit("=", 1)[-1]
        if video == "zzzzzzzzzzz":
            raise RuntimeError("secret failure text")
        return {"id": video, "title": "Fixture", "formats": []}

    def sanitize_info(self, info):
        return info


class ProtocolTest(unittest.TestCase):
    def test_multiple_requests_and_failure_keep_ids_and_worker(self):
        fake = FakeExtractor()
        source = io.StringIO("\n".join(json.dumps({"id": n, "url": url}) for n, url in [
            (1, "https://www.youtube.com/watch?v=aaaaaaaaaaa"),
            (2, "https://www.youtube.com/watch?v=zzzzzzzzzzz"),
            (3, "https://www.youtube.com/watch?v=bbbbbbbbbbb"),
            (4, "https://example.com/watch?v=aaaaaaaaaaa"),
        ]))
        sink = io.StringIO()
        serve(fake, SafeLogger(), source, sink)
        rows = [json.loads(line) for line in sink.getvalue().splitlines()]
        self.assertEqual(rows[0], {"ready": 1})
        self.assertEqual([(r["id"], r["ok"]) for r in rows[1:]],
                         [(1, True), (2, False), (3, True), (4, False)])
        self.assertEqual(json.loads(rows[3]["payload"])["id"], "bbbbbbbbbbb")
        self.assertEqual(len(fake.calls), 3)
        self.assertTrue(all(not download for _, download in fake.calls))
        self.assertNotIn("secret failure text", sink.getvalue())


if __name__ == "__main__":
    unittest.main()
