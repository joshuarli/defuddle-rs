import subprocess
import sys
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from tempfile import TemporaryDirectory
from threading import Thread


def check_page(directory: Path, chromium: str) -> None:
    handler = partial(SimpleHTTPRequestHandler, directory=directory)
    with ThreadingHTTPServer(("127.0.0.1", 0), handler) as server:
        port = server.server_address[1]
        thread = Thread(target=server.serve_forever)
        thread.start()
        try:
            with TemporaryDirectory(prefix="chrome-profile-", dir=directory.parent) as profile:
                result = subprocess.run(
                    [
                        chromium,
                        "--headless",
                        "--dump-dom",
                        "--no-first-run",
                        "--disable-background-networking",
                        "--virtual-time-budget=10000",
                        f"--user-data-dir={profile}",
                        f"http://127.0.0.1:{port}/",
                    ],
                    check=False,
                    capture_output=True,
                    text=True,
                    timeout=30,
                )
        finally:
            server.shutdown()
            thread.join()

    if result.returncode or '<output id="result">PASS</output>' not in result.stdout:
        sys.stderr.write(result.stderr)
        sys.stderr.write(result.stdout)
        raise SystemExit("Defuddle WASM browser fixture failed")
    print("Defuddle WASM browser fixture passed")


if __name__ == "__main__":
    check_page(Path(sys.argv[1]).resolve(), sys.argv[2])
