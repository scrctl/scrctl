"""Verify CLI target routing and native desktop windows against loopback RFB peers."""
import os
from pathlib import Path
import signal
import socket
import struct
import subprocess
import sys
import time

launcher = str(Path(sys.argv[1]).resolve())
env = {key: val for key, val in os.environ.items()
       if key not in ("SCRCTL_USERNAME", "SCRCTL_PASSWORD")}
env["SDL_VIDEO_DRIVER"] = "dummy"


def negotiate(conn, peer, apple):
    conn.settimeout(5)
    version = b"RFB 003.889\n" if apple else b"RFB 003.008\n"
    conn.sendall(version)
    assert peer.read(12) == version
    conn.sendall(b"\x01\x01")
    assert peer.read(1) == b"\x01"
    conn.sendall(bytes(4))
    assert peer.read(1) == (b"\xc1" if apple else b"\x01")
    name = b"Loopback"
    pixel_format = struct.pack(">BBBBHHHBBB3x", 32, 24, 0, 1, 255, 255, 255, 16, 8, 0)
    conn.sendall(struct.pack(">HH", 1, 1) + pixel_format + struct.pack(">I", len(name)) + name)
    if apple:
        assert peer.read(66)[0] == 0x21
    assert peer.read(20)[0] == 0
    set_encodings = peer.read(4)
    assert set_encodings[0] == 2
    encoding_bytes = int.from_bytes(set_encodings[2:], "big") * 4
    assert len(peer.read(encoding_bytes)) == encoding_bytes
    assert peer.read(10)[:2] == b"\x03\x00"


def update(conn, width, height, resize=False):
    header = struct.pack(">BBH", 0, 0, 2 if resize else 1)
    layout = struct.pack(">HHHHi", 0, 0, width, height, -223) if resize else b""
    frame = struct.pack(">HHHHi", 0, 0, width, height, 0)
    conn.sendall(header + layout + frame + b"\x00\x80\xff\x00" * (width * height))


def run(target, mode):
    apple = target == "mac"
    with socket.create_server(("127.0.0.1", 0)) as listener:
        listener.settimeout(5)
        port = listener.getsockname()[1]
        selector = ["--target-type", target] if not apple else ["--target-type=mac"]
        command = [launcher, *selector, "127.0.0.1", "--port", str(port), *mode]
        with subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env) as child:
            try:
                conn, _ = listener.accept()
                with conn, conn.makefile("rb") as peer:
                    negotiate(conn, peer, apple)
                    update(conn, 1, 1)
                    if mode == ["--probe"]:
                        assert len(peer.read()) <= 10
                    else:
                        assert peer.read(10)[0] == 3
                        update(conn, 4, 2, resize=True)
                        assert struct.unpack(">BBHHHH", peer.read(10))[4:] == (4, 2)
                        # Allow the UI loop to present the resized texture and
                        # ensure the prior diagnostic-only behavior didn't exit.
                        time.sleep(0.15)
                        assert child.poll() is None
                        if os.name == "posix":
                            child.send_signal(signal.SIGINT)
                            assert peer.read() == b""
                stdout, stderr = child.communicate(timeout=5)
                if mode == ["--probe"]:
                    assert child.returncode == 0, stderr.decode()
                    assert stdout == b"First frame: 1x1\n", stdout
                elif os.name == "posix":
                    assert child.returncode == 0, stderr.decode()
                else:
                    assert child.returncode == 1 and b"connection closed" in stderr, (stdout, stderr)
            finally:
                if child.poll() is None:
                    child.kill()
    print(f"{target} {' '.join(mode) or 'interactive'}: passed")


for target in ("vnc", "mac"):
    run(target, ["--probe"])
    if "--interactive" in sys.argv[2:]:
        run(target, [])
        run(target, ["--view-only"])

for value in ("", "=rdp"):
    result = subprocess.run([launcher, "--target-type" + value], env=env, capture_output=True, timeout=5)
    assert result.returncode != 0
    assert b"expected --target-type ios, vnc, or mac" in result.stderr
