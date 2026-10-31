# User guide

[English](USER_GUIDE.md) | [简体中文](USER_GUIDE.zh-CN.md) | [README](../README.md)

This guide describes the scrctl 1.0 desktop frontend with the separate XDB server.
Research notes and historical acceptance logs are retained in their original
language and are not a replacement for these current instructions.

## Installation and updates

```sh
brew install scrctl/tap/scrctl
brew update
brew upgrade scrctl/tap/xdb scrctl/tap/scrctl
```

The tap provides source formulae and, for tested platforms, bottles. xdb is a
runtime dependency of scrctl. No background login service is installed: clients
start xdb on demand. Use `xdb kill-server` before switching between separately
installed server versions; the next client request starts the selected server.

Alternatively, download both projects' runtime packages from their GitHub releases.
Keep each package intact and put `xdb` on `PATH`. `XDB_EXECUTABLE` overrides server
lookup. Verify release archives against `SHA256SUMS.txt`; corresponding source
archives are shipped alongside the runtime packages.

The macOS ARM64 release ZIP is Developer ID signed and Apple notarized. ZIPs and
bare executables cannot carry stapled tickets; macOS may need network access to
check Apple's ticket. Homebrew bottles are separate builds and do not inherit
that ZIP's notarization.

On Windows, install the appropriate Apple Mobile Device USB driver for USB access.
Wi-Fi avoids the USB driver requirement. On Linux, USB requires a running usbmuxd
service and suitable device permissions, normally provided by the distribution.
A VM must own the forwarded USB device; seeing it in a USB inventory alone does
not establish a working device-protocol connection.

## Device discovery and selection

```sh
scrctl --list-devices
scrctl --list-devices --discovery-timeout=0
scrctl -s '<UDID>'
scrctl -s '<UDID>' --wifi auto
scrctl -s '<UDID>' --wifi 192.168.1.50 --wifi-port 49152
```

`-s` selects the Apple device UDID. `--wifi auto` requests wireless discovery;
`--wifi <address>` bypasses discovery. A device may have several IPv4/IPv6 and USB
network candidates; these do not imply several phones or several pairings.
Discovery is a snapshot, not proof that every listed address is reachable.

Without an explicit selection, the connection must be unambiguous. A failed USB
connection is not silently retried over Wi-Fi. Explicitly use `--wifi auto` when
that is the connection you want.

## Pairing and DDI

1. Enable Developer Mode on the device, completing any requested reboot.
2. For USB, unlock the device and accept **Trust This Computer**.
3. Run `scrctl --pair -s '<UDID>'` to establish or verify pairing.
4. Run `scrctl --install-ddi -s '<UDID>'` to check and prepare the developer image.
5. Start `scrctl -s '<UDID>'`.

For first-time wireless PIN pairing:

```sh
scrctl --pair --wifi auto
```

Open **Settings → Privacy & Security → Developer Mode** on the phone, select the
advertised computer and enter the PIN displayed by scrctl. Unlock/authorize on the
device when prompted. If the phone revoked a pairing while the computer retained
its record, repeat the PIN workflow; the new record is saved only after validation.
There is no need to manually remove the old file first.

DDI preparation belongs to xdb. It reuses a suitable existing mount, otherwise
downloads and verifies the selected image and requests installation on the device.
After a system update, prepare DDI again if required services are missing.
`--replace-ddi` explicitly requests replacement rather than ordinary reuse:

```sh
scrctl --install-ddi -s '<UDID>' --wifi auto
scrctl --install-ddi --replace-ddi -s '<UDID>'
```

The current catalog distinguishes Classic images (iOS/iPadOS 16 and earlier),
Personalized images (17–26), and Cryptex images (27). Catalog membership or a
successful mount does not prove that screen, audio or input services are available.
Old Classic systems have no established CoreDevice feature acceptance here.
See the [XDB documentation](https://github.com/scrctl/XDeviceBridge) for current
catalog, identity, cache, installation and SDK details. Pairing credentials and
DDI caches are user data, separate from the installed application.

## Window and keyboard

`MOD` defaults to left Alt or left Super/Command. For example,
`--shortcut-mod=lalt` reserves only left Alt for desktop shortcuts, allowing
Command combinations to reach the phone.

| Action | Input |
| --- | --- |
| Quit | MOD+Q or close the window; Ctrl+C in the terminal |
| Fullscreen | MOD+F or unmodified F11 |
| Touch / drag | Left mouse button |
| Home | MOD+H or middle mouse button |
| Lock / side button | MOD+P |
| Volume | MOD+↑ / ↓; repeated host events support holding the key |
| Restore pixel size / remove letterboxing | MOD+G / MOD+W |
| Rotate desktop view | MOD+← / → |
| Flip desktop view horizontally | MOD+Shift+← / → |
| Flip desktop view vertically | MOD+Shift+↑ / ↓ |
| Request portrait/landscape on the device | MOD+R |
| Freeze / refresh frozen picture | MOD+Z |
| Resume live display | MOD+Shift+Z |
| Paste host clipboard | MOD+V |

Normal Q and Escape do not quit. Physical keys use the device's keyboard layout
and IME; host IME text is not automatically injected. Explicit paste supports
Unicode but the target app may require permission to paste from `dtpasteboardd`,
Apple's remote clipboard service. Focus loss and geometry changes release held
input. `--no-control` disables device input.

Device rotation requires an app that supports rotation. It does not override the
phone's orientation lock. Desktop rotation/flip affects display and touch mapping,
not the device's encoded recording.

On macOS, dragging a video window's border preserves the displayed picture's
aspect ratio, including after rotation. Fullscreen fits the picture without
stretching and may leave black borders. Resize events redraw the current frame
even when no new video frame has arrived.

## Audio

| Option | Behavior |
| --- | --- |
| Default | Forward audio to the computer and silence the device speaker |
| `--audio-dup` | Play on both device and computer |
| `--no-audio` | Skip capture and routing; preserve existing device playback state |
| `--no-audio-playback` | Disable computer playback; still capture for an audio recording track |
| `--audio-buffer=200` | Increase the playback buffer from the default 50 ms |

Changing routes may pause the player once. Resume on the device if necessary,
including after exiting. The program does not automatically press Play or change
the device volume. Device volume can also affect captured sound, so setting it to
zero is not a substitute for default computer-only routing.

Without a container audio recording, disabling audio playback also disables audio
capture. Raw HEVC has no audio track. `--audio-dup` requires audio capture to remain
enabled. Screenshot-polling mode does not capture audio.

## Recording and orientation

```sh
scrctl -r capture.mp4
scrctl -r capture.mkv --no-audio
scrctl -r capture.mkv --audio-dup --no-audio-playback
scrctl -r capture --record-format=mkv
scrctl --no-window --no-playback -r capture.mp4
scrctl --no-window --no-video --audio-dup --no-audio-playback -r audio.mkv
scrctl -r capture.hevc
```

MP4/MKV preserve original HEVC and AAC-ELD packets without re-encoding. Raw HEVC
stores video only. Container playback requires a player with suitable codec and
orientation support; `scrctl --play file.hevc` only plays raw HEVC files.

Recording uses a fixed initial device orientation. Later rotation preserves the
encoded canvas and does not update that metadata. There is no automatic pixel
rotation, resizing, black-border insertion or `--record-orientation=auto`.

```sh
scrctl -r capture.mp4 --record-orientation=270
scrctl -r capture.mp4 --display-orientation=90
scrctl -r capture.mp4 --orientation=90
```

The first changes only fixed container rotation metadata, the second only the
window, and the third both. Numeric rotation is 0/90/180/270 degrees clockwise;
none requires re-encoding. Nonzero recording rotation is unavailable for raw HEVC;
recording flips are unsupported. Display-only `flip0/90/180/270` is supported.

A transport discontinuity, unsupported configuration change, disk error or switch
to screenshot capture can end recording with an error. scrctl does not silently
join a restarted media session into the same file. Wait for finalization before
using the recording; an error may leave an incomplete file.

## Build from source

Use a Git checkout with submodules. The server is built and installed separately.

```sh
git clone --recurse-submodules https://github.com/scrctl/scrctl.git
cd scrctl
# macOS
brew install cmake sdl2-compat ffmpeg pkgconf gettext
# Debian / Ubuntu instead:
# sudo apt install build-essential cmake pkg-config gettext python3 libsdl2-dev \
#   libavcodec-dev libavutil-dev libswscale-dev
cmake -S . -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build --parallel 4
ctest --test-dir build --output-on-failure
```

Windows builds use MSYS2 CLANG64 (x64) or CLANGARM64 (ARM64), with the matching
CMake, compiler, SDL2, FFmpeg, gettext and Python packages; see the exact
[CI recipe](../.github/workflows/offline-selftest.yml).

Missing JSON/CLI11 dependencies are downloaded from pinned sources by default.
For offline builds, set `SCRCTL_FETCH_DEPENDENCIES=OFF` and supply installed packages
or `FETCHCONTENT_SOURCE_DIR_NLOHMANN_JSON` / `FETCHCONTENT_SOURCE_DIR_CLI11`.
`SCRCTL_LIBAV=OFF` disables local FFmpeg file playback, and
`SCRCTL_BUILD_TESTS=OFF` disables frontend tests. Live decoding remains in xdb.

Runtime archives include corresponding source deliveries, dependency recipes,
patches and checksums. Check the shipped FFmpeg configuration and licenses;
“no re-encoding” does not remove FFmpeg distribution obligations or establish
bit-for-bit reproducible builds.

## Troubleshooting

- **No device:** unlock it, confirm trust, verify USB ownership/driver/usbmuxd, or
  explicitly try Wi-Fi. Do not delete pairing records as the first diagnostic step.
- **No live media on an older device:** try `--video-source screenshot`. A working
  screenshot service does not imply live audio/video support.
- **Services missing after an update:** check/prepare DDI, then consider explicit
  replacement. Device model and service capability both matter.
- **Audio gaps:** try `--audio-buffer=200`; buffering trades latency for continuity.
- **Wrong server version or install:** check `command -v xdb` (Windows: `where xdb`)
  and `XDB_EXECUTABLE`; stop the old server before starting the new one.
- **Need diagnostics:** use `scrctl --stats` and `scrctl --lang en --help`.

Original experiment and acceptance detail remains available in
[protocol notes](coredevice.md), [refactoring notes](REFACTOR.md), and
[the historical README](README_HISTORY.zh-CN.md). Those files retain their original
language and revision-specific scope.
