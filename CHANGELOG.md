# Changelog

[English](CHANGELOG.md) | [简体中文](CHANGELOG.zh-CN.md)

## 2.1.0 — 2026-11-28

- Add `--request-permission` for Mac Screen Sharing. The person at the remote Mac can approve a connection without the viewer supplying account credentials.
- Use the dark blue dual-screen icon for CLI windows.
- Improve Mac Screen Sharing cursor updates, keyboard symbols, and first-frame reliability.

## 2.0.0 — 2026-11-23

- Replace the C++ CLI frontend with Rust and use SDL3 for windows, input, and audio output. iPhone and iPad mirroring, control, and recording continue to use XDeviceBridge.
- iOS is the default target. Add interactive VNC and Mac Screen Sharing sessions with `--target-type vnc` and `--target-type mac`. Use `--view-only` to disable remote input or `--probe` to verify the first frame and exit.
- Expose in-session RFB quality changes and Apple file transfer capability checks through the Core bindings.
- Remove `--play` for local raw HEVC playback and the CLI's FFmpeg dependency. Existing recordings can be opened in a compatible player.

## 1.4.0 — 2026-11-21

- Update the embedded XDeviceBridge client to 1.7.0.
- Prefer hardware decoding for live video through xdb Server. Use
  `--no-hw-decode` to select software decoding.

## 1.3.1 — 2026-11-01

- Keep macOS video windows at the picture aspect ratio when dragging their borders; update the constraint after rotation.
- Redraw the uploaded frame during native window resizing, even before the main event loop resumes or a new frame arrives, avoiding a stretched old picture.
- Fullscreen continues to fit the picture proportionally with borders where needed.

## 1.3.0 — 2026-10-30

- Add `--video-bit-rate` / `-b` with decimal K/M suffixes to request a device video bitrate ceiling.
- Add `--max-fps` to limit receiver image conversion/publication; device encoding and recording frame rates remain unchanged.
- Preserve the first shared stream's bitrate and show warnings for differing requests.
- Update the embedded XDeviceBridge client to 1.6.0. Default video behavior is unchanged.

## 1.2.0 — 2026-10-29

- Update the embedded XDeviceBridge client to 1.4.0.

- Forward up to five touchscreen contacts with xdb Server 1.4.0 or later.
  Preserve individual finger identities and release held contacts on focus or
  geometry changes. Older Servers retain single-finger compatibility.
- Match scrcpy mouse gestures: Ctrl + left drag for pinch/rotation, Shift for
  vertical two-finger slides, and Ctrl + Shift for horizontal slides.

## 1.1.0 — 2026-10-29

- Negotiate shared control for ordinary mirrors when supported by xdb Server.
  Independent windows share one video producer and can both control the device;
  closing one does not finalize another's media. Recording remains exclusive,
  audio has one consuming window, and older Servers retain legacy behavior.
- Update the embedded XDeviceBridge client to the published 1.3.0 release.
- Record the actual embedded xdb version in corresponding source materials.

## 1.0.1 — 2026-10-27

- Update the embedded XDeviceBridge client to 1.1.0. Upgrade and restart the
  separate `xdb` Server to use native continuous discovery and DDI improvements;
  existing pairing records are retained.
- Preserve explicit empty clipboard writes (`--copy ""`) with newer CLI11 versions.
- Provide English and Chinese user documentation as separate, linked pages.
- Fix Windows ARM64 CI setup by disabling its optional MSYS2 package cache.

Offline checks include all four macOS client/Server 1.0.0 and 1.1.0 combinations.
This update does not add new live-device or OS-version acceptance claims.

## 1.0.0 — 2026-10-25

First stable release. The separate XDeviceBridge 1.0 server handles device
protocols and media processing; scrctl provides the desktop window, input
forwarding and audio playback. Install `xdb` separately before use.

- USB / Wi-Fi mirroring and control, mDNS discovery, device selection and wireless PIN pairing.
- Wireless re-pairing can replace a device's old record after validating the new one.
- On-demand DDI download, native installation and status checks.
- Computer audio, duplicate playback, disabled audio and configurable buffering.
- Keyboard, explicit clipboard paste, mouse touch, device buttons, rotation and display pause.
- MP4 / MKV A/V recording and raw HEVC output, preserving original encoded streams without re-encoding.
- Startup cancellation, timeouts, English/Chinese messages and cross-platform builds.
- macOS ARM64 and Windows ARM64 / x64 runtime packages with corresponding dependency sources, recipes and patches.

### Scope and limitations

- Primary live-media device records are from an iPhone 13 mini on iOS 27. An
  iPad mini 5 on iPadOS 18.7.8 supported screenshot polling and control, but rejected
  live video and audio.
- Windows x64 passed builds, offline tests and package checks; x64 device
  connections have not been accepted on hardware. Debian ARM64 device testing is
  recorded; this release provides no standalone Linux runtime archive.
- Unicode text uses explicit clipboard paste, which can trigger iOS permission.
  Ordinary input forwards keyboard events rather than automatically pasting IME text.
- Recording uses fixed initial orientation metadata. Later rotation retains the
  original encoded canvas without automatic pixel rotation or black borders.
- Changing audio routes can pause the player once; resume playback if needed.
- macOS release packages are Developer ID signed and published after Apple
  notarization returns Accepted.
- Embedded window icon and frame handoff without a full-frame copy under the lock.

See the [README](README.md) and linked original records for device, system and
platform validation boundaries.
