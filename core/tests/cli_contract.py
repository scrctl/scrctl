"""Check the installed iOS CLI's public options without a device."""
import os
from pathlib import Path
import subprocess
import sys

scrctl = str(Path(sys.argv[1]).resolve())
env = dict(os.environ, LC_ALL="C")


def run(args, **locale):
    return subprocess.run([scrctl, *args], env={**env, **locale},
                          capture_output=True, text=True, encoding="utf-8", timeout=10)


for selector in ([], ["--target-type", "ios"], ["--target-type=ios"]):
    result = run([*selector, "--version"])
    assert result.returncode == 0 and result.stdout.startswith("scrctl "), result
    assert not result.stderr, result

for args, locale, chinese in (
    (["--lang=en", "--help"], {"LC_ALL": "zh_CN.UTF-8"}, False),
    (["--lang", "en", "-h"], {}, False),
    (["--help", "--lang=zh-CN"], {}, True),
    (["-h"], {"LC_ALL": "zh_CN.UTF-8"}, True),
):
    result = run(args, **locale)
    assert result.returncode == 0 and not result.stderr, result
    if chinese:
        assert "iOS 屏幕镜像与控制" in result.stdout and "显示帮助" in result.stdout, result
    else:
        assert "iOS screen mirroring and control" in result.stdout, result
    for option in ("--wifi", "--list-devices", "--pair", "--install-ddi", "--play",
                   "--record", "--record-orientation", "--audio-buffer", "MOD+V"):
        assert option in result.stdout, (option, result)


for args, option in (
    (["--definitely-invalid"], "--definitely-invalid"),
    (["--audio-buffer", "-1"], "-1"),
    (["--ddi-product-type", "Model"], "--ddi-product-type"),
    (["--wifi=auto", "--wifi-port=49152"], "--wifi-port"),
    (["--test-touch", "0.25,0.5"], "--test-touch"),
    (["--record", "probe.mp4", "--record-orientation", "flip0"], "--record-orientation"),
    (["--no-window", "--verify", "2", "probe.bmp"], "--verify"),
    (["--verify", "0", "probe.bmp"], "--verify"),
    (["--no-video", "--exit-after", "3"], "--no-video"),
    (["--no-video", "--record", "probe.hevc"], "--record"),
    (["--no-video", "--no-audio", "--record", "probe.mkv"], "--record"),
    (["--video-source", "screenshot", "--record", "probe.mkv"], "--record"),
    (["--no-window", "--no-audio", "--max-fps", "10"], "--max-fps"),
    (["--no-audio-playback", "--audio-dup"], "--audio-dup"),
    (["--download-ddi", "--no-audio"], "--download-ddi"),
):
    result = run(args)
    assert result.returncode == 2 and not result.stdout and option in result.stderr, result

for args in (["--title", "zh-CN", "--lang", "en"], ["--audio-buffer", "1001"]):
    result = run(["--version", *args])
    assert result.returncode == 0 and result.stdout.startswith("scrctl "), result

print("PASS iOS selectors, help, language, aliases and argument validation")
