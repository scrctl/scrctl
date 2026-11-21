use clap::Parser;
use std::ffi::OsString;

#[derive(Parser)]
#[command(disable_help_flag = true)]
struct Options {
    #[arg(short = 's', long, default_value = "")]
    serial: String,
    #[arg(long, default_value = "")]
    wifi: String,
    #[arg(long, default_value_t = 49152, value_parser = clap::value_parser!(u16).range(1..), requires = "wifi")]
    wifi_port: u16,
    #[arg(long)]
    list_apps: bool,
    #[arg(long)]
    copy: Option<String>,
    #[arg(long)]
    paste: bool,
}

pub fn run_if_supported(args: &[OsString]) -> Option<i32> {
    let options = Options::try_parse_from(
        std::iter::once(OsString::from("scrctl")).chain(args.iter().cloned()),
    )
    .ok()?;
    if !(options.list_apps || options.copy.is_some() || options.paste) {
        return None;
    }
    Some(
        match scrctl_ios::device_commands(
            0,
            &options.serial,
            &options.wifi,
            options.wifi_port,
            options.list_apps,
            options.copy.as_deref(),
            options.paste,
        ) {
            Ok(()) => 0,
            Err(scrctl_ios::Error::Client(message) | scrctl_ios::Error::Server { message, .. }) => {
                eprintln!("{message}");
                1
            }
        },
    )
}
