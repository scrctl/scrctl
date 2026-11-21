"""Compare the Rust launcher and C++ frontend on Unix without a device."""
import os
import re
import subprocess
import sys
from pathlib import Path

legacy, launcher = (str(Path(path).resolve()) for path in sys.argv[1:])
env = dict(os.environ, SCRCTL_LEGACY_EXECUTABLE=legacy, LC_ALL="C")
native = os.getenv("SCRCTL_TEST_NATIVE_OPTIONS") == "1"
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
        if native and "--help" in args:
            options = lambda output: set(re.findall(rb"(?m)^\s*(?:-[a-zA-Z],?\s+)?(--[a-z][-a-z]*)", output.split(b"With no arguments")[0]))
            assert direct.returncode == forwarded.returncode == 0 and not forwarded.stderr
            assert options(direct.stdout) == options(forwarded.stdout) - {b"--hw-decode", b"--title"}
            continue
        if (direct.returncode, direct.stdout, direct.stderr) != (
            forwarded.returncode, forwarded.stdout, forwarded.stderr
        ):
            raise SystemExit(f"CLI parity failed for {selector + args}")

if native:
    native_env = dict(env, SCRCTL_LEGACY_EXECUTABLE="/nonexistent")
    for args in (
        ["--version"],
        ["--title", "zh-CN", "--lang", "en", "--version"],
        ["--wifi=auto", "--wifi-port=49152"],
        ["--test-touch", "0.25,0.5"],
        ["--record", "probe.mp4", "--record-orientation", "flip0"],
    ):
        old = subprocess.run([legacy, *args], env=env, capture_output=True, timeout=10)
        new = subprocess.run([launcher, *args], env=native_env, capture_output=True, timeout=10)
        assert (new.returncode, new.stdout, new.stderr) == (old.returncode, old.stdout, old.stderr), (old, new)
        print("PASS native argument handling", *args)

    for args in (["--help", "--lang=en"], ["--lang", "en", "-h"]):
        result = subprocess.run([launcher, *args], env=native_env, capture_output=True, timeout=10)
        assert result.returncode == 0 and not result.stderr, result
        assert b"iOS screen mirroring and control" in result.stdout
        assert b"--play" in result.stdout and b"MOD+V" in result.stdout
        print("PASS native help", *args)
