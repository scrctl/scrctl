use std::{
    error::Error,
    ffi::OsString,
    path::Path,
    time::{Duration, Instant},
};

use sdl3::render::WindowCanvas;

#[derive(clap::Args)]
pub(crate) struct Diagnostics {
    /// Print statistics every second
    #[arg(long)]
    stats: bool,
    /// Read window at frame N into a BMP
    #[arg(long, num_args = 2, value_names = ["FRAME", "FILE"])]
    verify: Vec<OsString>,
}

impl Diagnostics {
    pub(crate) fn verify_at(&self) -> Option<u64> {
        self.verify
            .first()?
            .to_str()?
            .parse()
            .ok()
            .filter(|n| *n > 0)
    }

    pub(crate) fn has_verify(&self) -> bool {
        !self.verify.is_empty()
    }

    pub(crate) fn save(&self, canvas: &WindowCanvas, frame: u64) -> Result<(), Box<dyn Error>> {
        if self.verify_at() == Some(frame) {
            let path = Path::new(&self.verify[1]);
            let surface = canvas.read_pixels(None)?;
            surface.save_bmp(path)?;
            println!(
                "Window readback saved to {} ({}x{} pixels)",
                path.display(),
                surface.width(),
                surface.height()
            );
        }
        Ok(())
    }

    pub(crate) fn finish(&self, frames: u64) -> Result<(), Box<dyn Error>> {
        if let Some(requested) = self.verify_at().filter(|requested| frames < *requested) {
            return Err(format!(
                "Window readback was not reached (requested frame {requested}, rendered {frames})"
            )
            .into());
        }
        Ok(())
    }

    pub(crate) fn counter(&self) -> Statistics {
        let start = Instant::now();
        Statistics {
            enabled: self.stats,
            start,
            last_report: start,
            last_count: 0,
        }
    }
}

pub(crate) struct Statistics {
    enabled: bool,
    start: Instant,
    last_report: Instant,
    last_count: u64,
}

impl Statistics {
    pub(crate) fn report(&mut self, frames: u64, received: Option<u64>) {
        let now = Instant::now();
        let interval = now.duration_since(self.last_report);
        if self.enabled && interval >= Duration::from_secs(1) {
            let delta = frames - self.last_count;
            let rate = delta as f64 / interval.as_secs_f64();
            let average = frames as f64 / now.duration_since(self.start).as_secs_f64();
            println!(
                "  Render: {frames} frames total, {delta} this interval / {rate:.1} fps, average {average:.1} fps"
            );
            if let Some(received) = received {
                println!("xdb frames={received} delivered={frames}");
            }
            self.last_report = now;
            self.last_count = frames;
        }
    }
}
