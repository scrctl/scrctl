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
