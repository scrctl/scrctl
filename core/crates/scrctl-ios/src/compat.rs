#[path = "desktop.rs"]
mod desktop;

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
        Some(Some("ios")) | None => launch_legacy(args)
            .map_err(|error| format!("could not launch the C++ frontend: {error}").into()),
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
