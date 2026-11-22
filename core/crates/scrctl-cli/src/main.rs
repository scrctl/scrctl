#[path = "desktop.rs"]
mod desktop;
#[cfg(feature = "xdb")]
mod device_cli;
#[cfg(feature = "sdl3")]
mod diagnostics;
#[cfg(all(feature = "sdl3", feature = "ffmpeg-next"))]
mod file_cli;
#[cfg(feature = "sdl3")]
mod help;
#[cfg(feature = "sdl3")]
mod sdl_frontend;

#[cfg(feature = "xdb")]
use std::io;
use std::{env, error::Error, ffi::OsString};

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
    // Only a leading selector is consumed here. Leave the remaining CLI arguments intact.
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
            #[cfg(feature = "sdl3")]
            {
                sdl_frontend::run_cli(&args)
            }
            #[cfg(not(feature = "sdl3"))]
            {
                #[cfg(feature = "xdb")]
                if let Some(status) = device_cli::run_if_supported(&args) {
                    return Ok(status);
                }
                let _ = args;
                Err("iOS mirroring requires a build with SDL3".into())
            }
        }
        _ => Err("expected --target-type ios, vnc, or mac".into()),
    }
}

#[cfg(feature = "xdb")]
fn list_devices(timeout: u32) -> i32 {
    use std::io::Write;
    let result = xdb_engine::devices(0, "auto", timeout);
    let result = match result {
        Ok(result) => result,
        Err(xdb_engine::Error::Server { message, .. } | xdb_engine::Error::Client(message)) => {
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
