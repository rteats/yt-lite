"""Bounded audio prefetch for the synthetic loopback HTTP experiment only."""
import http.client
import threading


class AudioPrefetch:
    def __init__(self, max_bytes, size, now_ms):
        self.max_bytes = max_bytes
        self.size = size
        self.now_ms = now_ms
        self.lock = threading.Lock()
        self.done = threading.Event()
        self.started = False
        self.data = None

    def start(self, port):
        if not self.max_bytes:
            return
        with self.lock:
            if self.started:
                return
            self.started = True
            if not 0 < self.size <= self.max_bytes:
                print(f"audio-prefetch status=over_budget size={self.size} cap={self.max_bytes} t_ms={self.now_ms()}", flush=True)
                self.done.set()
                return
            threading.Thread(target=self.fetch, args=(port,), daemon=True).start()

    def fetch(self, port):
        started = self.now_ms()
        print(f"audio-prefetch status=begin size={self.size} cap={self.max_bytes} t_ms={started}", flush=True)
        connection = None
        status = "failed"
        try:
            connection = http.client.HTTPConnection("127.0.0.1", port, timeout=3)
            connection.request("GET", "/prefetch/audio.m4a", headers={"Range": f"bytes=0-{self.size-1}"})
            response = connection.getresponse()
            if (response.status != 206 or response.getheader("Content-Range") != f"bytes 0-{self.size-1}/{self.size}"
                    or response.getheader("Content-Length") != str(self.size)):
                return
            data = response.read(self.size)
            if len(data) == self.size:
                self.data = data
                status = "ready"
        except (OSError, http.client.HTTPException):
            pass
        finally:
            if connection is not None:
                connection.close()
            print(f"audio-prefetch status={status} bytes={len(self.data) if self.data is not None else 0} "
                  f"elapsed_ms={self.now_ms()-started} t_ms={self.now_ms()}", flush=True)
            self.done.set()

    def available(self, port):
        self.start(port)
        # Never hold up ordinary audio retrieval indefinitely on a failed prefetch.
        if self.started:
            self.done.wait(timeout=3)
        return self.data
