"""Fake persistent worker for offline Rust integration testing."""
import json
import sys
import time
import uuid

instance = str(uuid.uuid4())
print(json.dumps({"ready": 1}), flush=True)
for line in sys.stdin:
    request = json.loads(line)
    video = request["url"].rsplit("=", 1)[-1]
    if video == "ccccccccccc":
        time.sleep(30)
    reply_id = request["id"] + int(video == "ddddddddddd")
    info = {"id": video, "title": instance, "formats": []}
    print(json.dumps({"id": reply_id, "ok": True, "payload": json.dumps(info)}), flush=True)
