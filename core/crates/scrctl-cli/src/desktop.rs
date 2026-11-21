use clap::Parser;
use rfb_engine::session::{Event, Session, SessionConfig};
use std::{env, error::Error, ffi::OsString, io, time::Duration};

#[derive(Parser)]
#[command(about = "Connect to a desktop, verify its first frame, and exit")]
struct Args {
    /// Remote hostname or IP address.
    host: String,
    /// Remote RFB port.
    #[arg(long, default_value_t = 5900, value_parser = clap::value_parser!(u16).range(1..))]
    port: u16,
}

pub fn run(apple: bool, args: impl Iterator<Item = OsString>) -> Result<i32, Box<dyn Error>> {
    let name = if apple {
        "scrctl --target-type mac"
    } else {
        "scrctl --target-type vnc"
    };
    let args = Args::parse_from(std::iter::once(OsString::from(name)).chain(args));
    let mut config = SessionConfig::new(args.host, args.port);
    config.apple_session = apple;
    config.adaptive_dct = apple;
    config.username = env::var("SCRCTL_USERNAME").ok();
    config.password = env::var("SCRCTL_PASSWORD").ok();

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let session =
            tokio::time::timeout(Duration::from_secs(15), Session::connect(config)).await??;
        let first = tokio::time::timeout(Duration::from_secs(15), first_frame(&session)).await;
        session.disconnect().await;
        let (width, height) = first??;
        println!("First frame: {width}x{height}");
        Ok(0)
    })
}

async fn first_frame(session: &Session) -> io::Result<(u16, u16)> {
    while let Some(event) = session.next_event().await {
        match event {
            Event::Frame { .. } => {
                return session
                    .size()
                    .ok_or_else(|| io::Error::other("missing framebuffer size"));
            }
            Event::Disconnected { reason, .. } => return Err(io::Error::other(reason)),
            _ => {}
        }
    }
    Err(io::Error::other("session ended before the first frame"))
}
