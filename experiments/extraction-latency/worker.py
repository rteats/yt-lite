"""Private JSON-lines IPC for the opt-in playback experiment.

Stdout is reserved for the protocol; signed URLs are never printed to logs.
Run with a local Python environment containing the benchmarked yt-dlp release.
"""
import contextlib
import json
import sys
import time

from benchmark import SafeLogger, options, validate_url


def serve(worker, logger, source, sink):
    def send(response):
        sink.write(json.dumps(response, ensure_ascii=True, allow_nan=False) + "\n")
        sink.flush()

    send({"ready": 1})
    for line in source:
        request_id = None
        try:
            request = json.loads(line)
            request_id = request["id"]
            if type(request_id) is not int or not 0 < request_id < 2**64:
                raise ValueError("invalid request ID")
            url = validate_url(request["url"])
        except (ValueError, TypeError, KeyError, AttributeError):
            send({"id": request_id, "ok": False})
            continue
        logger.reset()
        started = time.perf_counter()
        response = {"id": request_id, "ok": False}
        try:
            info = worker.extract_info(url, download=False)
            elapsed = time.perf_counter() - started
            serialization = time.perf_counter()
            if not isinstance(info, dict) or info.get("id") != url.rsplit("=", 1)[-1]:
                raise ValueError("invalid video result")
            payload = json.dumps(worker.sanitize_info(info), ensure_ascii=True, allow_nan=False)
            response.update(ok=True, payload=payload,
                            serialize_ms=round((time.perf_counter() - serialization) * 1000))
        except Exception:
            elapsed = time.perf_counter() - started
        response.update(extract_ms=round(elapsed * 1000), retries=logger.retries,
                        resets=logger.resets, timeouts=logger.timeouts,
                        solver_failures=logger.solver_failures)
        send(response)


def main():
    # Suppress unexpected provider output on the protocol stream. The parent
    # discards stderr as it can contain credentials and signed media URLs.
    sink = sys.stdout
    with contextlib.redirect_stdout(sys.stderr):
        from yt_dlp import YoutubeDL
        logger = SafeLogger()
        with YoutubeDL(options(logger)) as worker:
            serve(worker, logger, sys.stdin, sink)


if __name__ == "__main__":
    main()
