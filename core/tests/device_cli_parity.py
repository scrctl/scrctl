"""Compare migrated device and headless paths with the installed legacy CLI."""
import os
from pathlib import Path
import subprocess
import sys

root = Path(__file__).resolve().parents[2]
fixture = Path(os.getenv("SCRCTL_TEST_XDB", root / "xdb"))
sys.path.insert(0, str(fixture / "tests" / "support"))
from fake_server import FakeServer

class ManagementPeer(FakeServer):
    def handle_call(self, name, params):
        result, data = super().handle_call(name, params)
        if name == "pair":
            return {"udid": "mock-iphone"}, b""
        if name == "ddi.download":
            return {"cached": True, "path": "/fixture/DDI"}, b""
        return result, data


legacy, launcher = sys.argv[1:]
for args, required in (
    (["--pair", "--wifi", "auto", "--pairing-timeout", "100"], ["pair"]),
    (["--pair", "--repair-pairing", "-s", "mock-iphone"], ["pair"]),
    (["--download-ddi", "--ddi-system-version", "26.0", "--ddi-product-type", "TestPhone"], ["ddi.download"]),
    (["--download-ddi", "-s", "mock-iphone"], ["connection.open", "device.info", "connection.close", "ddi.download"]),
    (["--install-ddi", "--replace-ddi", "--ddi-install-timeout", "55"], ["devices", "ddi.prepare"]),
    (["--list-apps"], ["app.list"]),
    (["--wifi", "auto", "--copy", "测试🙂", "--paste"], ["clipboard.set", "clipboard.get"]),
    (["--copy", "", "--paste"], ["clipboard.set", "clipboard.get"]),
    (["--no-window", "--no-audio", "--exit-after", "3"], ["video.start", "video.frame"]),
    (["--no-window", "--no-video", "--no-audio-playback", "--time-limit", "1", "-r", "audio.m4kv", "--record-format", "mkv"], ["recording.configure", "audio.start", "media.finish"]),
    (["--no-window", "--no-audio", "--time-limit", "1", "-r", "encoded.mp4", "--record-orientation", "270"], ["recording.configure", "video.start", "media.finish"]),
    (["--no-window", "--audio-dup", "--exit-after", "3", "-r", "capture.mp4"], ["recording.configure", "video.start", "audio.start", "media.finish"]),
    (["--no-window", "--no-audio-playback", "--exit-after", "3", "--record", "capture.mkv"], ["recording.configure", "video.start", "audio.start", "media.finish"]),
    (["--no-window", "--no-audio", "--exit-after", "3", "--record", "capture.MKV"], ["recording.configure", "video.start", "media.finish"]),
    (["--no-window", "--no-audio", "--exit-after", "3", "-r", "capture.mp4", "--record-format", "hevc"], ["video.start", "media.finish"]),
):
    observed = []
    for command in (legacy, launcher):
        with ManagementPeer() as peer:
            env = dict(
                os.environ, **peer.environment, SCRCTL_LEGACY_EXECUTABLE="/nonexistent",
                SDL_VIDEO_DRIVER="nonexistent", SDL_VIDEODRIVER="nonexistent",
                SDL_AUDIO_DRIVER="dummy", SDL_AUDIODRIVER="dummy",
            )
            result = subprocess.run([command, *args], env=env, capture_output=True, timeout=15)
            assert result.returncode == 0, result
            assert all(method in peer.methods() for method in required), peer.calls
            calls = [call for call in peer.calls if call[0] in required]
            if "video.frame" in required:
                calls = []  # The old video polling cadence differs.
            stdout = result.stdout
            if "audio.start" in required and "--no-audio-playback" not in args:
                if command == launcher:
                    assert "audio.read" in peer.methods(), peer.calls
                stdout = b"\n".join(line for line in stdout.split(b"\n")
                                    if not line.startswith(b"Audio output opened:"))
            observed.append((stdout, result.stderr, calls))
    assert observed[0] == observed[1], (args, observed)
    print("PASS", args)
