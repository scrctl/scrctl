use clap::Parser;
use scrctl_ios::{DdiOptions, Error};
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
    #[arg(long, conflicts_with_all = ["list_apps", "copy", "paste", "download_ddi", "install_ddi"])]
    pair: bool,
    #[arg(long, requires = "pair", default_value_t = 120000, value_parser = clap::value_parser!(u32).range(1..=300000))]
    pairing_timeout: u32,
    #[arg(long, requires = "pair")]
    repair_pairing: bool,
    #[arg(long, conflicts_with_all = ["list_apps", "copy", "paste", "install_ddi"])]
    download_ddi: bool,
    #[arg(long, conflicts_with_all = ["list_apps", "copy", "paste"])]
    install_ddi: bool,
    #[arg(long, requires = "install_ddi")]
    replace_ddi: bool,
    #[arg(long, requires = "install_ddi", default_value_t = 300, value_parser = clap::value_parser!(u32).range(1..=3600))]
    ddi_install_timeout: u32,
    #[arg(long, requires = "download_ddi", conflicts_with_all = ["serial", "wifi"])]
    ddi_system_version: Option<String>,
    #[arg(long, requires = "ddi_system_version")]
    ddi_product_type: Option<String>,
    #[arg(long)]
    ddi_directory: Option<String>,
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..=3600))]
    ddi_download_timeout: Option<u32>,
}

pub fn run_if_supported(args: &[OsString]) -> Option<i32> {
    let options = Options::try_parse_from(
        std::iter::once(OsString::from("scrctl")).chain(args.iter().cloned()),
    )
    .ok()?;
    let ddi = options.download_ddi || options.install_ddi;
    if !ddi && (options.ddi_directory.is_some() || options.ddi_download_timeout.is_some()) {
        return None;
    }
    if options.pair
        && !options.wifi.is_empty()
        && (options.wifi != "auto" || options.repair_pairing)
    {
        return None;
    }
    if ddi
        && (options
            .ddi_system_version
            .as_deref()
            .is_some_and(|version| {
                let parts: Vec<_> = version.split('.').collect();
                parts.len() > 3
                    || parts
                        .iter()
                        .any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()))
            })
            || options
                .ddi_product_type
                .as_deref()
                .is_some_and(|name| name.trim().is_empty())
            || options
                .ddi_directory
                .as_deref()
                .is_some_and(|path| path.trim().is_empty()))
    {
        return None;
    }

    let result = if options.pair {
        scrctl_ios::pair_device(
            &options.serial,
            !options.wifi.is_empty(),
            options.repair_pairing,
            options.pairing_timeout,
        )
        .map(|udid| println!("Pairing saved by xdb: {udid}"))
    } else if ddi {
        run_ddi(&options)
    } else if options.list_apps || options.copy.is_some() || options.paste {
        scrctl_ios::device_commands(
            0,
            &options.serial,
            &options.wifi,
            options.wifi_port,
            options.list_apps,
            options.copy.as_deref(),
            options.paste,
        )
    } else {
        return None;
    };
    Some(match result {
        Ok(()) => 0,
        Err(Error::Client(message) | Error::Server { message, .. }) => {
            eprintln!("{message}");
            1
        }
    })
}

fn run_ddi(options: &Options) -> Result<(), Error> {
    let absolute;
    let directory = match &options.ddi_directory {
        Some(path) => {
            absolute = std::path::absolute(path).map_err(|e| Error::Client(e.to_string()))?;
            absolute
                .to_str()
                .ok_or_else(|| Error::Client("Invalid DDI cache path".into()))?
        }
        None => "",
    };
    let ddi = DdiOptions {
        udid: &options.serial,
        wifi: &options.wifi,
        wifi_port: options.wifi_port,
        directory,
        download_timeout: options.ddi_download_timeout.unwrap_or(300),
    };
    if options.install_ddi {
        scrctl_ios::install_ddi(ddi, options.replace_ddi, options.ddi_install_timeout)?;
        println!("DDI prepared by xdb");
    } else {
        let value = scrctl_ios::download_ddi(
            ddi,
            options.ddi_system_version.as_deref(),
            options.ddi_product_type.as_deref().unwrap_or(""),
        )?;
        println!("{value}");
    }
    Ok(())
}
