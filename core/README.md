# scrctl Rust core

The RFB crates were imported from scrctl/scrctl-gui at
`a8a21fa8f43654254ed387f0eafccd7bc9f76aee`. The existing `rfb-ffi` UniFFI
surface and bindgen crate remain the entry points for VNC and Mac Screen Sharing.

```sh
cargo test --manifest-path core/Cargo.toml --workspace --locked
```

The separate `scrctl-ios` crate has an opt-in `xdb` feature. It links the
XDeviceBridge client shared library and uses the existing local XDB Server.
Set `XDB_LIB_DIR` to the directory containing that library at build time and
make it visible to the platform loader at runtime. For example, on macOS with
the Homebrew XDB package:

```sh
export XDB_LIB_DIR="$(brew --prefix scrctl/tap/xdb)/lib"
cargo build --manifest-path core/Cargo.toml -p scrctl-ios --features xdb --locked
DYLD_LIBRARY_PATH="$XDB_LIB_DIR" core/target/debug/scrctl-rs --transport wifi
```

This development CLI opens a device session and prints the first decoded BGRA
frame's dimensions. The Rust session also exposes touch and keyboard methods.
The iOS crate currently has no UniFFI exports.

## CLI compatibility during migration

The workspace's Rust `scrctl` launcher forwards every argument to the existing
C++ frontend. Set `SCRCTL_LEGACY_EXECUTABLE` to the absolute path of that
binary, or place it beside the launcher as `scrctl-legacy` (with `.exe` on
Windows). Keep the original frontend's runtime libraries beside it. The
`scrctl-rs` first-frame probe remains a separate development command.

Run the iOS session regression against the XDB loopback fixture without a phone.
On macOS, reuse the library path from the example above:

```sh
cargo build --manifest-path core/Cargo.toml -p scrctl-ios --features xdb --example session_fixture --locked
DYLD_LIBRARY_PATH="$XDB_LIB_DIR" python3 core/tests/ios_session.py core/target/debug/examples/session_fixture
```

## Select a target type

Put the optional selector first. Existing iOS commands keep their arguments
and behavior, with or without an explicit iOS selector.

```sh
scrctl --target-type ios --wifi auto
scrctl --target-type vnc 192.0.2.10 --port 5900
scrctl --target-type mac mac.example
```

The desktop commands are connection checks for this stage. They use the Rust
RFB engine, print the first framebuffer size, close the session, and exit.
`mac` enables Apple Screen Sharing extensions. For hosts that require
authentication, provide `SCRCTL_USERNAME` and `SCRCTL_PASSWORD` in the environment.

The loopback CLI test checks RFB 3.8 and Apple RFB 3.889 negotiation, first
frame, and clean disconnect. Pass the path to the original C++ executable to
the parity test to compare legacy output and exit codes.

```sh
cargo build --manifest-path core/Cargo.toml -p scrctl-ios --bin scrctl --locked
python3 core/tests/cli_targets.py core/target/debug/scrctl
python3 core/tests/cli_parity.py /path/to/legacy/scrctl core/target/debug/scrctl
```

## Optional XDB GUI bindings

The default `rfb-ffi` build exports the existing VNC and Mac Screen Sharing
API. Enable `rfb-ffi/xdb` for the additional iOS-device API. Generate Swift
and Kotlin from the same feature-enabled library used by your app.

```sh
(
  cd core
  XDB_LIB_DIR=/opt/homebrew/opt/xdb/lib cargo build -p rfb-ffi --features xdb --locked
  cargo run -p rfb-ffi-bindgen --locked -- \
    generate --library target/debug/librfb_ffi.dylib \
    --language swift --language kotlin --out-dir target/gui-bindings --no-format
)
```

- `ios_list_devices` returns the XDB `devices` result JSON, including all
  candidates and discovery warnings. Select a non-empty UDID to open.
- `IosSession.open` starts decoded video for the selected device over
  auto, USB, or Wi-Fi. The port selects the **local** XDB Server. Zero
  uses XDB's default resolution on the host.
- `next_frame` returns owned BGRA bytes, pixel dimensions, row pitch, the
  frame serial, and optional display size and orientation. Pass the last
  accepted serial on subsequent calls. A no-frame response yields nil/null.
  The wait argument is clamped to 1000 ms and bounds the server's frame
  wait. It does not impose an end-to-end socket deadline.
- `touch` takes normalized device coordinates and a press/release flag.
  `keyboard` takes the current set of USB HID key usages (empty releases all).
  Call `shutdown` explicitly for an observable shutdown result. After closing,
  session methods report `IosError.Closed`.
- Calls are synchronous and serialized per session. Run them off the UI thread.
  Use short frame waits so input and close can run between frame requests.

On macOS the feature links the shared XDB client, with XDB Server installed
on the Mac. With a Rust static library, the app still needs to link and
provide the shared XDB client. For Android, use an ABI-matched NDK build of
`libxdb_client.so` and package it alongside the Rust JNI library. The Android
host must start an embedded `libxdb_server.so` via XDB's server API and pass its
actual loopback port. The binding does not start or own an embedded server.
Android packaging and device acceptance are follow-up integration work.

Apple iOS/iPadOS currently use the RFB-only feature set. XDB 1.7.0 has no
validated iOS host build or app embedding target. Neither binding connects
to an XDB Server on a different host.

### Standalone XDB module

Apps that already consume the default RFB bindings can build XDB separately
and keep it out of their iOS/iPadOS targets. On macOS, from `core/`:

```sh
XDB_LIB_DIR=/opt/homebrew/opt/xdb/lib cargo build -p xdb-ffi --features xdb --locked
cargo run -p rfb-ffi-bindgen --locked -- generate --library target/debug/libxdb_ffi.dylib \
  --language swift --language kotlin --out-dir target/xdb-bindings --no-format
```

This generates `xdb_ffi.swift` and `xdb_ffiFFI`, and Kotlin package
`uniffi.xdb_ffi`. Put the Swift bindings in a separate target from the RFB
bindings. Link the macOS target to `libxdb_ffi.dylib` and make the matching
shared XDB client available to the loader. The API matches the combined
module's optional XDB interface. Build the regular RFB module without its
`xdb` feature when using the standalone module.


### Rust CLI discovery

With `--features xdb`, the Rust launcher handles standalone `--list-devices`
and its canonical decimal `--discovery-timeout` form through the XDB C ABI.
Mixed options and invalid arguments use the legacy parser. The standard
feature-free launcher also uses the legacy parser. Run
`python3 core/tests/discovery_parity.py LEGACY RUST_LAUNCHER` with an initialized
XDB submodule and a loader path to the shared XDB client to compare discovery
output and exit status without contacting a device.

### Rust SDL3 mirror

Build with `sdl3` and an SDL3 installation discoverable via pkg-config.
With that build, the ordinary `scrctl` entry (or a leading `--target-type ios`)
uses Rust for supported invocations with a positive `--exit-after`:

```sh
XDB_LIB_DIR=/opt/homebrew/opt/xdb/lib cargo build --manifest-path core/Cargo.toml \
  -p scrctl-ios --bin scrctl --features sdl3 --locked
DYLD_LIBRARY_PATH=/opt/homebrew/opt/xdb/lib \
  core/target/debug/scrctl --wifi auto --exit-after 120
```

The Rust SDL3 path uses XDB for decoded PCM, with the default SDL playback
device. It handles `-s`, `--wifi`, `--wifi-port`, `--no-hw-decode`,
`--window-title` / `--title`, `--no-audio`, `--no-audio-playback`,
`--audio-dup`, `--audio-buffer`, `--exit-after`, `--no-window` and
`-n` / `--no-control`.
Left click, drag and physical keys are forwarded. Focus loss, geometry changes
and shutdown release active input.

Set `SCRCTL_RUST_SDL=1` to try an unlimited interactive session with this
subset; `SCRCTL_RUST_SDL=0` selects the legacy mirror. Unlimited sessions
default to C++ while its shortcuts, multi-touch and window behavior are
being ported. Recording, file playback, advanced window controls (including fullscreen/crop options),
and other unrecognized option combinations also use the legacy frontend.

```sh
DYLD_LIBRARY_PATH=/opt/homebrew/opt/xdb/lib XDB_LIB_DIR=/opt/homebrew/opt/xdb/lib \
  python3 core/tests/sdl_playback.py core/target/debug/scrctl \
  cargo test --manifest-path core/Cargo.toml -p scrctl-ios --bin scrctl --features sdl3 --locked --
```

The fixture uses SDL dummy video/audio by default. On macOS set
`SCRCTL_TEST_AUDIO_DRIVER=coreaudio` to exercise the system audio device
with silent test PCM.


### Device commands and headless regression

With XDB enabled, the launcher handles standalone `--list-apps`, `--copy`
(including an empty value) and `--paste`, with optional device selection.
The same route handles `--pair`, `--download-ddi`, and `--install-ddi`
with their standalone options. Management RPC events print pairing codes and
status messages on stderr. Combinations outside that parser, including `--lang`, use the incumbent
frontend. For `--no-window` with a positive `--exit-after`, Rust counts real
decoded frames and can play audio without initializing SDL video.

```sh
DYLD_LIBRARY_PATH=/opt/homebrew/opt/xdb/lib \
  python3 core/tests/device_cli_parity.py /opt/homebrew/bin/scrctl core/target/debug/scrctl
```

Bounded video-only recording (`--no-window --no-audio --exit-after N -r FILE`)
uses Rust for MP4, MKV and raw HEVC, including explicit `--record-format`.
XDB still writes the file. The launcher checks `media.finish` before reporting
success and closes the session even if finalization fails. Other recording
forms, including audio tracks and custom orientation, still use the C++
frontend. The loopback test checks RPC parameters and finalization order;
it does not produce an actual media file.
