use std::collections::BTreeSet;
use std::error::Error;
use std::ffi::OsString;
use std::time::{Duration, Instant};

use clap::Parser;
use scrctl_ios::{AudioReader, Frame, Session};
use sdl2::audio::{AudioQueue, AudioSpecDesired};
use sdl2::event::{Event, WindowEvent};
use sdl2::mouse::MouseButton;
use sdl2::pixels::{Color, PixelFormatEnum};
use sdl2::rect::Rect;

#[derive(Parser)]
struct Options {
    #[arg(short = 's', long, default_value = "")]
    serial: String,
    #[arg(long, default_value = "")]
    wifi: String,
    #[arg(long, requires = "wifi", default_value_t = 49152, value_parser = clap::value_parser!(u16).range(1..))]
    wifi_port: u16,
    #[arg(long)]
    no_audio: bool,
    #[arg(long)]
    no_audio_playback: bool,
    #[arg(long, conflicts_with = "no_audio")]
    audio_dup: bool,
    #[arg(long, default_value_t = 50, value_parser = clap::value_parser!(u32).range(..=1000))]
    audio_buffer: u32,
    #[arg(short = 'n', long)]
    no_control: bool,
    #[arg(long)]
    no_hw_decode: bool,
    #[arg(long, visible_alias = "title", default_value = "scrctl")]
    window_title: String,
    #[arg(long, default_value_t = 0)]
    exit_after: u64,
}

pub fn run_if_supported(args: &[OsString]) -> Option<Result<(), Box<dyn Error>>> {
    // Keep the incumbent frontend as the default until interaction parity is complete.
    if std::env::var_os("SCRCTL_RUST_SDL").as_deref() != Some(std::ffi::OsStr::new("1")) {
        return None;
    }
    let options = Options::try_parse_from(
        std::iter::once(OsString::from("scrctl")).chain(args.iter().cloned()),
    )
    .ok()?;
    Some(run(options))
}

struct Playback {
    reader: AudioReader,
    output: AudioQueue<i16>,
}

impl Playback {
    fn start(
        sdl: &sdl2::Sdl,
        session: &mut Session,
        config: &Options,
    ) -> Result<Self, Box<dyn Error>> {
        let audio = sdl.audio()?;
        let reader = session.start_audio(config.audio_dup, config.audio_buffer)?;
        let (rate, channels) = reader.format();
        let spec = AudioSpecDesired {
            freq: Some(rate),
            channels: Some(channels),
            samples: Some(512),
        };
        let output = match audio.open_queue(None, &spec) {
            Ok(output) if output.spec().freq == rate && output.spec().channels == channels => {
                output
            }
            _ => {
                drop(reader);
                session.stop_audio()?;
                return Err(
                    "could not open the default SDL audio output with the XDB format".into(),
                );
            }
        };
        Ok(Self { reader, output })
    }

    fn fill(&mut self) -> Result<(), Box<dyn Error>> {
        if self.output.size() < 960 * u32::from(self.output.spec().channels) * 2 {
            let samples = self.reader.read(960)?;
            if !samples.is_empty() {
                self.output.queue_audio(&samples)?;
                self.output.resume();
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, PartialEq)]
struct Geometry {
    area: Rect,
    angle: i32,
    panel: (u32, u32),
}

impl Geometry {
    fn new(frame: &Frame, (window_w, window_h): (u32, u32)) -> Self {
        let dimension = |key, limit| {
            frame.display[key]
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .filter(|n| *n > 0)
                .unwrap_or(limit)
                .min(limit)
        };
        let panel = (
            dimension("width", frame.width),
            dimension("height", frame.height),
        );
        let angle = match frame.display["orientation"].as_str() {
            Some("rot90") => 90,
            Some("rot180") => 180,
            Some("rot270") => 270,
            _ => 0,
        };
        let (w, h) = if angle % 180 == 0 {
            panel
        } else {
            (panel.1, panel.0)
        };
        let scale = (window_w as f64 / w as f64).min(window_h as f64 / h as f64);
        let w = (w as f64 * scale).round().max(1.0) as u32;
        let h = (h as f64 * scale).round().max(1.0) as u32;
        Self {
            area: Rect::from_center((window_w as i32 / 2, window_h as i32 / 2), w, h),
            angle,
            panel,
        }
    }

    fn point(self, x: i32, y: i32, dragging: bool) -> Option<(f64, f64)> {
        if !dragging && !self.area.contains_point((x, y)) {
            return None;
        }
        let u = (f64::from(x - self.area.x()) / f64::from(self.area.width())).clamp(0.0, 1.0);
        let v = (f64::from(y - self.area.y()) / f64::from(self.area.height())).clamp(0.0, 1.0);
        Some(match self.angle {
            90 => (v, 1.0 - u),
            180 => (1.0 - u, 1.0 - v),
            270 => (1.0 - v, u),
            _ => (u, v),
        })
    }
}

#[derive(Default)]
struct Input {
    keys: BTreeSet<u16>,
    touch: Option<(f64, f64)>,
}

impl Input {
    fn release(&mut self, session: &mut Session) {
        if let Some((x, y)) = self.touch.take() {
            let _ = session.touch(x, y, false);
        }
        if !self.keys.is_empty() {
            self.keys.clear();
            let _ = session.keyboard(&[]);
        }
    }

    fn event(
        &mut self,
        session: &mut Session,
        event: Event,
        geometry: Geometry,
    ) -> Result<(), scrctl_ios::Error> {
        match event {
            Event::Window {
                win_event: WindowEvent::FocusLost | WindowEvent::SizeChanged(_, _),
                ..
            } => self.release(session),
            Event::MouseButtonDown {
                mouse_btn: MouseButton::Left,
                x,
                y,
                ..
            }
            | Event::MouseMotion { x, y, .. }
                if matches!(event, Event::MouseButtonDown { .. }) || self.touch.is_some() =>
            {
                if let Some((u, v)) = geometry.point(x, y, self.touch.is_some()) {
                    self.touch = Some((u, v));
                    session.touch(u, v, true)?;
                }
            }
            Event::MouseButtonUp {
                mouse_btn: MouseButton::Left,
                x,
                y,
                ..
            } if self.touch.is_some() => {
                let (u, v) = geometry.point(x, y, true).expect("active pointer");
                session.touch(u, v, false)?;
                self.touch = None;
            }
            Event::KeyDown {
                scancode: Some(scan),
                repeat: false,
                ..
            }
            | Event::KeyUp {
                scancode: Some(scan),
                repeat: false,
                ..
            } => {
                let usage = scan as i32;
                // SDL's standard keyboard scancodes follow USB HID usage IDs.
                if matches!(usage, 4..=115 | 224..=231) {
                    let changed = if matches!(event, Event::KeyDown { .. }) {
                        self.keys.insert(usage as u16)
                    } else {
                        self.keys.remove(&(usage as u16))
                    };
                    if changed {
                        session.keyboard(&self.keys.iter().copied().collect::<Vec<_>>())?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}

fn run(config: Options) -> Result<(), Box<dyn Error>> {
    let sdl = sdl2::init()?;
    let video = sdl.video()?;
    let mut events = sdl.event_pump()?;
    let mut session = Session::open_target(
        0,
        &config.serial,
        "auto",
        &config.wifi,
        config.wifi_port,
        !config.no_hw_decode,
    )?;
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut frame = loop {
        if let Some(frame) = session.next_frame(0, 20)? {
            break frame;
        }
        if Instant::now() >= deadline {
            return Err("No first frame from XDB within five seconds".into());
        }
    };
    let mut canvas = video
        .window(&config.window_title, 420, 840)
        .resizable()
        .position_centered()
        .build()?
        .into_canvas()
        .build()?;
    let creator = canvas.texture_creator();
    let mut texture =
        creator.create_texture_streaming(PixelFormatEnum::BGRA32, frame.width, frame.height)?;
    let mut texture_size = (frame.width, frame.height);
    let mut playback = if config.no_audio || config.no_audio_playback {
        None
    } else {
        Playback::start(&sdl, &mut session, &config)
            .map_err(|error| eprintln!("Audio unavailable: {error}"))
            .ok()
    };
    let mut input = Input::default();
    let mut geometry = Geometry::new(&frame, canvas.window().size());
    let result = (|| -> Result<(), Box<dyn Error>> {
        let mut count = 0;
        'running: loop {
            if texture_size != (frame.width, frame.height) {
                texture = creator.create_texture_streaming(
                    PixelFormatEnum::BGRA32,
                    frame.width,
                    frame.height,
                )?;
                texture_size = (frame.width, frame.height);
            }
            let new_geometry = Geometry::new(&frame, canvas.window().size());
            if new_geometry != geometry {
                input.release(&mut session);
                geometry = new_geometry;
            }
            texture.update(None, &frame.pixels, frame.row_pitch as usize)?;
            let (w, h) = (geometry.area.width(), geometry.area.height());
            let (w, h) = if geometry.angle % 180 == 0 {
                (w, h)
            } else {
                (h, w)
            };
            let dst = Rect::from_center(geometry.area.center(), w, h);
            canvas.set_logical_size(canvas.window().size().0, canvas.window().size().1)?;
            canvas.set_draw_color(Color::BLACK);
            canvas.clear();
            canvas.copy_ex(
                &texture,
                Rect::new(0, 0, geometry.panel.0, geometry.panel.1),
                dst,
                f64::from(geometry.angle),
                None,
                false,
                false,
            )?;
            canvas.present();
            count += 1;
            if count == config.exit_after {
                break;
            }
            loop {
                for event in events.poll_iter() {
                    if matches!(
                        event,
                        Event::Quit { .. }
                            | Event::Window {
                                win_event: WindowEvent::Close,
                                ..
                            }
                    ) {
                        break 'running;
                    }
                    if !config.no_control {
                        input.event(&mut session, event, geometry)?;
                    }
                }
                if let Some(audio) = &mut playback
                    && let Err(error) = audio.fill()
                {
                    eprintln!("Audio unavailable: {error}");
                    playback = None;
                    let _ = session.stop_audio();
                }
                if let Some(next) = session.next_frame(frame.serial, 8)? {
                    frame = next;
                    break;
                }
            }
        }
        println!("Finished: processed {count} frames");
        Ok(())
    })();
    input.release(&mut session);
    drop(playback);
    let close = session.close();
    result?;
    close?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn padded_rotated_frame_rejects_bars_and_maps_points_to_panel() {
        let frame = Frame {
            serial: 1,
            width: 12,
            height: 22,
            row_pitch: 48,
            pixels: Vec::new(),
            display: serde_json::json!({"width": 10, "height": 20, "orientation": "rot90"}),
        };
        let geometry = Geometry::new(&frame, (400, 400));
        assert_eq!(geometry.area, Rect::new(0, 100, 400, 200));
        assert_eq!(geometry.point(20, 20, false), None);
        assert_eq!(geometry.point(100, 250, false), Some((0.75, 0.75)));
        assert_eq!(geometry.point(500, 50, true), Some((0.0, 0.0)));
    }

    #[test]
    #[ignore = "requires the XDB loopback fixture"]
    fn forwards_sdl_input_and_releases_on_focus_loss() {
        use sdl2::keyboard::{Mod, Scancode};
        let port = std::env::var("SCRCTL_INPUT_FIXTURE_PORT")
            .unwrap()
            .parse()
            .unwrap();
        let mut session = Session::open(port, "mock-iphone", "wifi", false).unwrap();
        let mut input = Input::default();
        let geometry = Geometry {
            area: Rect::new(20, 40, 200, 100),
            angle: 90,
            panel: (100, 200),
        };
        let batch = [
            Event::MouseButtonDown {
                timestamp: 0,
                window_id: 1,
                which: 0,
                mouse_btn: MouseButton::Left,
                clicks: 1,
                x: 70,
                y: 90,
            },
            Event::KeyDown {
                timestamp: 0,
                window_id: 1,
                keycode: None,
                scancode: Some(Scancode::A),
                keymod: Mod::NOMOD,
                repeat: false,
            },
            Event::Window {
                timestamp: 0,
                window_id: 1,
                win_event: WindowEvent::FocusLost,
            },
        ];
        for event in batch {
            input.event(&mut session, event, geometry).unwrap();
        }
        session.close().unwrap();
    }
}
