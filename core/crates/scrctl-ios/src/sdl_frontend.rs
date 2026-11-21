use std::collections::BTreeSet;
use std::error::Error;
use std::ffi::OsString;
use std::time::{Duration, Instant};

use clap::Parser;
use scrctl_ios::{AudioReader, Frame, Session, VideoRecording};
use sdl3::audio::{AudioFormat, AudioSpec, AudioStreamOwner};
use sdl3::event::{Event, WindowEvent};
use sdl3::mouse::MouseButton;
use sdl3::pixels::{Color, PixelFormat};
use sdl3::rect::Rect;
use sdl3::render::FRect;
use sdl3::sys::render::SDL_LOGICAL_PRESENTATION_STRETCH;

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
    #[arg(long)]
    no_window: bool,
    #[arg(short = 'r', long)]
    record: Option<String>,
    #[arg(long, requires = "record", value_parser = ["mp4", "mkv", "hevc"])]
    record_format: Option<String>,
    #[arg(long, value_parser = ["0", "90", "180", "270"])]
    record_orientation: Option<String>,
    #[arg(long)]
    no_video: bool,
    #[arg(long, default_value_t = 0)]
    time_limit: u64,
    #[arg(long, default_value_t = 0)]
    exit_after: u64,
}

pub fn run_if_supported(args: &[OsString]) -> Option<Result<(), Box<dyn Error>>> {
    let mode = std::env::var_os("SCRCTL_RUST_SDL");
    if mode.as_deref() == Some(std::ffi::OsStr::new("0")) {
        return None;
    }
    let options = Options::try_parse_from(
        std::iter::once(OsString::from("scrctl")).chain(args.iter().cloned()),
    )
    .ok()?;
    if let Some(path) = options.record.as_deref() {
        let container = recording_container(path, options.record_format.as_deref()).is_some();
        let rotation = options.record_orientation.as_deref().unwrap_or("0");
        if path.is_empty()
            || (options.no_video && (!container || options.no_audio || options.exit_after != 0))
            || ((!container || options.no_video) && rotation != "0")
            || (!container && options.no_audio_playback && options.audio_dup)
        {
            return None;
        }
    } else if options.no_video
        || options.record_orientation.is_some()
        || (options.exit_after == 0 && mode.as_deref() != Some(std::ffi::OsStr::new("1")))
    {
        return None;
    }
    Some(run(options))
}

struct Playback {
    reader: AudioReader,
    output: AudioStreamOwner,
    chunk_bytes: i32,
}

impl Playback {
    fn open(audio: &sdl3::AudioSubsystem, reader: AudioReader) -> Result<Self, sdl3::Error> {
        let (rate, channels) = reader.format();
        let spec = AudioSpec {
            freq: Some(rate),
            channels: Some(i32::from(channels)),
            format: Some(AudioFormat::s16_sys()),
        };
        let output = audio
            .default_playback_device()
            .open_device_stream(Some(&spec))?;
        Ok(Self {
            reader,
            output,
            chunk_bytes: 960 * i32::from(channels) * 2,
        })
    }

    fn fill(&mut self) -> Result<(), Box<dyn Error>> {
        if self.output.queued_bytes()? < self.chunk_bytes {
            let samples = self.reader.read(960)?;
            if !samples.is_empty() {
                self.output.put_data_i16(&samples)?;
                self.output.resume()?;
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
        enabled: bool,
        session: &mut Session,
        event: Event,
        geometry: Geometry,
    ) -> Result<(), scrctl_ios::Error> {
        if !enabled {
            return Ok(());
        }
        match event {
            Event::Window {
                win_event:
                    WindowEvent::FocusLost
                    | WindowEvent::Resized(_, _)
                    | WindowEvent::PixelSizeChanged(_, _),
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
                if let Some((u, v)) =
                    geometry.point(x.floor() as i32, y.floor() as i32, self.touch.is_some())
                {
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
                let (u, v) = geometry
                    .point(x.floor() as i32, y.floor() as i32, true)
                    .expect("active pointer");
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

fn recording_container(path: &str, explicit: Option<&str>) -> Option<&'static str> {
    match explicit {
        Some("mp4") => Some("mp4"),
        Some("mkv") => Some("mkv"),
        Some(_) => None,
        None if path.to_ascii_lowercase().ends_with(".mp4") => Some("mp4"),
        None if path.to_ascii_lowercase().ends_with(".mkv") => Some("mkv"),
        None => None,
    }
}

fn run(config: Options) -> Result<(), Box<dyn Error>> {
    let sdl = sdl3::init()?;
    let mut events = sdl.event_pump()?;
    let absolute_recording = config
        .record
        .as_deref()
        .map(std::path::absolute)
        .transpose()?;
    let recording = absolute_recording
        .as_ref()
        .map(|path| {
            let path = path.to_str().ok_or("Recording path is not UTF-8")?;
            let container = recording_container(path, config.record_format.as_deref());
            Ok::<_, &str>(VideoRecording {
                path,
                container,
                audio: container.is_some() && !config.no_audio,
                video: !config.no_video,
                decode_video: !config.no_video && (!config.no_window || config.exit_after > 0),
                orientation: config
                    .record_orientation
                    .as_deref()
                    .and_then(|s| s.parse().ok()),
            })
        })
        .transpose()?;
    let mut session = Session::open_video(
        0,
        &config.serial,
        "auto",
        &config.wifi,
        config.wifi_port,
        !config.no_hw_decode,
        recording,
    )?;
    let decode_video = recording.is_none_or(|recording| recording.decode_video);
    let first_frame = if decode_video {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(frame) = session.next_frame(0, 20)? {
                break Some(frame);
            }
            if Instant::now() >= deadline {
                return Err("No first frame from XDB within five seconds".into());
            }
        }
    } else {
        None
    };
    if config.no_audio_playback && recording.is_some_and(|recording| recording.audio) {
        session.start_recording_audio(config.audio_dup, config.audio_buffer)?;
        println!("Local audio playback disabled; recording original AAC without PCM decoding");
    }
    if config.no_window || config.no_video {
        return run_headless(&sdl, events, session, first_frame, &config);
    }
    let mut frame = first_frame.expect("windowed session decodes video");
    let video = sdl.video()?;
    let mut canvas = video
        .window(&config.window_title, 420, 840)
        .resizable()
        .position_centered()
        .build()?
        .into_canvas();
    let creator = canvas.texture_creator();
    let mut texture =
        creator.create_texture_streaming(PixelFormat::BGRA32, frame.width, frame.height)?;
    let mut texture_size = (frame.width, frame.height);
    let mut playback = start_playback(&sdl, &mut session, &config)?;
    let mut input = Input::default();
    let mut geometry = Geometry::new(&frame, canvas.window().size());
    let start = Instant::now();
    let result = (|| -> Result<u64, Box<dyn Error>> {
        let mut count = 0;
        let mut presented_serial = None;
        'running: loop {
            if texture_size != (frame.width, frame.height) {
                texture = creator.create_texture_streaming(
                    PixelFormat::BGRA32,
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
            canvas.set_logical_size(
                canvas.window().size().0,
                canvas.window().size().1,
                SDL_LOGICAL_PRESENTATION_STRETCH,
            )?;
            canvas.set_draw_color(Color::BLACK);
            canvas.clear();
            canvas.copy_ex(
                &texture,
                FRect::new(0.0, 0.0, geometry.panel.0 as f32, geometry.panel.1 as f32),
                FRect::new(
                    dst.x() as f32,
                    dst.y() as f32,
                    dst.width() as f32,
                    dst.height() as f32,
                ),
                f64::from(geometry.angle),
                None,
                false,
                false,
            )?;
            canvas.present();
            if presented_serial != Some(frame.serial) {
                count += 1;
                presented_serial = Some(frame.serial);
                if count == config.exit_after {
                    println!("Reached --exit-after {}", config.exit_after);
                    break;
                }
            }
            loop {
                if reached_time_limit(start, config.time_limit) {
                    break 'running;
                }
                for event in events.poll_iter() {
                    if matches!(
                        event,
                        Event::Quit { .. }
                            | Event::Window {
                                win_event: WindowEvent::CloseRequested,
                                ..
                            }
                    ) {
                        break 'running;
                    }
                    let redraw = matches!(
                        event,
                        Event::Window {
                            win_event: WindowEvent::Resized(_, _)
                                | WindowEvent::PixelSizeChanged(_, _)
                                | WindowEvent::Exposed,
                            ..
                        }
                    );
                    input.event(!config.no_control, &mut session, event, geometry)?;
                    if redraw {
                        continue 'running;
                    }
                }
                fill_playback(&mut playback, &mut session, config.record.is_some());
                if let Some(next) = session.next_frame(frame.serial, 8)? {
                    frame = next;
                    break;
                }
            }
        }
        Ok(count)
    })();
    input.release(&mut session);
    drop(playback);
    let finish = if config.record.is_some() {
        session.finish_media()
    } else {
        Ok(())
    };
    let close = session.close();
    let count = result?;
    finish?;
    close?;
    println!("Finished: processed {count} frames");
    Ok(())
}

fn reached_time_limit(start: Instant, seconds: u64) -> bool {
    if seconds > 0 && start.elapsed() >= Duration::from_secs(seconds) {
        println!("Reached --time-limit {seconds} seconds");
        true
    } else {
        false
    }
}

fn start_playback(
    sdl: &sdl3::Sdl,
    session: &mut Session,
    config: &Options,
) -> Result<Option<Playback>, Box<dyn Error>> {
    if config.no_audio || config.no_audio_playback {
        return Ok(None);
    }
    let recording = config.record.is_some();
    let audio = match sdl.audio() {
        Ok(audio) => audio,
        Err(error) if recording => return Err(error.into()),
        Err(error) => {
            eprintln!("Audio unavailable: {error}");
            return Ok(None);
        }
    };
    let reader = match session.start_audio(config.audio_dup, config.audio_buffer) {
        Ok(reader) => reader,
        Err(error) if recording => return Err(error.into()),
        Err(error) => {
            eprintln!("Audio unavailable: {error}");
            return Ok(None);
        }
    };
    match Playback::open(&audio, reader) {
        Ok(playback) => Ok(Some(playback)),
        Err(error) if recording => {
            eprintln!(
                "Failed to open audio output: {error} (recording and audio reception continue)"
            );
            Ok(None)
        }
        Err(error) => {
            eprintln!("Audio unavailable: {error}");
            let _ = session.stop_audio();
            Ok(None)
        }
    }
}

fn fill_playback(playback: &mut Option<Playback>, session: &mut Session, recording: bool) {
    if let Some(audio) = playback
        && let Err(error) = audio.fill()
    {
        eprintln!("Audio unavailable: {error}");
        *playback = None;
        if !recording {
            let _ = session.stop_audio();
        }
    }
}

fn run_headless(
    sdl: &sdl3::Sdl,
    mut events: sdl3::EventPump,
    mut session: Session,
    first_frame: Option<Frame>,
    config: &Options,
) -> Result<(), Box<dyn Error>> {
    let mut background = if config.no_window {
        None
    } else {
        let mut canvas = sdl
            .video()?
            .window(&config.window_title, 640, 480)
            .resizable()
            .position_centered()
            .build()?
            .into_canvas();
        canvas.set_draw_color(Color::BLACK);
        canvas.clear();
        canvas.present();
        Some(canvas)
    };
    let mut playback = match start_playback(sdl, &mut session, config) {
        Ok(playback) => playback,
        Err(error) => {
            let _ = session.finish_media();
            return Err(error);
        }
    };
    let result = (|| -> Result<u64, scrctl_ios::Error> {
        let decode_video = first_frame.is_some();
        let mut count = u64::from(decode_video);
        let mut serial = first_frame.as_ref().map_or(0, |frame| frame.serial);
        drop(first_frame);
        let start = Instant::now();
        let mut next_status = start;
        loop {
            if reached_time_limit(start, config.time_limit) {
                break;
            }
            if config.exit_after > 0 && count == config.exit_after {
                println!("Reached --exit-after {}", config.exit_after);
                break;
            }
            let mut quit = false;
            for event in events.poll_iter() {
                match event {
                    Event::Quit { .. }
                    | Event::Window {
                        win_event: WindowEvent::CloseRequested,
                        ..
                    } => quit = true,
                    Event::Window {
                        win_event: WindowEvent::Resized(_, _) | WindowEvent::Exposed,
                        ..
                    } => {
                        if let Some(canvas) = &mut background {
                            canvas.clear();
                            canvas.present();
                        }
                    }
                    _ => {}
                }
            }
            if quit {
                break;
            }
            fill_playback(&mut playback, &mut session, config.record.is_some());
            if decode_video {
                if let Some(frame) = session.next_frame(serial, 8)? {
                    serial = frame.serial;
                    count += 1;
                }
            } else {
                if Instant::now() >= next_status {
                    session.check_media()?;
                    next_status = Instant::now() + Duration::from_secs(1);
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        Ok(count)
    })();
    drop(playback);
    let finish = if config.record.is_some() {
        session.finish_media()
    } else {
        Ok(())
    };
    let close = session.close();
    let count = result?;
    finish?;
    close?;
    if config.no_video {
        println!("Finished: audio recording finalized");
    } else if count == 0 && config.record.is_some() {
        println!("Finished: encoded video recording finalized");
    } else {
        println!("Finished: processed {count} frames");
    }
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
        use sdl3::keyboard::{Mod, Scancode};
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
                x: 70.0,
                y: 90.0,
            },
            Event::KeyDown {
                timestamp: 0,
                window_id: 1,
                keycode: None,
                scancode: Some(Scancode::A),
                keymod: Mod::NOMOD,
                repeat: false,
                which: 0,
                raw: 0,
            },
            Event::Window {
                timestamp: 0,
                window_id: 1,
                win_event: WindowEvent::FocusLost,
            },
        ];
        for event in batch.iter().cloned() {
            input.event(false, &mut session, event, geometry).unwrap();
        }
        assert!(input.touch.is_none() && input.keys.is_empty());
        for event in batch.iter().cloned() {
            input.event(true, &mut session, event, geometry).unwrap();
        }
        for event in batch.into_iter().take(2) {
            input.event(true, &mut session, event, geometry).unwrap();
        }
        input.release(&mut session);
        session.close().unwrap();
    }
}
