# scrctl

[English](README.md) | [简体中文](README.zh-CN.md)

Mirror and control iOS devices over USB or Wi-Fi from a native desktop window.
Inspired by scrcpy, with video, audio, mouse and keyboard control, explicit
clipboard paste, and recording without re-encoding.

scrctl is the desktop frontend. [XDeviceBridge (xdb)](https://github.com/scrctl/XDeviceBridge)
handles device discovery, pairing, Developer Disk Images (DDI), media capture,
decoding and recording in a separate local server process.

## Install

### Homebrew (macOS and Linux)

```sh
brew install scrctl/tap/scrctl
```

This also installs `xdb`. Compatible bottles are used when available; otherwise
Homebrew builds the pinned release sources. The tap's bottles are separate builds
from the Developer ID notarized macOS release ZIP.

### Release packages

Download [scrctl 1.1.0](https://github.com/scrctl/scrctl/releases/tag/v1.1.0)
and the matching [XDeviceBridge runtime](https://github.com/scrctl/XDeviceBridge/releases).

- scrctl packages: macOS ARM64, Windows ARM64 and Windows x64.
- macOS release ZIPs are Developer ID signed and Apple notarized.
- Preserve the archive's `bin`, private libraries and `share` directories.
- Put `xdb` on `PATH`, or set `XDB_EXECUTABLE` to its absolute path.
- Download the matching source archive and verify `SHA256SUMS.txt` as needed.

DDI images are downloaded when needed, not bundled in the runtime.

## Quick start

Enable Developer Mode on the device. For USB, unlock it and accept the trust
prompt. Discover and prepare the selected device:

```sh
scrctl --list-devices
scrctl --pair -s '<UDID>'
scrctl --install-ddi -s '<UDID>'
scrctl -s '<UDID>'
```

For wireless PIN pairing, keep the device and computer reachable on the same
network, run the command below, then select the computer in the device's
**Settings → Privacy & Security → Developer Mode** page and enter the displayed PIN:

```sh
scrctl --pair --wifi auto
scrctl -s '<UDID>' --wifi auto
```

If discovery is blocked, use `--wifi <address>` and, if needed, `--wifi-port`.
A manually specified address still requires a valid pairing record.
xdb starts on demand and listens on loopback only.

## Shared mirror windows

With xdb Server 1.3.0 or later advertising `shared_control_sessions`, ordinary
mirrors (without `--record`) automatically share one video producer. Open scrctl twice
for the same device: both windows can control it, and closing one preserves the
other. Touch gestures are serialized between windows; keyboard/button
holds are tracked per window. Video decoding options must match. Audio plays
through one window; the other continues with video/control. Closing the audio
window does not automatically move sound to the other. Recording remains
exclusive. Older Servers retain their original exclusive behavior.

With xdb Server 1.4.0 or later, touchscreens forward up to five fingers.
Mouse shortcuts match scrcpy: Ctrl + left drag simulates pinch/rotation, Shift +
left drag simulates a two-finger vertical slide, and Ctrl + Shift + left drag
simulates a horizontal slide. Hold the modifiers when pressing the mouse button;
releasing the mouse lifts both fingers. Older Servers keep single-finger mouse input.

## Common operations

```sh
scrctl --video-bit-rate=2M          # Request device video bitrate ceiling (-b 2M)
scrctl --max-fps=15                 # Receiver output limit; device encoding/recording unchanged
scrctl --audio-buffer=200           # More buffering for uneven delivery
scrctl --audio-dup                  # Keep sound on both device and computer
scrctl --no-audio                   # Leave device audio routing unchanged
scrctl --record capture.mkv         # Original HEVC + AAC-ELD tracks
scrctl --record capture.mp4 --no-audio
scrctl --video-source screenshot    # Screenshot polling; no audio/encoded recording
scrctl --lang en --help
```

Video rate options require an XDB Server advertising the corresponding capability.
Omitted bitrate preserves device/shared-stream settings; 0 requests device defaults.
The device may negotiate a different bitrate. Shared video keeps the first stream's
bitrate and warns on different requests. Shared `--max-fps` requests use the highest
rate (unlimited wins); it is not a per-window traffic limit.

`MOD` means left Alt or left Super/Command by default (`--shortcut-mod` changes it).

| Action | Shortcut |
| --- | --- |
| Quit / fullscreen | MOD+Q / MOD+F or F11 |
| Touch / drag | Left mouse button |
| Pinch / rotate | Ctrl + left drag |
| Two-finger vertical / horizontal slide | Shift + left drag / Ctrl + Shift + left drag |
| Home / lock / volume | MOD+H / MOD+P / MOD+↑ or ↓ |
| Paste host clipboard | MOD+V |
| Request device rotation | MOD+R |
| Rotate desktop view | MOD+← or → |
| Freeze / refresh frozen view / resume | MOD+Z / MOD+Z / MOD+Shift+Z |

Normal keys are forwarded as physical keyboard events using the device's keyboard
layout and input method. Host IME text is not automatically pasted. Explicit paste
supports Unicode and may prompt for iOS clipboard permission.

## Compatibility and limits

The primary tested device is an iPhone 13 mini (`iPhone14,4`) running iOS 27.x.
An iPad mini 5 on iPadOS 18.7.8 supported screenshot polling and control, but rejected
live video/audio. These observations do not establish support for every device
running those system versions. DDI availability also does not prove that a device
exposes the required services.

Recording preserves the device's encoded stream. It does not re-encode, resize,
or add black borders. Containers use a fixed initial orientation; later device
rotation does not dynamically change it. Switching audio routes may pause the
player once; resume playback on the device if necessary.

## Documentation

- [User guide](docs/USER_GUIDE.md): installation, pairing, DDI, controls, audio,
  recording, building and troubleshooting.
- [Changelog](CHANGELOG.md).
- [XDB installation and SDK documentation](https://github.com/scrctl/XDeviceBridge).
- Original research and acceptance records remain in their original language:
  [protocol notes](docs/coredevice.md), [refactoring notes](docs/REFACTOR.md),
  [historical README and validation details](docs/README_HISTORY.zh-CN.md).

The CLI language remains locale-based (`--lang auto`); `--lang en` or
`--lang zh-CN` overrides it. English is the default documentation language.

## Build from source

```sh
git clone --recurse-submodules https://github.com/scrctl/scrctl.git
cd scrctl
# macOS prerequisites:
brew install cmake sdl2-compat ffmpeg pkgconf gettext
cmake -S . -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build --parallel 4
ctest --test-dir build --output-on-failure
./build/scrctl --help
```

Building scrctl builds only the pinned XDB client library. Install the `xdb`
server separately; upgrade it to 1.1.0 and restart it to use native discovery
subscriptions and the DDI improvements. Existing pairing records are retained.
See the [build guide](docs/USER_GUIDE.md#build-from-source) for Linux,
Windows and offline dependency options.

## License

scrctl is licensed under [Apache-2.0](LICENSE). XDeviceBridge is a separate
LGPL-3.0-only project. Bundled dependencies have their own licenses; see
[third-party notices](THIRD_PARTY_NOTICES.md) and each package's source materials.
