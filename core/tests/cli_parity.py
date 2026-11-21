"""Compare the Rust launcher and C++ frontend on Unix without a device."""
import os
import subprocess
import sys
from pathlib import Path

legacy, launcher = (str(Path(path).resolve()) for path in sys.argv[1:])
env = dict(os.environ, SCRCTL_LEGACY_EXECUTABLE=legacy)
for args in (
    ["--version"],
    ["--lang", "en", "--help"],
    ["--definitely-invalid"],
    ["--audio-buffer", "-1"],
    ["--ddi-product-type", "iPad11,2"],
):
    direct = subprocess.run(["scrctl", *args], executable=legacy, env=env, capture_output=True)
    for selector in ([], ["--target-type", "ios"], ["--target-type=ios"]):
        forwarded = subprocess.run(
            ["scrctl", *selector, *args], executable=launcher, env=env, capture_output=True,
        )
        if (direct.returncode, direct.stdout, direct.stderr) != (
            forwarded.returncode, forwarded.stdout, forwarded.stderr
        ):
            raise SystemExit(f"CLI parity failed for {selector + args}")
