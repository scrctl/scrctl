"""Run the actual scrctl frontend through the public XDB1 protocol."""
import os
from pathlib import Path
import subprocess
import sys

root = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(root / 'xdb/tests/support'))
from fake_server import FakeServer

exe = str(Path(sys.argv[1]).resolve())
with FakeServer() as server:
    env = {**os.environ, **server.environment, 'SDL_VIDEODRIVER':'dummy', 'SDL_AUDIODRIVER':'dummy', 'SDL_RENDER_DRIVER':'software'}
    def run(*args):
        result = subprocess.run([exe, '--lang', 'en', *args], env=env, capture_output=True, text=True, encoding="utf-8", timeout=12)
        assert result.returncode == 0, (args, result.stdout, result.stderr)
        return result.stdout
    run('--list-devices', '--discovery-timeout', '0')
    # Actual background BGRA receiver, ASCII input and finalized media ownership.
    run('-s', 'mock-iphone', '--no-window', '--no-audio', '--exit-after', '2', '--test-type', 'abc123')
    assert 'video.frame' in server.methods() and 'input.text' in server.methods()
    # PNG decoding occurs in the server; the UI receives upright BGRA geometry.
    run('-s', 'mock-iphone', '--video-source', 'screenshot', '--no-window', '--no-audio', '--exit-after', '2')
    assert any(name == 'screenshot' and p.get('format') == 'bgra' for name,p in server.calls)
    run('-s', 'mock-iphone', '--copy', '中文✓🙂')
    assert server.clipboard == '中文✓🙂', repr(server.clipboard)
    pasted = run('-s', 'mock-iphone', '--paste').strip()
    assert pasted == '中文✓🙂', repr(pasted)
    # The audio callback reads only the local queue; a separate attached client polls PCM.
    run('-s', 'mock-iphone', '--no-window', '--no-video', '--time-limit', '1', '--audio-dup', '--audio-buffer', '200')
    assert 'audio.read' in server.methods()
    assert any(name == 'audio.start' and p['duplicate'] and p['buffer_ms'] == 200 for name,p in server.calls)
    # Server-side encoded recording remains usable without local decode or playback.
    run('-s', 'mock-iphone', '--no-window', '--no-playback', '--record', str(root/'build-xdb/mock-recording.mkv'), '--time-limit', '1')
    assert 'recording.configure' in server.methods()
    assert any(name == 'video.start' and p['decode'] is False for name,p in server.calls)
    assert 'media.finish' in server.methods()
    assert not {'pair', 'ddi.prepare'} & set(server.methods())

    # A capable Server shares ordinary mirrors. Closing one must not issue the
    # legacy process-wide media.finish; recording still requests exclusivity.
    server.shared_control = True
    start = len(server.calls)
    run('-s', 'mock-iphone', '--no-window', '--no-audio', '--exit-after', '2')
    shared_calls = server.calls[start:]
    assert any(name == 'connection.open' and p.get('access') == 'shared' for name, p in shared_calls)
    assert not any(name == 'media.finish' for name, _ in shared_calls)
    assert any(name == 'connection.close' for name, _ in shared_calls)
    start = len(server.calls)
    run('-s', 'mock-iphone', '--no-window', '--no-playback', '--record', str(root/'build-xdb/mock-recording.mkv'), '--time-limit', '1')
    assert any(name == 'connection.open' and 'access' not in p for name, p in server.calls[start:])
    assert any(name == 'media.finish' for name, _ in server.calls[start:])

class FrameFailureServer(FakeServer):
    def __init__(self, disconnect):
        self.disconnect = disconnect
        self.frame_requests = 0
        super().__init__()

    def handle_call(self, name, params):
        result = super().handle_call(name, params)
        if name == 'video.frame':
            self.frame_requests += 1
            if self.frame_requests == 2:
                if self.disconnect:
                    # The C++ client must classify EOF without inspecting its message.
                    raise ConnectionResetError('test peer disconnect')
                raise ValueError('temporary socket capture error')
        return result


for disconnect in (False, True):
    with FrameFailureServer(disconnect) as server:
        env = {**os.environ, **server.environment, 'SDL_VIDEODRIVER': 'dummy',
               'SDL_AUDIODRIVER': 'dummy', 'SDL_RENDER_DRIVER': 'software'}
        result = subprocess.run(
            [exe, '--lang', 'en', '-s', 'mock-iphone', '--no-window', '--no-audio',
             '--exit-after', '3'], env=env, capture_output=True, text=True, timeout=8)
        assert result.returncode == (1 if disconnect else 0), (result.stdout, result.stderr)
        assert server.frame_requests == 2 if disconnect else server.frame_requests >= 4

print('PASS: scrctl BGRA/PNG, ASCII, explicit clipboard, PCM, recording and coded frame failures')
