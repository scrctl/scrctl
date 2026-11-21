"""Run the public Rust SDL entry with dummy output and the local XDB fixture."""
import os
from pathlib import Path
import subprocess
import sys
import signal
import time

root = Path(__file__).resolve().parents[2]
fixture = Path(os.getenv("SCRCTL_TEST_XDB", root / "xdb"))
sys.path.insert(0, str(fixture / "tests" / "support"))
from fake_server import FakeServer

class VideoLimitsServer(FakeServer):
    def handle_call(self, name, params):
        result, data = super().handle_call(name, params)
        if name == "server.info":
            result["capabilities"].update(video_max_bitrate=True, video_max_fps=True)
        return result, data


for server_class in (VideoLimitsServer, FakeServer):
    with server_class() as peer:
        env = dict(os.environ, **peer.environment, SDL_VIDEO_DRIVER="dummy",
                   SCRCTL_LEGACY_EXECUTABLE="/nonexistent")
        result = subprocess.run(
            [sys.argv[1], "--exit-after", "5", "--no-audio", "-b", "2m",
             "--max-fps", "30", "--window-x", "-10", "--window-y", "auto",
             "--always-on-top", "--disable-screensaver", "--render-driver", "software",
             "--background-color", "#123", "--display-orientation", "flip90",
             "--crop", "2x2+0+0", "--scale", "2", "--hw-decode", "--video-source", "stream"],
            env=env, capture_output=True, text=True, timeout=15,
        )
        if server_class is VideoLimitsServer:
            assert result.returncode == 0 and "processed 5 frames" in result.stdout, result
            assert ("video.start", {"decode": True, "hardware_decode": True,
                                    "max_bitrate": 2000000, "max_fps": 30}) in peer.calls, peer.calls
        else:
            assert result.returncode == 1 and "does not support" in result.stderr, result
            assert "connection.open" not in peer.methods() and "video.start" not in peer.methods()
print("PASS video limits, capability checks and SDL window options")


for extra in (["--audio-dup", "--audio-buffer", "200"], ["--no-audio", "--no-control"], ["--no-audio-playback"]):
    with FakeServer() as peer:
        env = dict(
            os.environ, **peer.environment,
            SCRCTL_LEGACY_EXECUTABLE="/nonexistent",
            SDL_VIDEO_DRIVER="dummy", SDL_AUDIO_DRIVER=os.getenv("SCRCTL_TEST_AUDIO_DRIVER", "dummy"),
        )
        result = subprocess.run(
            [sys.argv[1], "--exit-after", "5", *extra],
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



with FakeServer() as peer:
    env = dict(os.environ, **peer.environment, SDL_VIDEO_DRIVER="dummy",
               SDL_AUDIO_DRIVER="dummy", SCRCTL_LEGACY_EXECUTABLE="/nonexistent")
    result = subprocess.run(
        [sys.argv[1], "--exit-after", "5", "--record", "window.mkv",
         "--record-orientation", "180", "--orientation", "flip0", "--record-orientation", "90",
         "--orientation", "auto", "--no-audio-playback"],
        env=env, capture_output=True, text=True, timeout=15,
    )
    assert result.returncode == 0 and "processed 5 frames" in result.stdout, result
    config = next(params for method, params in peer.calls if method == "recording.configure")
    assert (config["orientation"], config["orientation_from_device"], config["audio"]) == (90, False, True)
    assert peer.methods()[-2:] == ["media.finish", "connection.close"], peer.calls
    print("PASS windowed recording with explicit orientation")


with FakeServer() as peer:
    env = dict(os.environ, **peer.environment, SDL_VIDEO_DRIVER="dummy",
               SCRCTL_LEGACY_EXECUTABLE="/nonexistent")
    result = subprocess.run(
        [sys.argv[1], "-N", "--no-control", "--time-limit", "1", "--record", "quiet.mkv"],
        env=env, capture_output=True, text=True, timeout=15,
    )
    assert result.returncode == 0 and "encoded video recording finalized" in result.stdout, result
    assert ("video.start", {"decode": False, "hardware_decode": False}) in peer.calls
    assert ("audio.start", {"decode": False, "duplicate": False, "buffer_ms": 50}) in peer.calls
    assert "video.frame" not in peer.methods() and "audio.read" not in peer.methods()
    assert peer.methods()[-2:] == ["media.finish", "connection.close"], peer.calls
    print("PASS disabled playback preserves encoded audio and video recording")


if os.name == "posix":
    for extra, active in (([], "video.frame"), (["--no-video-playback"], "media.status")):
        with FakeServer() as peer:
            env = dict(os.environ, **peer.environment, SDL_VIDEO_DRIVER="dummy",
                       SCRCTL_LEGACY_EXECUTABLE="/nonexistent")
            with subprocess.Popen([sys.argv[1], "--no-audio", *extra], env=env,
                                  stdout=subprocess.PIPE, stderr=subprocess.PIPE) as child:
                try:
                    deadline = time.monotonic() + 5
                    while active not in peer.methods():
                        assert child.poll() is None and time.monotonic() < deadline, peer.calls
                        time.sleep(0.01)
                    child.send_signal(signal.SIGINT)
                    out, err = child.communicate(timeout=10)
                    assert child.returncode == 0, (out, err)
                    assert peer.methods()[-1] == "connection.close", peer.calls
                    assert ("video.start" in peer.methods()) == (not extra), peer.calls
                finally:
                    if child.poll() is None:
                        child.kill()
    print("PASS unlimited video and background window startup and exit")


if os.name == "posix":
    with FakeServer() as peer:
        env = dict(os.environ, **peer.environment, SCRCTL_LEGACY_EXECUTABLE="/nonexistent")
        args = [sys.argv[1], "--no-window", "--no-video", "--no-audio-playback",
                "--record", "audio.mkv"]
        with subprocess.Popen(args, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE) as child:
            try:
                deadline = time.monotonic() + 5
                while "media.status" not in peer.methods():
                    assert child.poll() is None and time.monotonic() < deadline, peer.calls
                    time.sleep(0.01)
                child.send_signal(signal.SIGINT)
                out, err = child.communicate(timeout=10)
                assert child.returncode == 0 and b"audio recording finalized" in out, (out, err)
                assert peer.methods()[-2:] == ["media.finish", "connection.close"], peer.calls
                assert "video.start" not in peer.methods() and "audio.read" not in peer.methods()
                print("PASS unlimited audio-only recording finalizes on SIGINT")
            finally:
                if child.poll() is None:
                    child.kill()

if len(sys.argv) > 2:
    with FakeServer() as peer:
        env = dict(os.environ, SCRCTL_INPUT_FIXTURE_PORT=str(peer.server_address[1]))
        subprocess.run(
            [*sys.argv[2:], "--ignored", "forwards_sdl_input_and_releases_on_focus_loss"],
            check=True, env=env, timeout=30,
        )
        assert peer.calls[3:] == 2 * [
            ("input.touch", {"x": 0.5, "y": 0.75, "down": True}),
            ("input.keyboard", {"usages": [4]}),
            ("input.touch", {"x": 0.5, "y": 0.75, "down": False}),
            ("input.keyboard", {"usages": []}),
        ] + [
            ("input.button", {"page": 12, "usage": 233, "down": True}),
            ("input.button", {"page": 12, "usage": 233, "down": True}),
            ("input.button", {"page": 12, "usage": 233, "down": False}),
            ("input.button", {"page": 12, "usage": 233, "down": True}),
            ("input.touch", {"contact": 1, "x": 0.75, "y": 0.25, "down": True}),
            ("input.touch", {"contact": 1, "x": 0.75, "y": 0.25, "down": False}),
            ("input.button", {"page": 12, "usage": 233, "down": False}),
            ("display.rotate", {}),
            ("connection.close", {}),
        ], peer.calls
        print("PASS SDL input and focus loss")


    with FakeServer() as peer:
        env = dict(os.environ, SCRCTL_INPUT_FIXTURE_PORT=str(peer.server_address[1]))
        subprocess.run(
            [*sys.argv[2:], "--ignored", "clipboard_readback_and_stale_paste"],
            check=True, env=env, timeout=30,
        )
        assert ("clipboard.set", {"text": "scrctl 夹具"}) in peer.calls
        assert [params["usages"] for method, params in peer.calls if method == "input.keyboard"] == [
            [227], [227, 25], [],
        ], peer.calls
        first_paste = peer.methods().index("input.keyboard")
        assert "clipboard.get" in peer.methods()[:first_paste], peer.calls
        print("PASS clipboard readback and stale-paste suppression")
