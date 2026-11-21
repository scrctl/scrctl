"""Exercise the Rust session through the real client ABI and a loopback XDB fixture."""
import subprocess
import sys
from pathlib import Path

root = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(root / "xdb" / "tests" / "support"))
from fake_server import FakeServer


def run(executable, scenario):
    with FakeServer() as server:
        server.no_video = scenario == "start-error"
        server.bad_frame = scenario == "bad-frame"
        subprocess.run(
            [executable, str(server.server_address[1]), scenario], check=True, timeout=10
        )
        expected = [
            ("hello", {"protocol": 1}),
            ("connection.open", {"udid": "mock-iphone", "transport": "wifi"}),
            ("video.start", {"decode": True, "hardware_decode": False}),
        ]
        if scenario != "start-error":
            expected.append(("video.frame", {"since": 0, "timeout_ms": 100}))
        if scenario == "success":
            expected.extend([
                ("input.touch", {"x": 0.25, "y": 0.75, "down": True}),
                ("input.touch", {"x": 0.25, "y": 0.75, "down": False}),
                ("input.keyboard", {"usages": [4]}),
                ("input.keyboard", {"usages": []}),
            ])
        expected.append(("connection.close", {}))
        assert server.calls == expected, (scenario, server.calls)
        print(f"PASS {scenario}")


for scenario in ("success", "start-error", "bad-frame"):
    run(sys.argv[1], scenario)
