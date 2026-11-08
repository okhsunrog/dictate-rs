"""Qt Bridge/LayerShellQt regression test in a private nested Niri session.

Run with uv run tests/qt_wayland.py from a Wayland session. Uses synthetic
audio and private configuration, D-Bus and clipboard; never records a microphone
or contacts a remote service. Artifacts are saved under --output or TMPDIR.
"""

import argparse
import json
import os
import re
import shlex
import subprocess
import tempfile
import time
from pathlib import Path

from ui_smoke import ROOT, launcher, run, wait_for


def inside(output):
    env = os.environ.copy()
    runtime = Path(env["XDG_RUNTIME_DIR"])
    assert Path(env["NIRI_SOCKET"]).is_relative_to(runtime), (
        "test requires its own Niri socket"
    )
    fake = runtime / "bin"
    fake.mkdir()
    launcher(
        fake / "pw-record",
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
    env.update(
        PATH=str(fake) + os.pathsep + env["PATH"],
        QT_QPA_PLATFORMTHEME="",
        QT_QPA_PLATFORMTHEME_QT6="",
        QT_FORCE_STDERR_LOGGING="1",
        QT_QPA_PLATFORM="wayland",
        QT_QUICK_BACKEND="software",
        WAYLAND_DEBUG="client",
    )
    for key in ["DICTATE_API_KEY", "DICTATE_BASE_URL", "ELEVENLABS_API_KEY"]:
        env.pop(key, None)
    binary = str(ROOT / "target/debug/dictate")
    log_path = output / "wayland.log"

    def windows():
        return json.loads(run("niri", "msg", "--json", "windows", env=env).stdout)

    def command(*args):
        run(binary, *args, env=env)

    def trace():
        return log_path.read_text(errors="replace")

    with log_path.open("w") as log:
        process = subprocess.Popen([binary], env=env, stdout=log, stderr=log)
        try:
            wait_for(
                lambda: "set_keyboard_interactivity(0)" in trace(),
                "OSD must create a non-focusing layer surface",
                15,
            )
            wait_for(
                lambda: re.search(r"wl_surface[#@]\d+\.attach\(wl_buffer", trace()),
                "OSD must submit a rendered buffer",
            )
            assert "get_layer_surface" in trace(), "OSD must use layer-shell"
            assert not windows(), "recording must not map a normal window"
            assert not re.search(r"set_keyboard_interactivity\([12]\)", trace()), (
                "OSD requested keyboard focus"
            )
            time.sleep(0.6)
            command()
            editor = wait_for(
                windows, "preview must open after stopping without an API key"
            )[0]
            assert editor["pid"] == process.pid
            assert editor["app_id"] == "ru.okhsunrog.dictate"
            time.sleep(0.3)
            run(
                "niri",
                "msg",
                "action",
                "screenshot-window",
                "--id",
                str(editor["id"]),
                "--path",
                str(output / "qt-niri-preview.png"),
                env=env,
            )
            wait_for(
                lambda: (output / "qt-niri-preview.png").exists(),
                "preview screenshot missing",
            )
            command("settings")
            command("quit")
            assert process.wait(timeout=5) == 0
            assert not (runtime / "dictate.sock").exists(), "IPC socket survived exit"
            errors = trace()
            for error in [
                "panicked at",
                "ReferenceError",
                "TypeError",
                "Binding loop",
                "QQmlApplicationEngine failed",
            ]:
                assert error not in errors, errors[-4000:]
            (output / "passed").write_text(
                "Qt Bridge on Niri: layer-shell, no focus request, capture-first, preview, IPC and exit OK\n"
            )
        finally:
            if process.poll() is None:
                command("quit")
                process.wait(timeout=5)
            run(
                "niri",
                "msg",
                "action",
                "quit",
                "--skip-confirmation",
                env=env,
                check=False,
            )


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--inside", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if args.inside:
        inside(args.inside)
        return
    output = (
        args.output or Path(tempfile.mkdtemp(prefix="dictate-wayland-"))
    ).resolve()
    output.mkdir(parents=True, exist_ok=True)
    (output / "passed").unlink(missing_ok=True)
    host_display = Path(os.environ["WAYLAND_DISPLAY"])
    if not host_display.is_absolute():
        host_display = Path(os.environ["XDG_RUNTIME_DIR"]) / host_display
    # Unix socket paths have a 108-byte limit. Keep the runtime next to the
    # artifacts rather than under Nix's additional nix-shell-* temp directory.
    with tempfile.TemporaryDirectory(prefix="w-", dir=output.parent) as temp:
        runtime = Path(temp)
        config = runtime / "niri.kdl"
        config.write_text("animations { off; }\n")
        env = os.environ.copy()
        env.update(
            XDG_RUNTIME_DIR=temp,
            XDG_CONFIG_HOME=temp + "/config",
            WAYLAND_DISPLAY=str(host_display),
            QT_QPA_PLATFORM="wayland",
        )
        env.pop("NIRI_SOCKET", None)
        # Niri exports its private display and IPC socket to startup commands.
        session = runtime / "session"
        session.write_text(
            "#!/bin/sh\nexec uv run "
            + shlex.quote(str(Path(__file__).resolve()))
            + " --inside "
            + shlex.quote(str(output))
            + "\n"
        )
        session.chmod(0o755)
        with (output / "niri.log").open("w") as log:
            process = subprocess.Popen(
                [
                    "dbus-run-session",
                    "--",
                    "niri",
                    "--config",
                    str(config),
                    "--",
                    str(session),
                ],
                env=env,
                stdout=log,
                stderr=log,
                start_new_session=True,
            )
            try:
                assert process.wait(timeout=45) == 0, f"Niri failed: {output}/niri.log"
            finally:
                if process.poll() is None:
                    import signal

                    os.killpg(process.pid, signal.SIGTERM)
                    process.wait(timeout=5)
    assert (output / "passed").exists(), f"Wayland test failed: {output}/niri.log"
    print((output / "passed").read_text().strip())
    print(f"Artifacts: {output}")


if __name__ == "__main__":
    main()
