"""Compare native Rust discovery and the legacy CLI using the XDB fixture."""
import os
from pathlib import Path
import subprocess
import sys

root = Path(__file__).resolve().parents[2]
fixture = Path(os.getenv("SCRCTL_TEST_XDB", root / "xdb"))
sys.path.insert(0, str(fixture / "tests" / "support"))
from fake_server import FakeServer

legacy, launcher = sys.argv[1:]
for args, timeout in (
    (["--list-devices"], 3000),
    (["--list-devices", "--discovery-timeout", "0"], 0),
    (["--discovery-timeout=12", "--list-devices"], 12),
):
    with FakeServer() as peer:
        env = dict(os.environ, **peer.environment)
        old = subprocess.run([legacy, *args], capture_output=True, env=env, timeout=15)
        # A successful run proves discovery did not delegate to the old binary.
        env["SCRCTL_LEGACY_EXECUTABLE"] = "/nonexistent"
        new = subprocess.run(
            [launcher, "--target-type=ios", *args],
            capture_output=True, env=env, timeout=15,
        )
        assert (new.returncode, new.stdout, new.stderr) == (0, old.stdout, old.stderr), (old, new)
        assert old.returncode == 0, old
        assert peer.calls[-1] == ("devices", {"timeout_ms": timeout, "transport": "auto"}), peer.calls
        print(f"PASS {' '.join(args)}")
