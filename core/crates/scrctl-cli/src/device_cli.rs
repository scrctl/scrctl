#[cfg(not(feature = "sdl3"))]
use clap::Parser;
#[cfg(not(feature = "sdl3"))]
use std::ffi::OsString;
use xdb_engine::{DdiOptions, Error};

#[derive(clap::Args)]
pub(crate) struct Target {
    /// Select the device by UDID
    #[arg(short = 's', long, default_value = "")]
    pub(crate) serial: String,
    /// LAN address or auto to discover a paired device
    #[arg(long, default_value = "")]
    pub(crate) wifi: String,
    /// RemotePairing port for a manual LAN address
    #[arg(long, default_value_t = 49152, value_parser = clap::value_parser!(u16).range(1..), requires = "wifi")]
    pub(crate) wifi_port: u16,
}

#[derive(clap::Args)]
pub(crate) struct Commands {
    /// List device apps
    #[arg(long)]
    list_apps: bool,
    /// List USB and RemotePairing devices
    #[arg(long, conflicts_with = "pair")]
    list_devices: bool,
    /// Wireless discovery timeout in milliseconds; 0 lists USB only
    #[arg(long, requires = "list_devices", default_value_t = 3000, value_parser = clap::value_parser!(u32).range(..=60000))]
    discovery_timeout: u32,
    /// Write device clipboard and exit
    #[arg(long)]
    copy: Option<String>,
    /// Read device clipboard; with --copy, read it back after writing
    #[arg(long)]
    paste: bool,
    /// Pair over USB or use --wifi auto for PIN pairing
    #[arg(long, conflicts_with_all = ["list_apps", "copy", "paste", "download_ddi", "install_ddi"])]
    pair: bool,
    /// Pairing timeout in milliseconds
    #[arg(long, requires = "pair", default_value_t = 120000, value_parser = clap::value_parser!(u32).range(1..=300000))]
    pairing_timeout: u32,
    /// Allow replacing an incomplete or rejected USB pairing record
    #[arg(long, requires = "pair")]
    repair_pairing: bool,
    /// Download and verify the required developer disk image
    #[arg(long, conflicts_with_all = ["list_apps", "copy", "paste", "install_ddi"])]
    download_ddi: bool,
    /// Download and install the required developer disk image
    #[arg(long, conflicts_with_all = ["list_apps", "copy", "paste"])]
    install_ddi: bool,
    /// Replace the device’s mounted developer disk image
    #[arg(long, requires = "install_ddi")]
    replace_ddi: bool,
    /// DDI installation timeout in seconds
    #[arg(long, requires = "install_ddi", default_value_t = 300, value_parser = clap::value_parser!(u32).range(1..=3600))]
    ddi_install_timeout: u32,
    /// iOS or iPadOS version for offline DDI download
    #[arg(long, requires = "download_ddi", conflicts_with_all = ["serial", "wifi"])]
    ddi_system_version: Option<String>,
    /// Device model for offline DDI manifest checking
    #[arg(long, requires = "ddi_system_version")]
    ddi_product_type: Option<String>,
    /// Override the developer disk image cache directory
    #[arg(long)]
    ddi_directory: Option<String>,
    /// DDI download timeout in seconds
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..=3600))]
    ddi_download_timeout: Option<u32>,
}

#[cfg(not(feature = "sdl3"))]
#[derive(Parser)]
#[command(disable_help_flag = true)]
struct Options {
    #[command(flatten)]
    target: Target,
    #[command(flatten)]
    commands: Commands,
}

#[cfg(not(feature = "sdl3"))]
pub fn run_if_supported(args: &[OsString]) -> Option<i32> {
    let options = Options::try_parse_from(
        std::iter::once(OsString::from("scrctl")).chain(args.iter().cloned()),
    )
    .ok()?;
    options
        .commands
        .validate(&options.target, !options.target.serial.is_empty())?;
    options.commands.run(&options.target)
}

impl Commands {
    pub(crate) fn is_ddi(&self) -> bool {
        self.download_ddi || self.install_ddi
    }

    #[cfg(feature = "sdl3")]
    pub(crate) fn is_requested(&self) -> bool {
        self.pair
            || self.is_ddi()
            || self.list_devices
            || self.list_apps
            || self.copy.is_some()
            || self.paste
    }

    #[cfg(feature = "sdl3")]
    pub(crate) fn pair_requested(&self) -> bool {
        self.pair
    }

    pub(crate) fn validate(&self, target: &Target, serial_explicit: bool) -> Option<()> {
        let ddi = self.download_ddi || self.install_ddi;
        if !ddi && (self.ddi_directory.is_some() || self.ddi_download_timeout.is_some()) {
            return None;
        }
        if self.pair && !target.wifi.is_empty() && (target.wifi != "auto" || self.repair_pairing) {
            return None;
        }
        if ddi
            && ((self.ddi_system_version.is_none()
                && target.serial.trim().is_empty()
                && serial_explicit)
                || self.ddi_system_version.as_deref().is_some_and(|version| {
                    let parts: Vec<_> = version.split('.').collect();
                    parts.len() > 3
                        || parts
                            .iter()
                            .any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()))
                })
                || self
                    .ddi_product_type
                    .as_deref()
                    .is_some_and(|name| name.trim().is_empty())
                || self
                    .ddi_directory
                    .as_deref()
                    .is_some_and(|path| path.trim().is_empty()))
        {
            return None;
        }

        Some(())
    }

    pub(crate) fn run(&self, target: &Target) -> Option<i32> {
        if self.list_devices {
            return Some(crate::list_devices(self.discovery_timeout));
        }
        let result = if self.pair {
            xdb_engine::pair_device(
                &target.serial,
                !target.wifi.is_empty(),
                self.repair_pairing,
                self.pairing_timeout,
            )
            .map(|udid| println!("Pairing saved by xdb: {udid}"))
        } else if self.is_ddi() {
            run_ddi(self, target)
        } else if self.list_apps || self.copy.is_some() || self.paste {
            xdb_engine::device_commands(
                0,
                &target.serial,
                &target.wifi,
                target.wifi_port,
                self.list_apps,
                self.copy.as_deref(),
                self.paste,
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
}

fn run_ddi(options: &Commands, target: &Target) -> Result<(), Error> {
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
        udid: &target.serial,
        wifi: &target.wifi,
        wifi_port: target.wifi_port,
        directory,
        download_timeout: options.ddi_download_timeout.unwrap_or(300),
    };
    if options.install_ddi {
        xdb_engine::install_ddi(ddi, options.replace_ddi, options.ddi_install_timeout)?;
        println!("DDI prepared by xdb");
    } else {
        let value = xdb_engine::download_ddi(
            ddi,
            options.ddi_system_version.as_deref(),
            options.ddi_product_type.as_deref().unwrap_or(""),
        )?;
        println!("{value}");
    }
    Ok(())
}
