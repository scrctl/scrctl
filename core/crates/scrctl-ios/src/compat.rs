#[path = "desktop.rs"]
mod desktop;
#[cfg(feature = "sdl3")]
mod sdl_frontend;

use std::{env, error::Error, ffi::OsString, io, path::PathBuf, process::Command};

fn main() {
    match launch() {
        Ok(status) => std::process::exit(status),
        Err(error) => {
            eprintln!("scrctl: {error}");
            std::process::exit(1);
        }
    }
}

fn launch() -> Result<i32, Box<dyn Error>> {
    let mut args = env::args_os().skip(1).peekable();
    // Only a leading selector belongs to this launcher. All legacy arguments
    // (including option values that look like flags) otherwise pass through.
    let target = match args.peek().and_then(|arg| arg.to_str()) {
        Some("--target-type") => {
            args.next();
            Some(args.next().unwrap_or_default())
        }
        Some(arg) if arg.starts_with("--target-type=") => args.next().and_then(|arg| {
            arg.into_string()
                .ok()
                .map(|arg| OsString::from(&arg["--target-type=".len()..]))
        }),
        _ => None,
    };
    match target.as_ref().map(|arg| arg.to_str()) {
        Some(Some("vnc")) => desktop::run(false, args),
        Some(Some("mac")) => desktop::run(true, args),
        Some(Some("ios")) | None => {
            let args: Vec<_> = args.collect();
            #[cfg(feature = "xdb")]
            if let Some(timeout) = discovery_timeout(&args) {
                return Ok(list_devices(timeout));
            }
            #[cfg(feature = "sdl3")]
            if let Some(result) = sdl_frontend::run_if_supported(&args) {
                return result.map(|()| 0);
            }
            launch_legacy(args.into_iter())
                .map_err(|error| format!("could not launch the C++ frontend: {error}").into())
        }
        _ => Err("expected --target-type ios, vnc, or mac".into()),
    }
}

fn launch_legacy(args: impl Iterator<Item = OsString>) -> io::Result<i32> {
    let frontend = match env::var_os("SCRCTL_LEGACY_EXECUTABLE") {
        Some(path) => PathBuf::from(path),
        None => {
            env::current_exe()?.with_file_name(format!("scrctl-legacy{}", env::consts::EXE_SUFFIX))
        }
    };
    let frontend = frontend.canonicalize()?;
    if frontend == env::current_exe()?.canonicalize()? {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "frontend points to this launcher",
        ));
    }

    let mut command = Command::new(frontend);
    command.args(args);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        Err(command
            .arg0(env::args_os().next().unwrap_or_else(|| "scrctl".into()))
            .exec())
    }
    #[cfg(not(unix))]
    command.status().map(|status| status.code().unwrap_or(1))
}

// Only dispatch complete, unambiguous invocations. The original parser is
// still responsible for mixed options, diagnostics, and alternate forms.
#[cfg(feature = "xdb")]
fn discovery_timeout(args: &[OsString]) -> Option<u32> {
    let value = match args {
        [flag] if flag == "--list-devices" => return Some(3000),
        [flag, option, value] if flag == "--list-devices" && option == "--discovery-timeout" => {
            value.to_str()?
        }
        [option, value, flag] if flag == "--list-devices" && option == "--discovery-timeout" => {
            value.to_str()?
        }
        [flag, option] if flag == "--list-devices" => {
            option.to_str()?.strip_prefix("--discovery-timeout=")?
        }
        [option, flag] if flag == "--list-devices" => {
            option.to_str()?.strip_prefix("--discovery-timeout=")?
        }
        _ => return None,
    };
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    value.parse().ok().filter(|value| *value <= 60_000)
}

#[cfg(feature = "xdb")]
fn list_devices(timeout: u32) -> i32 {
    use std::io::Write;
    let result = scrctl_ios::devices(0, "auto", timeout);
    let result = match result {
        Ok(result) => result,
        Err(scrctl_ios::Error::Server { message, .. } | scrctl_ios::Error::Client(message)) => {
            eprintln!("{message}");
            return 1;
        }
    };
    let (Some(devices), Some(warnings)) =
        (result["devices"].as_array(), result["warnings"].as_array())
    else {
        eprintln!("Invalid XDB devices response");
        return 1;
    };
    let mut out = io::stdout().lock();
    for device in devices {
        let field = |name| device[name].as_str().unwrap_or("");
        let Some(candidates) = device["candidates"].as_array() else {
            eprintln!("Invalid XDB device candidates");
            return 1;
        };
        if writeln!(out, "{}  {}", field("udid"), field("name")).is_err() {
            return 1;
        }
        for candidate in candidates {
            if writeln!(out, "  {candidate}").is_err() {
                return 1;
            }
        }
    }
    for warning in warnings.iter().filter_map(serde_json::Value::as_str) {
        eprintln!("{warning}");
    }
    0
}
