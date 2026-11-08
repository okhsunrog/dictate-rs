"""Isolated desktop regression test. Run through `just ui-smoke` (Xvfb).

Uses synthetic PCM, a localhost HTTP server and a fake clipboard. Never accesses
real audio, configuration, credentials, clipboard, or an existing resident.
Screenshots go to --output (default: a new directory under TMPDIR).
"""

import argparse
import ctypes
import http.server
import json
import os
import shlex
import subprocess
import tempfile
import threading
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def run(*args, env=None, check=True):
    result = subprocess.run(
        args, env=env, check=False, capture_output=True, text=True, timeout=8
    )
    if check and result.returncode:
        raise AssertionError(f"{args}: {result.stderr}")
    return result


def close_window(wid):
    """Send the same WM_DELETE_WINDOW request as the title-bar close button."""

    class Data(ctypes.Union):
        _fields_ = [
            ("b", ctypes.c_char * 20),
            ("s", ctypes.c_short * 10),
            ("l", ctypes.c_long * 5),
        ]

    class Client(ctypes.Structure):
        _fields_ = [
            ("type", ctypes.c_int),
            ("serial", ctypes.c_ulong),
            ("send_event", ctypes.c_int),
            ("display", ctypes.c_void_p),
            ("window", ctypes.c_ulong),
            ("message_type", ctypes.c_ulong),
            ("format", ctypes.c_int),
            ("data", Data),
        ]

    class Event(ctypes.Union):
        _fields_ = [("client", Client), ("pad", ctypes.c_long * 24)]

    x = ctypes.CDLL("libX11.so.6")
    x.XOpenDisplay.argtypes = [ctypes.c_char_p]
    x.XOpenDisplay.restype = ctypes.c_void_p
    x.XInternAtom.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int]
    x.XInternAtom.restype = ctypes.c_ulong
    x.XSendEvent.argtypes = [
        ctypes.c_void_p,
        ctypes.c_ulong,
        ctypes.c_int,
        ctypes.c_long,
        ctypes.POINTER(Event),
    ]
    x.XCloseDisplay.argtypes = [ctypes.c_void_p]
    display = x.XOpenDisplay(None)
    assert display
    event = Event()
    event.client.type = 33
    event.client.display = display
    event.client.window = int(wid)
    event.client.message_type = x.XInternAtom(display, b"WM_PROTOCOLS", 0)
    event.client.format = 32
    event.client.data.l[0] = x.XInternAtom(display, b"WM_DELETE_WINDOW", 0)
    assert x.XSendEvent(display, int(wid), 0, 0, ctypes.byref(event))
    x.XCloseDisplay(display)


def wait_for(predicate, description, timeout=6):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        value = predicate()
        if value:
            return value
        time.sleep(0.05)
    raise AssertionError(description)


class Backend(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_POST(self):
        self.rfile.read(int(self.headers["Content-Length"]))
        if self.path.endswith("/transcriptions"):
            self.server.transcriptions += 1
            if self.server.transcriptions == 1:
                self.send_response(503)
                self.end_headers()
                self.server.failed.set()
                return
            value = {"text": "Original test transcript"}
        elif self.path.endswith("/chat/completions"):
            self.server.edits += 1
            self.server.edit_started.set()
            self.server.release_edit.wait(6)
            value = {
                "choices": [
                    {
                        "finish_reason": "stop",
                        "message": {"content": "Edited test transcript."},
                    }
                ],
            }
        else:
            self.send_error(404)
            return
        body = json.dumps(value).encode()
        self.send_response(200)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        try:
            self.wfile.write(body)
        except (BrokenPipeError, ConnectionResetError):
            pass


def launcher(path, code):
    source = path.with_suffix(".py")
    source.write_text(code)
    path.write_text(
        "#!/bin/sh\nexec uv run --offline " + shlex.quote(str(source)) + "\n"
    )
    path.chmod(0o755)


def exercise(binary, out, server, configured, capture=True):
    with tempfile.TemporaryDirectory(prefix="dictate-fixture-") as temp:
        fixture = Path(temp)
        config = fixture / "config" / "dictate"
        config.mkdir(parents=True)
        (config / "config.toml").write_text(
            f'base_url = "http://127.0.0.1:{server.server_port}/v1"\napi_key = "{"fixture" if configured else ""}"\nmodel = "fixture-model"\noutput = "paste"\n'
        )
        fake_bin = fixture / "bin"
        fake_bin.mkdir()
        launcher(
            fake_bin / "pw-record",
            """import signal, sys, time
running = True
def stop(*_):
    global running
    running = False
signal.signal(signal.SIGINT, stop)
while running:
    sys.stdout.buffer.write(b"\\x01\\x01" * 1600)
    sys.stdout.buffer.flush()
    time.sleep(.1)
""",
        )
        clipboard = fixture / "clipboard"
        launcher(
            fake_bin / "wl-copy",
            f"import sys\nfrom pathlib import Path\nPath({str(clipboard)!r}).write_bytes(sys.stdin.buffer.read())\n",
        )
        env = os.environ.copy()
        env.update(
            XDG_RUNTIME_DIR=temp,
            XDG_CONFIG_HOME=str(fixture / "config"),
            PATH=str(fake_bin) + os.pathsep + env["PATH"],
            QT_QPA_PLATFORM="xcb",
            QT_QPA_PLATFORMTHEME="",
            QT_QPA_PLATFORMTHEME_QT6="",
            QT_FORCE_STDERR_LOGGING="1",
            QT_QUICK_CONTROLS_STYLE="org.kde.desktop",
            QT_QUICK_BACKEND="software",
        )
        env.pop("DICTATE_API_KEY", None)
        env.pop("DICTATE_BASE_URL", None)
        env.pop("ELEVENLABS_API_KEY", None)
        # Exercise ordinary windows here; tests/wayland.py checks layer-shell.
        env.pop("WAYLAND_DISPLAY", None)
        executable = str(ROOT / "target" / "debug" / binary)
        log_path = out / f"{binary}-{'pipeline' if configured else 'no-key'}.log"
        # XTest events track releases correctly when a shortcut hides its window.
        run("xdotool", "keyup", "Escape", "ctrl", "Return")
        with log_path.open("w") as log:
            process = subprocess.Popen([executable], env=env, stdout=log, stderr=log)

            def command(*args):
                run(executable, *args, env=env)

            def windows():
                return run(
                    "xdotool",
                    "search",
                    "--onlyvisible",
                    "--pid",
                    str(process.pid),
                    check=False,
                ).stdout.split()

            def main_window():
                title = "^(Dictate|Dictate Settings)$"
                matches = run(
                    "xdotool", "search", "--onlyvisible", "--name", title, check=False
                ).stdout.split()
                return matches[-1] if matches else None

            def screenshot(wid, suffix):
                if capture:
                    target = "root"
                    run(
                        "import", "-window", target, str(out / f"{binary}-{suffix}.png")
                    )

            try:
                try:
                    wait_for(windows, "recording overlay did not appear")
                except AssertionError:
                    print(run("xwininfo", "-root", "-tree").stdout, flush=True)
                    screenshot(None, "startup-failure")
                    raise
                assert not main_window(), (
                    "record shortcut must not open the main window"
                )
                screenshot(windows()[-1], "recording")
                time.sleep(0.6)
                command()
                try:
                    wid = wait_for(main_window, "preview must appear after stop")
                except AssertionError:
                    print(
                        binary,
                        "exit",
                        process.poll(),
                        "visible",
                        [
                            (w, run("xdotool", "getwindowname", w).stdout.strip())
                            for w in windows()
                        ],
                        flush=True,
                    )
                    print(run("xwininfo", "-root", "-tree").stdout, flush=True)
                    for w in run(
                        "xdotool", "search", "--name", "^Dictate$", check=False
                    ).stdout.split():
                        print(run("xwininfo", "-id", w).stdout, flush=True)
                        print(run("xprop", "-id", w, "_NET_WM_PID").stdout, flush=True)
                    for index, w in enumerate(windows()):
                        screenshot(w, f"failure-{index}")
                    raise
                if not configured:
                    time.sleep(0.2)
                    screenshot(wid, "missing-key")
                    assert server.transcriptions == 0, (
                        "missing config must not send audio"
                    )
                    close_window(wid)
                    process.wait(timeout=4)
                    return
                assert server.failed.wait(4), "first transcription should fail"
                time.sleep(0.2)
                command("retry")
                assert server.edit_started.wait(4), "retry must use retained audio"
                # Edit the exposed transcript while the model is still working.
                run("xdotool", "windowfocus", wid)
                run("xdotool", "mousemove", "--window", wid, "90", "200", "click", "1")
                run("xdotool", "key", "--clearmodifiers", "ctrl+a")
                run("xdotool", "type", "--clearmodifiers", "Manual revision")
                server.release_edit.set()
                time.sleep(0.4)
                screenshot(wid, "manual-draft")
                run("xdotool", "key", "--clearmodifiers", "ctrl+Return")
                wait_for(clipboard.exists, "copy action did not run")
                assert clipboard.read_text() == "Manual revision", (
                    "late model response overwrote manual edits"
                )
                wait_for(
                    lambda: not main_window(), "copy and close must hide the window"
                )
                # A retry must only edit, not transcribe; closing must not cancel it
                # or reopen the window when a successful result arrives.
                server.release_edit.clear()
                server.edit_started.clear()
                command("retry")
                wid = wait_for(main_window, "retry preview did not open")
                assert server.edit_started.wait(4)
                run("xdotool", "windowfocus", wid)
                run("xdotool", "key", "--clearmodifiers", "Escape")
                wait_for(lambda: not main_window(), "Escape must hide the window")
                server.release_edit.set()
                time.sleep(0.4)
                assert not main_window(), "completion must not reopen a hidden preview"
                assert server.transcriptions == 2, (
                    "editor retry must not transcribe again"
                )
                command("show")
                wid = wait_for(main_window, "resident did not reopen the saved draft")
                run("xdotool", "windowsize", wid, "440", "500")
                time.sleep(0.2)
                screenshot(wid, "narrow")
                command("settings")
                time.sleep(0.4)
                wid = wait_for(main_window, "settings window did not appear")
                screenshot(wid, "settings")
                command("clipboard")
                wait_for(windows, "clipboard recording did not start")
                time.sleep(0.6)
                command()
                wait_for(
                    lambda: clipboard.read_text() == "Edited test transcript.",
                    "clipboard argument must deliver without preview",
                )
                assert not main_window(), "clipboard delivery opened the editor"
                server.release_edit.clear()
                server.edit_started.clear()
                command("retry")
                assert server.edit_started.wait(4)
                wid = wait_for(main_window, "retry did not open editor")
                close_window(wid)
                process.wait(timeout=4)
            finally:
                server.release_edit.set()
                command("quit")
                try:
                    process.wait(timeout=4)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
                    raise AssertionError(f"{binary}: resident did not exit after quit")
        errors = log_path.read_text()
        assert "QQmlApplicationEngine failed" not in errors, errors
        assert "ReferenceError" not in errors, errors
        assert "Binding loop" not in errors, errors


def main():
    # All helper programs must target Xvfb as well as the application.
    os.environ.pop("WAYLAND_DISPLAY", None)
    os.environ["XDG_SESSION_TYPE"] = "x11"
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--output", type=Path, default=Path(tempfile.mkdtemp(prefix="dictate-ui-"))
    )
    parser.add_argument("--no-screenshots", action="store_true")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    binary = "dictate"
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Backend)
    server.transcriptions = server.edits = 0
    server.failed = threading.Event()
    server.edit_started = threading.Event()
    server.release_edit = threading.Event()
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        exercise(binary, args.output, server, False, not args.no_screenshots)
        server.release_edit.clear()
        exercise(binary, args.output, server, True, not args.no_screenshots)
        print(
            f"{binary}: capture without key, error/retry, draft protection, copy/close, hidden completion OK"
        )
    finally:
        server.release_edit.set()
        server.shutdown()
        server.server_close()
        thread.join()
    print(f"Screenshots: {args.output}")


if __name__ == "__main__":
    main()
