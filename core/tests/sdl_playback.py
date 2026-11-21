"""Run the public Rust SDL entry with dummy output and the local XDB fixture."""
import os
from pathlib import Path
import subprocess
import sys

root = Path(__file__).resolve().parents[2]
fixture = Path(os.getenv("SCRCTL_TEST_XDB", root / "xdb"))
sys.path.insert(0, str(fixture / "tests" / "support"))
from fake_server import FakeServer

for extra in (["--audio-dup", "--audio-buffer", "200"], ["--no-audio"], ["--no-audio-playback"]):
    with FakeServer() as peer:
        env = dict(
            os.environ, **peer.environment, SCRCTL_RUST_SDL="1",
            SCRCTL_LEGACY_EXECUTABLE="/nonexistent",
            SDL_VIDEODRIVER="dummy", SDL_AUDIODRIVER="dummy",
        )
        result = subprocess.run(
            [sys.argv[1], "--target-type", "ios", "--exit-after", "5", *extra],
            env=env, capture_output=True, text=True, timeout=15,
        )
        assert result.returncode == 0 and "processed 5 frames" in result.stdout, result
        assert peer.methods()[-1] == "connection.close", peer.calls
        if "--audio-dup" in extra:
            assert ("audio.start", {"decode": True, "duplicate": True, "buffer_ms": 200}) in peer.calls
            assert ("audio.read", {"frames": 960}) in peer.calls
            assert "Audio unavailable" not in result.stderr, result
        else:
            assert not any(method.startswith("audio.") for method in peer.methods()), peer.calls
        print("PASS", *extra)


if len(sys.argv) > 2:
    with FakeServer() as peer:
        env = dict(os.environ, SCRCTL_INPUT_FIXTURE_PORT=str(peer.server_address[1]))
        subprocess.run(
            [*sys.argv[2:], "--ignored", "forwards_sdl_input_and_releases_on_focus_loss"],
            check=True, env=env, timeout=30,
        )
        assert peer.calls[-5:] == [
            ("input.touch", {"x": 0.5, "y": 0.75, "down": True}),
            ("input.keyboard", {"usages": [4]}),
            ("input.touch", {"x": 0.5, "y": 0.75, "down": False}),
            ("input.keyboard", {"usages": []}),
            ("connection.close", {}),
        ], peer.calls
        print("PASS SDL input and focus loss")
