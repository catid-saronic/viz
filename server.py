"""
Simple HTTP server to host the animated projection page.

Usage::

    python server.py [port] [--no-tunnel]

The server binds to 0.0.0.0 so that it is reachable from other
machines on the same network. By default it listens on port 8000.

On start-up the script will try to create a public Internet-accessible tunnel
with *ngrok* so that the page can be shared with people outside your local
network.  The public URL will be printed to the console.  Use the ``--no-tunnel``
flag to disable this behaviour.
"""

from http.server import ThreadingHTTPServer, SimpleHTTPRequestHandler
from pathlib import Path
import argparse
import os
import sys

# ``pyngrok`` is imported lazily inside ``main`` so that running the server
# locally does not require any extra dependencies unless a public tunnel is
# requested.


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Serve the projection art page over HTTP and optionally expose it publicly via ngrok.")
    parser.add_argument(
        "port",
        nargs="?",
        type=int,
        default=8000,
        help="TCP port to listen on (default: 8000)",
    )
    parser.add_argument(
        "--no-tunnel",
        action="store_true",
        help="Do not create an Internet-accessible ngrok tunnel",
    )

    args = parser.parse_args()

    # Ensure we serve files relative to the directory that contains this script.
    root = Path(__file__).resolve().parent
    os.chdir(root)

    handler = SimpleHTTPRequestHandler
    server = ThreadingHTTPServer(("0.0.0.0", args.port), handler)

    print(f"Serving HTTP on 0.0.0.0 port {args.port} (http://<this-host>:{args.port}/) …")
    tunnel = None

    if not args.no_tunnel:
        try:
            from pyngrok import ngrok  # type: ignore

            tunnel = ngrok.connect(args.port, "http")
            print()
            print("Public tunnel created ✔")
            print(f" -> {tunnel.public_url}")
            print()
        except ModuleNotFoundError:
            print()
            print(
                "pyngrok is not installed – skipping public tunnel. "
                "Install it with 'pip install pyngrok' and run again to enable Internet sharing."
            )
            print()
        except Exception as exc:
            print()
            print(f"Failed to create ngrok tunnel: {exc}\nContinuing without it…")
            print()

    print("Press Ctrl+C to stop.")
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\nShutting down…")
        server.server_close()
        if tunnel is not None:
            try:
                from pyngrok import ngrok  # type: ignore

                ngrok.disconnect(tunnel.public_url)
                ngrok.kill()
            except Exception:
                pass


if __name__ == "__main__":
    # If the script is invoked with `python -m server`, ensure that it still works.
    sys.exit(main())
