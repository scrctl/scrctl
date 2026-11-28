use clap::Parser;
use rfb_engine::handshake::MacSessionChoice;
use rfb_engine::session::{Event, Session, SessionConfig};
#[cfg(feature = "sdl3")]
#[path = "desktop/window.rs"]
mod window;

use std::{env, error::Error, ffi::OsString, io, time::Duration};

#[derive(Parser)]
#[command(about = "View and control a remote desktop")]
struct Args {
    /// Remote hostname or IP address.
    host: String,
    /// Remote RFB port.
    #[arg(long, default_value_t = 5900, value_parser = clap::value_parser!(u16).range(1..))]
    port: u16,
    /// Verify the first framebuffer update and disconnect.
    #[arg(long)]
    probe: bool,
    /// Request approval from the person using the remote Mac.
    #[arg(long)]
    request_permission: bool,
    /// Display the remote desktop without sending input.
    #[arg(long, alias = "no-control")]
    #[cfg_attr(not(feature = "sdl3"), allow(dead_code))]
    view_only: bool,
}

pub fn run(apple: bool, args: impl Iterator<Item = OsString>) -> Result<i32, Box<dyn Error>> {
    let name = if apple {
        "scrctl --target-type mac"
    } else {
        "scrctl --target-type vnc"
    };
    let args = Args::parse_from(std::iter::once(OsString::from(name)).chain(args));
    if args.request_permission && !apple {
        return Err("--request-permission requires --target-type mac".into());
    }
    #[cfg(not(feature = "sdl3"))]
    if !args.probe {
        return Err("Interactive desktop viewing requires a build with SDL3; use --probe to check the connection".into());
    }
    let mut config = SessionConfig::new(args.host, args.port);
    config.apple_session = apple;
    config.adaptive_dct = apple;
    let connect_timeout = if args.request_permission {
        config.mac_session_choice = Some(MacSessionChoice::Guest);
        println!("Requesting screen sharing permission from the remote Mac…");
        Duration::from_secs(120)
    } else {
        config.username = env::var("SCRCTL_USERNAME").ok();
        config.password = env::var("SCRCTL_PASSWORD").ok();
        Duration::from_secs(15)
    };

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let session = tokio::time::timeout(connect_timeout, Session::connect(config))
            .await
            .map_err(|_| {
                if args.request_permission {
                    "Timed out connecting or waiting for screen sharing approval"
                } else {
                    "Timed out connecting to the remote desktop"
                }
            })??;
        let result = if args.probe {
            probe(&session).await
        } else {
            #[cfg(feature = "sdl3")]
            {
                window::run(&session, args.view_only).await
            }
            #[cfg(not(feature = "sdl3"))]
            unreachable!()
        };
        session.disconnect().await;
        result.map(|()| 0)
    })
}

async fn probe(session: &Session) -> Result<(), Box<dyn Error>> {
    let (width, height) =
        tokio::time::timeout(Duration::from_secs(15), first_frame(session)).await??;
    println!("First frame: {width}x{height}");
    Ok(())
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
