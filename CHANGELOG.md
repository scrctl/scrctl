# Changelog

[English](CHANGELOG.md) | [简体中文](CHANGELOG.zh-CN.md)

## Unreleased

- Negotiate shared control for ordinary mirrors when supported by xdb Server.
  Independent windows share one video producer and can both control the device;
  closing one does not finalize another's media. Recording remains exclusive,
  audio has one consuming window, and older Servers retain legacy behavior.

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
