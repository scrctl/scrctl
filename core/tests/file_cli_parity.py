"""Compare local software HEVC playback with the legacy CLI."""
import os
from pathlib import Path
import subprocess
import struct
import sys
from tempfile import TemporaryDirectory

legacy, rust = sys.argv[1:]
env = dict(os.environ, SCRCTL_LEGACY_EXECUTABLE="/nonexistent", SDL_VIDEO_DRIVER="dummy")

with TemporaryDirectory() as directory:
    root = Path(directory)
    clip = root / "playback.hevc"
    subprocess.run([
        "ffmpeg", "-hide_banner", "-loglevel", "error", "-f", "lavfi",
        "-i", "testsrc2=size=128x96:rate=60", "-frames:v", "5", "-c:v", "libx265",
        "-x265-params", "log-level=error:bframes=0", "-f", "hevc", str(clip),
    ], check=True, timeout=30)

    for path, options, code in (
        (clip, [], 0),
        (clip, ["--exit-after", "3"], 0),
        (root / "missing.hevc", [], 1),
        (root, [], 1),
    ):
        args = ["--no-window", "--play", str(path), *options]
        old, new = (subprocess.run([exe, *args], env=env, capture_output=True, timeout=10)
                    for exe in (legacy, rust))
        assert old.returncode == code, old
        assert old.stdout.splitlines() == new.stdout.splitlines(), (old, new)
        assert old.stderr.splitlines() == new.stderr.splitlines(), (old, new)
        assert new.returncode == code, new
        print(f"PASS {path.name} {options} exit={code}")

    windowed = subprocess.run(
        [rust, "--play", str(clip), "--exit-after", "3"],
        env=env, capture_output=True, timeout=10,
    )
    assert windowed.returncode == 0 and b"processed 3 frames" in windowed.stdout, windowed
    print("PASS SDL3 dummy window playback")

    window_options = [
        "--play", str(clip), "--lang", "en", "--crop", "64:32:4:8",
        "--orientation", "180", "--display-orientation", "flip90",
        "--window-borderless", "--window-width", "90", "--window-height", "100",
        "--background-color", "#123", "--exit-after", "3",
    ]
    for frontend in (legacy, rust):
        bmp = root / ("readback-" + ("rust" if frontend == rust else "legacy") + ".bmp")
        result = subprocess.run(
            [frontend, *window_options, "--verify", "2", str(bmp)],
            env=dict(env, SDL_VIDEODRIVER="dummy"), capture_output=True, timeout=10,
        )
        assert result.returncode == 0 and b"processed 3 frames" in result.stdout, result
        data = bmp.read_bytes()
        assert data.startswith(b"BM") and struct.unpack_from("<ii", data, 18) == (90, 100)
        offset = struct.unpack_from("<I", data, 10)[0]
        assert data[offset:offset + 3] == bytes.fromhex("332211")
    print("PASS cropped and rotated file playback honors window size and background")

    if sys.platform == "darwin" and os.getenv("SCRCTL_TEST_HW_DECODE") == "1":
        args = ["--no-window", "--play", str(clip), "--hw-decode"]
        old, new = (subprocess.run([exe, *args], env=env, capture_output=True, timeout=15)
                    for exe in (legacy, rust))
        assert (old.returncode, old.stdout, old.stderr) == (0, new.stdout, new.stderr), (old, new)
        assert new.returncode == 0, new
        windowed = subprocess.run([rust, "--play", str(clip), "--hw-decode", "--exit-after", "3"],
                                  env=env, capture_output=True, timeout=15)
        assert windowed.returncode == 0 and b"processed 3 frames" in windowed.stdout, windowed
        print("PASS VideoToolbox hardware frames and SDL3 texture upload")
