use clap::{Parser, ValueEnum};
use std::time::{Duration, Instant};

#[derive(Clone, ValueEnum)]
enum Transport {
    Auto,
    Usb,
    Wifi,
}

#[derive(Parser)]
#[command(about = "Receive a first iOS video frame through XDeviceBridge")]
struct Args {
    #[arg(short = 's', long, default_value = "")]
    serial: String,
    #[arg(long, value_enum, default_value = "auto")]
    transport: Transport,
    #[arg(long, default_value_t = 0)]
    xdb_port: u16,
    #[arg(long)]
    no_hw_decode: bool,
}

fn main() -> std::process::ExitCode {
    match run(Args::parse()) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run(args: Args) -> Result<(), scrctl_ios::Error> {
    let transport = match args.transport {
        Transport::Auto => "auto",
        Transport::Usb => "usb",
        Transport::Wifi => "wifi",
    };
    let mut session =
        scrctl_ios::Session::open(args.xdb_port, &args.serial, transport, !args.no_hw_decode)?;
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if let Some(frame) = session.next_frame(0, 100)? {
            println!(
                "BGRA {}x{} (pitch {})",
                frame.width, frame.height, frame.row_pitch
            );
            return Ok(());
        }
    }
    Err(scrctl_ios::Error::Client(
        "No iOS frame within five seconds".into(),
    ))
}
