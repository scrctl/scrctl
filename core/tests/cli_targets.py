"""Verify CLI target routing against local RFB peers."""
import os
from pathlib import Path
import socket
import struct
import subprocess
import sys

launcher = str(Path(sys.argv[1]).resolve())
env = {key: val for key, val in os.environ.items()
       if key not in ("SCRCTL_USERNAME", "SCRCTL_PASSWORD")}
env["SCRCTL_LEGACY_EXECUTABLE"] = launcher

for target, version, client_init in (
    ("vnc", b"RFB 003.008\n", b"\x01"),
    ("mac", b"RFB 003.889\n", b"\xc1"),
):
    with socket.create_server(("127.0.0.1", 0)) as listener:
        listener.settimeout(5)
        port = listener.getsockname()[1]
        selector = ["--target-type", target] if target == "vnc" else ["--target-type=mac"]
        command = [launcher, *selector, "127.0.0.1", "--port", str(port)]
        with subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env) as child:
            try:
                conn, _ = listener.accept()
                with conn, conn.makefile("rb") as peer:
                    conn.settimeout(5)
                    conn.sendall(version)
                    assert peer.read(12) == version
                    conn.sendall(b"\x01\x01")
                    assert peer.read(1) == b"\x01"
                    conn.sendall(bytes(4))
                    assert peer.read(1) == client_init
                    name = b"Loopback"
                    pixel_format = struct.pack(">BBBBHHHBBB3x", 32, 24, 0, 1, 255, 255, 255, 16, 8, 0)
                    conn.sendall(struct.pack(">HH", 1, 1) + pixel_format + struct.pack(">I", len(name)) + name)
                    if target == "mac":
                        assert peer.read(66)[0] == 0x21
                    assert peer.read(20)[0] == 0
                    set_encodings = peer.read(4)
                    assert set_encodings[0] == 2
                    encoding_bytes = int.from_bytes(set_encodings[2:], "big") * 4
                    assert len(peer.read(encoding_bytes)) == encoding_bytes
                    assert peer.read(10)[:2] == b"\x03\x00"
                    conn.sendall(struct.pack(">BBHHHHHi", 0, 0, 1, 0, 0, 1, 1, 0) + b"\x00\x80\xff\x00")
                    assert len(peer.read()) <= 10
                stdout, stderr = child.communicate(timeout=5)
                assert child.returncode == 0, stderr.decode()
                assert stdout == b"First frame: 1x1\n", stdout
            finally:
                if child.poll() is None:
                    child.kill()
    print(f"{target}: negotiated, received first frame, disconnected")

for value in ("", "=rdp"):
    result = subprocess.run([launcher, "--target-type" + value], env=env, capture_output=True, timeout=5)
    assert result.returncode != 0
    assert b"expected --target-type ios, vnc, or mac" in result.stderr
