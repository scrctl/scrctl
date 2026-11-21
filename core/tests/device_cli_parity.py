"""Compare migrated device and headless paths with the installed legacy CLI."""
import os
from pathlib import Path
import subprocess
import sys

root = Path(__file__).resolve().parents[2]
fixture = Path(os.getenv("SCRCTL_TEST_XDB", root / "xdb"))
sys.path.insert(0, str(fixture / "tests" / "support"))
from fake_server import FakeServer

legacy, launcher = sys.argv[1:]
for args, required in (
    (["--list-apps"], ["app.list"]),
    (["--wifi", "auto", "--copy", "测试🙂", "--paste"], ["clipboard.set", "clipboard.get"]),
    (["--copy", "", "--paste"], ["clipboard.set", "clipboard.get"]),
    (["--no-window", "--no-audio", "--exit-after", "3"], ["video.start", "video.frame"]),
):
    observed = []
    for command in (legacy, launcher):
        with FakeServer() as peer:
            env = dict(
                os.environ, **peer.environment, SCRCTL_LEGACY_EXECUTABLE="/nonexistent",
                SDL_VIDEO_DRIVER="nonexistent", SDL_VIDEODRIVER="nonexistent",
            )
            result = subprocess.run([command, *args], env=env, capture_output=True, timeout=15)
            assert result.returncode == 0, result
            assert all(method in peer.methods() for method in required), peer.calls
            observed.append((result.stdout, result.stderr))
    assert observed[0] == observed[1], (args, observed)
    print("PASS", args)
