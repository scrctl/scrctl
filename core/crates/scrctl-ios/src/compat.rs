// During migration the existing frontend remains the authority for its CLI.
use std::{env, io, path::PathBuf, process::Command};

fn main() {
    match launch() {
        Ok(status) => std::process::exit(status),
        Err(error) => {
            eprintln!("scrctl: could not launch the C++ frontend: {error}");
            std::process::exit(1);
        }
    }
}

fn launch() -> io::Result<i32> {
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
    command.args(env::args_os().skip(1));
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
