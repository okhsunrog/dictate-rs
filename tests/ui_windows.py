"""Render manual-opening states and verify they never start the microphone.

Run inside Xvfb + dbus-run-session, using uv run. No real audio or API access.
"""

import argparse
import os
import subprocess
import tempfile
import time
from pathlib import Path

from ui_smoke import ROOT, close_window, launcher, run, wait_for

os.environ.pop("WAYLAND_DISPLAY", None)
os.environ["XDG_SESSION_TYPE"] = "x11"

parser = argparse.ArgumentParser()
parser.add_argument("--output", type=Path)
args = parser.parse_args()
output = args.output or Path(tempfile.mkdtemp(prefix="dictate-windows-"))
output.mkdir(parents=True, exist_ok=True)
binary = "dictate"
with tempfile.TemporaryDirectory(prefix="dictate-show-") as d:
    root = Path(d)
    fake = root / "bin"
    fake.mkdir()
    marker = root / "microphone-started"
    launcher(
        fake / "pw-record", f"from pathlib import Path\nPath({str(marker)!r}).touch()\n"
    )
    env = os.environ.copy()
    env.update(
        XDG_RUNTIME_DIR=d,
        XDG_CONFIG_HOME=d + "/config",
        PATH=str(fake) + os.pathsep + env["PATH"],
        QT_QPA_PLATFORM="xcb",
        QT_QUICK_BACKEND="software",
        QT_QPA_PLATFORMTHEME="",
        QT_QPA_PLATFORMTHEME_QT6="",
        QT_FORCE_STDERR_LOGGING="1",
        QT_QUICK_CONTROLS_STYLE="org.kde.desktop",
    )
    for key in [
        "DICTATE_API_KEY",
        "DICTATE_BASE_URL",
        "ELEVENLABS_API_KEY",
        "WAYLAND_DISPLAY",
    ]:
        env.pop(key, None)
    exe = str(ROOT / "target/debug" / binary)
    broken = env | {"QT_QUICK_CONTROLS_STYLE": "DictateMissingStyle"}
    failed = run(exe, "show", env=broken, check=False)
    assert failed.returncode != 0, "QML loading failure must not report success"
    assert not (root / "dictate.sock").exists(), "failed startup left an IPC socket"
    assert not marker.exists(), "failed startup started microphone"

    def window():
        title = "Dictate|Dictate Settings"
        ids = run(
            "xdotool", "search", "--onlyvisible", "--name", title, check=False
        ).stdout.split()
        return ids[-1] if ids else None

    with (output / f"{binary}.log").open("w") as log:
        p = subprocess.Popen([exe, "show"], env=env, stdout=log, stderr=log)
        try:
            wait_for(window, f"{binary}: initial manual window missing")
            for state in ["show", "settings"]:
                run(exe, state, env=env)
                wid = wait_for(window, f"{binary}: {state} window missing")
                time.sleep(0.3)
                run("import", "-window", wid, str(output / f"{binary}-{state}.png"))
                assert not marker.exists(), f"{state} started microphone"
            close_window(wid)
            p.wait(timeout=4)
            assert p.returncode == 0, f"{binary}: failed to close cleanly"
        finally:
            if p.poll() is None:
                run(exe, "quit", env=env)
                p.wait(timeout=4)
print(f"Manual opening, no recording, close-to-exit: passed. Screenshots: {output}")
