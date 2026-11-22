use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::ffi::OsString;
use std::time::{Duration, Instant};

use crate::diagnostics::Diagnostics;
use clap::{CommandFactory, FromArgMatches, Parser};
use sdl3::audio::{AudioFormat, AudioSpec, AudioStreamOwner};
use sdl3::event::{Event, WindowEvent};
use sdl3::keyboard::{Mod, Scancode};
use sdl3::mouse::MouseButton;
use sdl3::pixels::{Color, PixelFormat};
use sdl3::rect::Rect;
use sdl3::render::FRect;
use sdl3::sys::render::SDL_LOGICAL_PRESENTATION_STRETCH;
use xdb_engine::{AudioReader, Frame, Session, VideoOptions, VideoRecording};

#[derive(Parser)]
#[command(
    name = "scrctl",
    disable_help_flag = true,
    about = "iOS screen mirroring and control"
)]
struct Options {
    /// Show help
    #[arg(short, long)]
    help: bool,
    #[command(flatten)]
    target: crate::device_cli::Target,
    #[command(flatten)]
    commands: crate::device_cli::Commands,
    /// Message language: auto, en, zh-CN
    #[arg(long, value_parser = ["auto", "en", "zh-CN"])]
    lang: Option<String>,
    #[arg(long, help = "Show version")]
    version: bool,
    /// Do not start audio capture
    #[arg(long)]
    no_audio: bool,
    /// Disable computer audio playback; keep encoded audio when recording MP4 or MKV
    #[arg(long)]
    no_audio_playback: bool,
    /// Keep playing audio on the phone as well as the computer
    #[arg(long, conflicts_with = "no_audio")]
    audio_dup: bool,
    /// Audio buffer in milliseconds
    #[arg(long, default_value_t = 50, value_parser = clap::value_parser!(u32).range(..=i64::from(i32::MAX)))]
    audio_buffer: u32,
    #[command(flatten)]
    diagnostics: Diagnostics,
    /// Print input coordinates
    #[arg(long)]
    debug_input: bool,
    /// Enable tunnel diagnostics
    #[arg(long)]
    debug_net: bool,
    /// Launch a bundle ID; ? matches a name prefix, + terminates an existing instance
    #[arg(long, conflicts_with = "no_control")]
    start_app: Option<String>,
    /// Inject a normalized X0,Y0,X1,Y1 swipe and exit
    #[arg(long, conflicts_with = "no_control", num_args = 1..=4, value_delimiter = ',', value_parser = parse_touch_coordinate)]
    test_touch: Vec<f64>,
    /// Inject home, lock, volup, voldn, or mute
    #[arg(long, conflicts_with = "no_control", value_parser = parse_test_button)]
    test_button: Option<TestButton>,
    /// Inject ASCII text into the focused device field
    #[arg(long, conflicts_with = "no_control")]
    test_type: Option<String>,
    /// Disable device input
    #[arg(short = 'n', long)]
    no_control: bool,
    /// Comma-separated shortcut modifiers: lctrl, rctrl, lalt, ralt, lsuper, rsuper
    #[arg(long, default_value = "lalt,lsuper", value_parser = parse_shortcut_mod)]
    shortcut_mod: Mod,
    #[arg(long, conflicts_with = "no_hw_decode")]
    hw_decode: bool,
    /// Use software video decoding
    #[arg(long)]
    no_hw_decode: bool,
    /// Choose live display, its stream alias, or screenshot polling
    #[arg(long, value_parser = ["display", "stream", "screenshot"])]
    video_source: Option<String>,
    /// Alternately force screenshot polling and resume live video at the specified times in seconds
    #[arg(long, value_parser = parse_degrade_marks)]
    test_degrade: Option<DegradeMarks>,
    /// Request a device bitrate ceiling in bit/s, optionally suffixed K or M; 0 uses device defaults
    #[arg(short = 'b', long, value_parser = parse_bit_rate)]
    video_bit_rate: Option<u64>,
    /// Limit decoded receiver output to 0–240 fps; 0 means unlimited
    #[arg(long, value_parser = clap::value_parser!(u16).range(..=240))]
    max_fps: Option<u16>,
    #[command(flatten)]
    window: WindowOptions,
    /// Run without a window
    #[arg(long)]
    no_window: bool,
    /// Start fullscreen
    #[arg(short = 'f', long)]
    fullscreen: bool,
    /// Create a borderless window
    #[arg(long)]
    window_borderless: bool,
    /// Window width; 0 for automatic
    #[arg(long, default_value_t = 0)]
    window_width: u32,
    /// Window height; 0 for automatic
    #[arg(long, default_value_t = 0)]
    window_height: u32,
    /// Finite positive initial scale; defaults to fitting the screen
    #[arg(long, value_parser = parse_scale)]
    scale: Option<f64>,
    /// Crop displayed pixels using WxH+X+Y or W:H:X:Y
    #[arg(long, value_parser = parse_crop)]
    crop: Option<[u32; 4]>,
    /// Display-only orientation: auto, 0/90/180/270 or flip0/flip90/flip180/flip270
    #[arg(long, value_parser = parse_display_orientation)]
    display_orientation: Vec<DisplayOrientation>,
    /// Display and recording orientation; auto resets display only
    #[arg(long, value_parser = parse_display_orientation)]
    orientation: Vec<DisplayOrientation>,
    /// Record MP4, MKV or raw HEVC; format follows the file extension by default
    #[arg(short = 'r', long)]
    record: Option<String>,
    /// Override the recording format
    #[arg(long, requires = "record", value_parser = ["mp4", "mkv", "hevc"])]
    record_format: Option<String>,
    /// Set clockwise MP4/MKV rotation; default is the initial device orientation
    #[arg(long, value_parser = parse_record_orientation)]
    record_orientation: Vec<DisplayOrientation>,
    /// Do not start video capture
    #[arg(long)]
    no_video: bool,
    /// Disable display; keep encoded video when recording
    #[arg(long)]
    no_video_playback: bool,
    /// Disable local video and audio playback while retaining selected recording tracks
    #[arg(short = 'N', long)]
    no_playback: bool,
    /// Run duration in seconds; 0 means unlimited
    #[arg(long, default_value_t = 0)]
    time_limit: u64,
    /// Exit after N frames; 0 means unlimited
    #[arg(long, default_value_t = 0)]
    exit_after: u64,
}

#[derive(clap::Args)]
pub(crate) struct WindowOptions {
    /// Window title
    #[arg(long, visible_alias = "title", default_value = "scrctl")]
    window_title: String,
    /// Window horizontal position; use auto to center
    #[arg(long, value_parser = parse_window_position, allow_negative_numbers = true)]
    window_x: Option<i32>,
    /// Window vertical position; use auto to center
    #[arg(long, value_parser = parse_window_position, allow_negative_numbers = true)]
    window_y: Option<i32>,
    /// Keep the window on top
    #[arg(long)]
    always_on_top: bool,
    /// Select the SDL render driver
    #[arg(long)]
    render_driver: Option<String>,
    /// Prevent local screen sleep
    #[arg(long)]
    disable_screensaver: bool,
    #[arg(long, default_value = "000", value_parser = parse_background)]
    pub(crate) background_color: Color,
}

fn parse_window_position(value: &str) -> Result<i32, &'static str> {
    if value == "auto" {
        Ok(sdl3::sys::video::SDL_WINDOWPOS_CENTERED_MASK as i32)
    } else {
        value.parse().map_err(|_| "Expected an integer or auto")
    }
}

fn parse_background(value: &str) -> Result<Color, &'static str> {
    let value = value.strip_prefix('#').unwrap_or(value);
    let error = "Expected RGB or RRGGBB";
    if !matches!(value.len(), 3 | 6) || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(error);
    }
    let digits = value.len() / 3;
    let channel = |index| {
        let value = u8::from_str_radix(&value[index * digits..(index + 1) * digits], 16)
            .expect("hex digits validated");
        if digits == 1 { value * 17 } else { value }
    };
    Ok(Color::RGB(channel(0), channel(1), channel(2)))
}

impl WindowOptions {
    pub(crate) fn keep_screen_awake(&self, sdl: &sdl3::Sdl) -> Result<(), sdl3::Error> {
        if self.disable_screensaver {
            sdl.video()?.disable_screen_saver()?;
        }
        Ok(())
    }

    pub(crate) fn builder(
        &self,
        video: &sdl3::VideoSubsystem,
        width: u32,
        height: u32,
    ) -> sdl3::video::WindowBuilder {
        use sdl3::video::WindowFlags;
        let center = sdl3::sys::video::SDL_WINDOWPOS_CENTERED_MASK as i32;
        let mut builder = video.window(&self.window_title, width, height);
        builder.resizable().position(
            self.window_x.unwrap_or(center),
            self.window_y.unwrap_or(center),
        );
        if self.always_on_top {
            let flags = builder.flags() | WindowFlags::ALWAYS_ON_TOP;
            builder.set_flags(flags);
        }
        builder
    }

    pub(crate) fn set_icon(window: &mut sdl3::video::Window) {
        const ICON: &[u8] = include_bytes!("../../../../assets/icon/scrctl-icon-1024.png");
        if let Ok(icon) = sdl3::iostream::IOStream::from_bytes(ICON)
            .and_then(|mut stream| sdl3::surface::Surface::load_png_io(&mut stream))
        {
            window.set_icon(icon);
        }
    }

    pub(crate) fn canvas(
        &self,
        window: sdl3::video::Window,
    ) -> Result<sdl3::render::WindowCanvas, Box<dyn Error>> {
        let driver = self
            .render_driver
            .as_deref()
            .filter(|driver| !driver.is_empty())
            .map(std::ffi::CString::new)
            .transpose()?;
        Ok(sdl3::render::create_renderer(window, driver.as_deref())?)
    }
}

#[derive(Clone, Copy, Default)]
pub(crate) struct DisplayOrientation {
    pub(crate) angle: Option<i32>,
    pub(crate) flip: bool,
}

fn parse_display_orientation(value: &str) -> Result<DisplayOrientation, &'static str> {
    if value == "auto" {
        return Ok(DisplayOrientation::default());
    }
    let degrees = value.strip_prefix("flip").unwrap_or(value);
    if !matches!(degrees, "0" | "90" | "180" | "270") {
        return Err("Expected auto or a right angle, optionally prefixed with flip");
    }
    Ok(DisplayOrientation {
        angle: degrees.parse().ok(),
        flip: value != degrees,
    })
}

fn parse_record_orientation(value: &str) -> Result<DisplayOrientation, &'static str> {
    let orientation = parse_display_orientation(value)?;
    if orientation.angle.is_none() {
        return Err("Recording orientation must be a right angle");
    }
    Ok(orientation)
}

#[derive(Default)]
struct Orientations {
    display: DisplayOrientation,
    record: DisplayOrientation,
}

impl Orientations {
    fn resolve(matches: &clap::ArgMatches) -> Self {
        const FLAGS: [&str; 3] = ["orientation", "display_orientation", "record_orientation"];
        let mut ordered: Vec<_> = FLAGS
            .into_iter()
            .flat_map(|flag| {
                matches
                    .indices_of(flag)
                    .into_iter()
                    .flatten()
                    .zip(
                        matches
                            .get_many::<DisplayOrientation>(flag)
                            .into_iter()
                            .flatten(),
                    )
                    .map(move |(index, orientation)| (index, flag, *orientation))
            })
            .collect();
        ordered.sort_unstable_by_key(|(index, _, _)| *index);
        let mut result = Self::default();
        for (_, flag, orientation) in ordered {
            if flag != "record_orientation" {
                result.display = orientation;
            }
            if flag == "record_orientation"
                || (flag == "orientation" && orientation.angle.is_some())
            {
                result.record = orientation;
            }
        }
        result
    }
}

fn parse_crop(value: &str) -> Result<[u32; 4], &'static str> {
    let invalid = "Expected WxH+X+Y or W:H:X:Y";
    let parts = if let Some((width, rest)) = value.split_once('x') {
        let (height, offsets) = rest.split_once('+').ok_or(invalid)?;
        let (x, y) = offsets.split_once('+').ok_or(invalid)?;
        [width, height, x, y]
    } else {
        let fields: Vec<_> = value.split(':').collect();
        fields.try_into().map_err(|_| invalid)?
    };
    let mut crop = [0; 4];
    for (output, part) in crop.iter_mut().zip(parts) {
        *output = part
            .trim_start()
            .parse::<i32>()
            .ok()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or(invalid)?;
    }
    if crop[0] == 0 || crop[1] == 0 {
        return Err(invalid);
    }
    Ok(crop)
}

fn parse_scale(value: &str) -> Result<f64, &'static str> {
    value
        .parse::<f64>()
        .ok()
        .filter(|scale| scale.is_finite() && *scale > 0.0)
        .ok_or("Expected a finite positive number")
}

#[derive(Clone)]
struct DegradeMarks(Vec<u64>);

fn parse_degrade_marks(value: &str) -> Result<DegradeMarks, &'static str> {
    let mut marks = Vec::new();
    for part in value.split(',') {
        let millis = part
            .trim_start()
            .parse::<f64>()
            .map_err(|_| "Invalid fallback time")?
            * 1000.0;
        if !millis.is_finite() || millis < 0.0 || millis >= u64::MAX as f64 {
            return Err("Fallback times must be finite nonnegative seconds");
        }
        let millis = millis as u64;
        if marks.last().is_some_and(|previous| millis < *previous) {
            return Err("Fallback times must be in ascending order");
        }
        marks.push(millis);
    }
    Ok(DegradeMarks(marks))
}

struct FrameSource<'a> {
    sequence: u64,
    video_serial: u64,
    video_unusable: bool,
    started: Instant,
    next_status: Instant,
    next_screenshot: Instant,
    marks: &'a [u64],
}

impl<'a> FrameSource<'a> {
    fn new(marks: &'a [u64]) -> Self {
        let now = Instant::now();
        Self {
            sequence: 0,
            video_serial: 0,
            video_unusable: false,
            started: now,
            next_status: now,
            next_screenshot: now,
            marks,
        }
    }

    fn next(
        &mut self,
        session: &mut Session,
        timeout: u16,
    ) -> Result<Option<Frame>, xdb_engine::Error> {
        let now = Instant::now();
        if now >= self.next_status {
            self.video_unusable = session.media_status()?["video_unusable"] == true;
            self.next_status = now + Duration::from_secs(1);
        }
        let elapsed = self.started.elapsed().as_millis();
        let forced = self.marks.partition_point(|&ms| u128::from(ms) <= elapsed) % 2 == 1;
        let screenshot = session.uses_screenshots() || forced || self.video_unusable;
        let frame = if screenshot {
            if now < self.next_screenshot {
                std::thread::sleep(
                    self.next_screenshot
                        .duration_since(now)
                        .min(Duration::from_millis(u64::from(timeout))),
                );
                return Ok(None);
            }
            let result = session.screenshot();
            self.next_screenshot = Instant::now() + Duration::from_millis(100);
            result
        } else {
            session.next_frame(self.video_serial, timeout)
        };
        let frame = match frame {
            Err(xdb_engine::Error::Server { code, .. })
                if code != "not_connected" && code != "transport_error" =>
            {
                if !screenshot {
                    std::thread::sleep(Duration::from_millis(5));
                }
                None
            }
            result => result?,
        };
        Ok(frame.map(|mut frame| {
            if !screenshot {
                self.video_serial = frame.serial;
            }
            self.sequence += 1;
            frame.serial = self.sequence;
            frame
        }))
    }
}

fn parse_touch_coordinate(value: &str) -> Result<f64, &'static str> {
    value
        .parse::<f64>()
        .ok()
        .filter(|value| (0.0..=1.0).contains(value))
        .ok_or("Coordinates must be finite numbers in [0, 1]")
}

#[derive(Clone)]
struct TestButton {
    name: String,
    usage: u16,
}

fn parse_test_button(name: &str) -> Result<TestButton, &'static str> {
    let usage = match name {
        "home" => 0x40,
        "lock" => 0x30,
        "volup" => 0xe9,
        "voldn" => 0xea,
        "mute" => 0xe2,
        _ => return Err("Unknown button"),
    };
    Ok(TestButton {
        name: name.to_owned(),
        usage,
    })
}

fn parse_bit_rate(value: &str) -> Result<u64, &'static str> {
    let (digits, multiplier) = match value.as_bytes().last() {
        Some(b'K' | b'k') => (&value[..value.len() - 1], 1_000),
        Some(b'M' | b'm') => (&value[..value.len() - 1], 1_000_000),
        _ => (value, 1),
    };
    let error = "Expected a nonnegative integer bit/s value, optionally suffixed K or M";
    if !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(error);
    }
    digits
        .parse::<u64>()
        .ok()
        .and_then(|rate| rate.checked_mul(multiplier))
        .ok_or(error)
}

fn parse_shortcut_mod(text: &str) -> Result<Mod, &'static str> {
    text.split(',').try_fold(Mod::NOMOD, |set, name| {
        Ok(set
            | match name {
                "lctrl" => Mod::LCTRLMOD,
                "rctrl" => Mod::RCTRLMOD,
                "lalt" => Mod::LALTMOD,
                "ralt" => Mod::RALTMOD,
                "lsuper" => Mod::LGUIMOD,
                "rsuper" => Mod::RGUIMOD,
                _ => return Err("Unknown shortcut modifier"),
            })
    })
}

fn modifier_key(scan: Scancode) -> Mod {
    match scan {
        Scancode::LCtrl => Mod::LCTRLMOD,
        Scancode::RCtrl => Mod::RCTRLMOD,
        Scancode::LAlt => Mod::LALTMOD,
        Scancode::RAlt => Mod::RALTMOD,
        Scancode::LGui => Mod::LGUIMOD,
        Scancode::RGui => Mod::RGUIMOD,
        _ => Mod::NOMOD,
    }
}

pub fn run_cli(args: &[OsString]) -> Result<i32, Box<dyn Error>> {
    let mut command = Options::command().after_help(
        "With no arguments, mirror the connected device.
\
         Left click or drag sends touch. Hold Ctrl while dragging to simulate two fingers.
\
         Ordinary keys use the device keyboard layout and input method.
\
         MOD defaults to left Alt or left Super; change it with --shortcut-mod.
\
         Quit: MOD+Q. Fullscreen: MOD+F or F11. Clipboard paste: MOD+V.
\
         Home: MOD+H or middle click. Volume: MOD+Up/Down. Lock: MOD+P.
\
         Actual size: MOD+G. Remove margins: MOD+W or double-click a margin.
\
         Rotate display: MOD+Left/Right. Rotate device: MOD+R.
\
         Flip display: MOD+Shift+Left/Right or MOD+Shift+Up/Down.
\
         Pause or refresh: MOD+Z. Resume: MOD+Shift+Z.
\
         Audio plays on the computer by default; --audio-dup also keeps it on the phone.
\
         Wireless use requires pairing. --help and --version do not connect to the device.",
    );
    let matches = match command
        .clone()
        .try_get_matches_from(std::iter::once(OsString::from("scrctl")).chain(args.iter().cloned()))
    {
        Ok(matches) => matches,
        Err(error) => {
            error.print()?;
            return Ok(2);
        }
    };
    let mut options = Options::from_arg_matches(&matches)?;
    let explicit = |id| matches.value_source(id) == Some(clap::parser::ValueSource::CommandLine);
    let chinese = match options.lang.as_deref() {
        Some("en") => false,
        Some("zh-CN") => true,
        _ => ["LC_ALL", "LC_MESSAGES", "LANG"]
            .into_iter()
            .filter_map(|name| std::env::var(name).ok())
            .find(|locale| !locale.is_empty())
            .is_some_and(|locale| {
                locale
                    .split(['_', '.', '@', '-'])
                    .next()
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case("zh"))
            }),
    };
    if options.help {
        if chinese {
            command = crate::help::localize_chinese(command);
        }
        command.print_long_help()?;
        return Ok(0);
    }
    let invalid = |message: &str| {
        if chinese {
            eprintln!("参数错误：{message}");
        } else {
            eprintln!("{message}");
        }
        Ok(2)
    };
    if !options.test_touch.is_empty() && options.test_touch.len() != 4 {
        return invalid(&format!(
            "--test-touch: At least 4 required but received {}",
            options.test_touch.len()
        ));
    }
    if explicit("wifi") && options.target.wifi.trim().is_empty() {
        return invalid("--wifi: Requires a nonempty LAN address or auto");
    }
    if options.target.wifi == "auto" && explicit("wifi_port") {
        return invalid("--wifi-port: Use a manual LAN address; auto uses discovered SRV ports");
    }
    if let Err(message) = options
        .commands
        .validate(&options.target, explicit("serial"))
    {
        return invalid(message);
    }
    if options.no_playback {
        options.no_video_playback = true;
        options.no_audio_playback = true;
    }
    if options.commands.is_ddi() {
        const DDI_OPTIONS: &[&str] = &[
            "download_ddi",
            "install_ddi",
            "replace_ddi",
            "ddi_install_timeout",
            "ddi_system_version",
            "ddi_product_type",
            "ddi_directory",
            "ddi_download_timeout",
            "serial",
            "wifi",
            "wifi_port",
            "lang",
        ];
        if command.get_arguments().any(|arg| {
            let id = arg.get_id().as_str();
            explicit(id) && !DDI_OPTIONS.contains(&id)
        }) {
            let flag = if explicit("install_ddi") {
                "--install-ddi"
            } else {
                "--download-ddi"
            };
            return invalid(&format!(
                "{flag}: Use only DDI, device selection and language options with this standalone command"
            ));
        }
        return Ok(options
            .commands
            .run(&options.target)
            .expect("device command was selected"));
    }
    if options.commands.pair_requested() && options.start_app.is_some() {
        return invalid("--pair excludes --start-app");
    }
    let orientations = Orientations::resolve(&matches);
    if options.record.as_deref() == Some("") {
        if options.record_format.is_some() {
            return invalid("--record-format: Requires a nonempty recording file");
        }
        options.record = None;
    }
    let container = options
        .record
        .as_deref()
        .and_then(|path| recording_container(path, options.record_format.as_deref()));
    if options.no_audio_playback && container.is_none() {
        options.no_audio = true;
    }
    if options.audio_dup && options.no_audio {
        return invalid(
            "--audio-dup: Audio capture is disabled; --audio-dup requires playback or MP4/MKV recording",
        );
    }

    let screenshots = options.video_source.as_deref() == Some("screenshot");
    if options.record.is_none()
        && (options.no_window || options.no_video_playback)
        && options.exit_after == 0
        && !options.hw_decode
        && !screenshots
        && options.test_degrade.is_none()
    {
        options.no_video = true;
    }
    if (options.max_fps.is_some() || options.video_bit_rate.is_some())
        && (options.no_video || screenshots)
    {
        return invalid("--video-bit-rate/--max-fps: Requires live video stream capture");
    }
    let rotation = orientations.record.angle.unwrap_or_default();
    if options.no_video && options.record.is_some() {
        if container.is_none() {
            return invalid(
                "--record: Audio-only recording requires MP4 or MKV; raw HEVC cannot contain audio",
            );
        }
        if options.no_audio {
            return invalid(
                "--record: Recording requires at least one enabled audio or video track",
            );
        }
        if rotation != 0 || orientations.record.flip {
            return invalid("--record-orientation: Recording orientation requires a video track");
        }
    }
    if options.no_video
        && (options.exit_after > 0
            || options.hw_decode
            || options.test_degrade.is_some()
            || screenshots)
    {
        return invalid(
            "--no-video: Frame counting, hardware decoding, fallback tests and screenshots require video capture",
        );
    }
    if screenshots && container.is_some() {
        return invalid(
            "--record: Container recording requires live video; screenshot polling cannot be recorded",
        );
    }
    if options.record.is_some() {
        if orientations.record.flip {
            return invalid(
                "--record-orientation: Recording does not support flipping; use --display-orientation for display-only flipping or reset --record-orientation to a rotation",
            );
        }
        if container.is_none() && rotation != 0 {
            return invalid(
                "--record: Recording rotation requires MP4 or MKV; use --display-orientation to rotate only the display",
            );
        }
    }
    if options.diagnostics.has_verify() {
        if options.diagnostics.verify_at().is_none() {
            return invalid("--verify: Frame number must be a positive integer");
        }
        if options.no_window {
            return invalid("--verify: Requires a window; incompatible with --no-window");
        }
        if options.no_video || options.no_video_playback {
            return invalid("--verify: Window readback requires video playback");
        }
    }

    let startup_control = options
        .start_app
        .as_ref()
        .is_some_and(|app| !app.is_empty())
        || !options.test_touch.is_empty()
        || options.test_button.is_some()
        || options
            .test_type
            .as_ref()
            .is_some_and(|text| !text.is_empty());
    if !options.version
        && !options.commands.is_requested()
        && options.no_video
        && options.no_audio
        && (options.no_control || (options.no_window && !startup_control))
    {
        return invalid("--no-playback: Nothing to do: enable media, recording or device control");
    }
    if screenshots && options.record.is_some() {
        return invalid("--record: Screenshot polling cannot be recorded");
    }
    if options.version {
        println!("scrctl {}", env!("SCRCTL_PUBLIC_VERSION"));
        return Ok(0);
    }
    if options.commands.is_requested() {
        return Ok(options
            .commands
            .run(&options.target)
            .expect("device command was selected"));
    }
    if options
        .start_app
        .as_ref()
        .is_some_and(|app| !app.is_empty() && app.trim_start_matches(['?', '+']).is_empty())
    {
        return invalid("--start-app: Requires a bundle ID or name prefix");
    }
    run(options, orientations).map(|()| 0)
}

const AUDIO_READ_FRAMES: u32 = 960;
const AUDIO_QUEUE_CHUNKS: i32 = 2;

struct Playback {
    reader: AudioReader,
    output: AudioStreamOwner,
    queue_target_bytes: i32,
    started: bool,
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
            queue_target_bytes: AUDIO_QUEUE_CHUNKS
                * AUDIO_READ_FRAMES as i32
                * i32::from(channels)
                * size_of::<i16>() as i32,
            started: false,
        })
    }

    fn fill(&mut self) -> Result<(), Box<dyn Error>> {
        // One 960-frame IPC chunk can be shorter than a hardware callback.
        // Keep two chunks ready and bound work in each iteration of the UI loop.
        for _ in 0..AUDIO_QUEUE_CHUNKS {
            if self.output.queued_bytes()? >= self.queue_target_bytes {
                break;
            }
            let samples = self.reader.read(AUDIO_READ_FRAMES)?;
            if samples.is_empty() {
                break;
            }
            self.output.put_data_i16(&samples)?;
        }
        if !self.started && self.output.queued_bytes()? >= self.queue_target_bytes {
            self.output.resume()?;
            self.started = true;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct Geometry {
    flip: (bool, bool),
    area: Rect,
    pub(crate) angle: i32,
    panel: (u32, u32),
    pub(crate) source: Rect,
    pixel_angle: Option<i32>,
}

impl Geometry {
    pub(crate) fn transformed(
        frame: &Frame,
        (window_w, window_h): (u32, u32),
        orientation: Option<i32>,
        offset: i32,
        flip: (bool, bool),
        crop: Option<[u32; 4]>,
    ) -> Self {
        let dimension = |key, limit| {
            frame.display[key]
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .filter(|n| *n > 0)
                .unwrap_or(limit)
                .min(limit)
        };
        let fallback = if (frame.width, frame.height) == (1136, 2464)
            && (frame.display["width"].as_u64().unwrap_or(0) == 0
                || frame.display["height"].as_u64().unwrap_or(0) == 0)
        {
            (1125, 2436)
        } else {
            (frame.width, frame.height)
        };
        let device_angle = match frame.display["orientation"].as_str() {
            Some("rot0") => Some(0),
            Some("rot90") => Some(90),
            Some("rot180") => Some(180),
            Some("rot270") => Some(270),
            _ => None,
        };
        let screenshot_angle = if frame.screenshot {
            device_angle.unwrap_or_default()
        } else {
            0
        };
        let swapped = screenshot_angle % 180 != 0;
        let (expected_w, expected_h) = if swapped {
            (fallback.1, fallback.0)
        } else {
            fallback
        };
        let panel = (
            dimension("width", expected_w),
            dimension("height", expected_h),
        );
        let screenshot_valid = (!frame.screenshot || device_angle.is_some())
            && (!frame.screenshot
                || (if swapped { (panel.1, panel.0) } else { panel })
                    == (frame.width, frame.height))
            && (!frame.screenshot
                || frame.display["width"]
                    .as_u64()
                    .is_none_or(|w| w == 0 || w == u64::from(panel.0))
                    && frame.display["height"]
                        .as_u64()
                        .is_none_or(|h| h == 0 || h == u64::from(panel.1)));
        let (w, h) = if frame.screenshot {
            (frame.width, frame.height)
        } else {
            panel
        };
        let [w, h, x, y] = crop.unwrap_or([w, h, 0, 0]);
        let x = x.min(frame.width - 1);
        let y = y.min(frame.height - 1);
        let source = Rect::new(
            x as i32,
            y as i32,
            w.min(frame.width - x),
            h.min(frame.height - y),
        );
        let base_angle = if frame.screenshot {
            0
        } else {
            device_angle.unwrap_or_default()
        };
        let angle = (orientation.unwrap_or(base_angle) + offset).rem_euclid(360);
        let (w, h) = if angle % 180 == 0 {
            (source.width(), source.height())
        } else {
            (source.height(), source.width())
        };
        let scale = (window_w as f64 / w as f64).min(window_h as f64 / h as f64);
        let w = (w as f64 * scale).round().max(1.0) as u32;
        let h = (h as f64 * scale).round().max(1.0) as u32;
        Self {
            area: Rect::from_center((window_w as i32 / 2, window_h as i32 / 2), w, h),
            angle,
            panel,
            source,
            pixel_angle: screenshot_valid.then_some(screenshot_angle),
            flip,
        }
    }

    pub(crate) fn view_size(self) -> (u32, u32) {
        let (w, h) = (self.source.width(), self.source.height());
        if self.angle % 180 == 0 {
            (w, h)
        } else {
            (h, w)
        }
    }

    pub(crate) fn render(
        self,
        canvas: &mut sdl3::render::WindowCanvas,
        texture: &sdl3::render::Texture<'_>,
    ) -> Result<(), Box<dyn Error>> {
        let (w, h) = (self.area.width(), self.area.height());
        let (w, h) = if self.angle % 180 == 0 {
            (w, h)
        } else {
            (h, w)
        };
        let dst = Rect::from_center(self.area.center(), w, h);
        let (w, h) = canvas.window().size();
        canvas.set_logical_size(w, h, SDL_LOGICAL_PRESENTATION_STRETCH)?;
        canvas.copy_ex(
            texture,
            FRect::new(
                self.source.x() as f32,
                self.source.y() as f32,
                self.source.width() as f32,
                self.source.height() as f32,
            ),
            FRect::new(
                dst.x() as f32,
                dst.y() as f32,
                dst.width() as f32,
                dst.height() as f32,
            ),
            f64::from(self.angle),
            None,
            self.flip.0,
            self.flip.1,
        )?;
        Ok(())
    }

    fn point(self, x: i32, y: i32, dragging: bool) -> Option<(f64, f64)> {
        if !dragging && !self.area.contains_point((x, y)) {
            return None;
        }
        let u = (f64::from(x - self.area.x()) / f64::from(self.area.width())).clamp(0.0, 1.0);
        let v = (f64::from(y - self.area.y()) / f64::from(self.area.height())).clamp(0.0, 1.0);
        let (x, y) = match self.angle {
            90 => (v, 1.0 - u),
            180 => (1.0 - u, 1.0 - v),
            270 => (1.0 - v, u),
            _ => (u, v),
        };
        let pixel_angle = self.pixel_angle?;
        let (w, h) = if pixel_angle % 180 == 0 {
            self.panel
        } else {
            (self.panel.1, self.panel.0)
        };
        let (x, y) = (
            ((f64::from(self.source.x())
                + f64::from(self.source.width()) * if self.flip.0 { 1.0 - x } else { x })
                / f64::from(w))
            .clamp(0.0, 1.0),
            ((f64::from(self.source.y())
                + f64::from(self.source.height()) * if self.flip.1 { 1.0 - y } else { y })
                / f64::from(h))
            .clamp(0.0, 1.0),
        );
        Some(match pixel_angle {
            90 => (y, 1.0 - x),
            180 => (1.0 - x, 1.0 - y),
            270 => (1.0 - y, x),
            _ => (x, y),
        })
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Shortcut {
    Quit,
    Fullscreen,
    Rotate(i32),
    Flip(bool),
    RotateDevice,
    Paste,
    Pause(bool),
    Fit(bool),
}

fn shortcut(event: &Event, shortcut_mods: Mod) -> Option<Shortcut> {
    let Event::KeyDown {
        scancode: Some(key),
        keymod,
        repeat: false,
        ..
    } = event
    else {
        return None;
    };
    let shift = keymod.intersects(Mod::LSHIFTMOD | Mod::RSHIFTMOD);
    let shortcut_mod = keymod.intersects(shortcut_mods);
    if *key == Scancode::F11
        && !shift
        && !keymod.intersects(
            Mod::LALTMOD
                | Mod::RALTMOD
                | Mod::LGUIMOD
                | Mod::RGUIMOD
                | Mod::LCTRLMOD
                | Mod::RCTRLMOD,
        )
    {
        return Some(Shortcut::Fullscreen);
    }
    if !shortcut_mod {
        return None;
    }
    match (*key, shift) {
        (Scancode::Q, _) => Some(Shortcut::Quit),
        (Scancode::Z, resume) => Some(Shortcut::Pause(resume)),
        (Scancode::G, false) => Some(Shortcut::Fit(true)),
        (Scancode::W, false) => Some(Shortcut::Fit(false)),
        (Scancode::V, false) => Some(Shortcut::Paste),
        (Scancode::R, false) => Some(Shortcut::RotateDevice),
        (Scancode::F, false) => Some(Shortcut::Fullscreen),
        (Scancode::Left, false) => Some(Shortcut::Rotate(-90)),
        (Scancode::Right, false) => Some(Shortcut::Rotate(90)),
        (Scancode::Left | Scancode::Right, true) => Some(Shortcut::Flip(false)),
        (Scancode::Up | Scancode::Down, true) => Some(Shortcut::Flip(true)),
        _ => None,
    }
}

struct PendingPaste {
    cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
    result: std::sync::mpsc::Receiver<Result<(), xdb_engine::Error>>,
    until: Instant,
}

impl PendingPaste {
    fn cancel(&self) {
        self.cancelled
            .store(true, std::sync::atomic::Ordering::Release);
    }

    fn start(session: &Session, text: String) -> Result<Self, xdb_engine::Error> {
        let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let result = session.prepare_clipboard(text, cancelled.clone())?;
        Ok(Self {
            cancelled,
            result,
            until: Instant::now() + Duration::from_secs(5),
        })
    }

    fn poll(&self, session: &mut Session, valid: bool) -> bool {
        use std::sync::mpsc::TryRecvError;
        if !valid {
            self.cancel();
        }
        if !self.cancelled.load(std::sync::atomic::Ordering::Acquire)
            && Instant::now() >= self.until
        {
            self.cancel();
            eprintln!("Clipboard paste timed out");
        }
        match self.result.try_recv() {
            Ok(result) => {
                if !self.cancelled.load(std::sync::atomic::Ordering::Acquire)
                    && let Err(error) = result.and_then(|()| session.paste_keyboard())
                {
                    eprintln!("Clipboard paste failed: {error}");
                }
                true
            }
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => true,
        }
    }
}

impl Drop for PendingPaste {
    fn drop(&mut self) {
        self.cancel();
    }
}

struct Input {
    shortcut_mods: Mod,
    debug: bool,
    keys: BTreeSet<u16>,
    touch: Option<(f64, f64)>,
    fingers: BTreeMap<(u64, u64), (u8, f64, f64)>,
    buttons: BTreeMap<i32, u16>,
    middle_home: bool,
    virtual_axes: Option<(bool, bool)>,
    virtual_touch: Option<(f64, f64)>,
}

impl Default for Input {
    fn default() -> Self {
        Self {
            shortcut_mods: Mod::LALTMOD | Mod::LGUIMOD,
            debug: false,
            keys: BTreeSet::new(),
            fingers: BTreeMap::new(),
            buttons: BTreeMap::new(),
            touch: None,
            middle_home: false,
            virtual_axes: None,
            virtual_touch: None,
        }
    }
}

impl Input {
    fn can_paste(&self, window: &sdl3::video::Window) -> bool {
        window.has_input_focus()
            && self.keys.is_empty()
            && self.touch.is_none()
            && self.fingers.is_empty()
            && self.buttons.is_empty()
            && !self.middle_home
    }

    fn begin_paste(
        &self,
        window: &sdl3::video::Window,
        session: &Session,
        paste: &mut Option<PendingPaste>,
    ) {
        if paste.is_none()
            && self.can_paste(window)
            && let Ok(text) = window.subsystem().clipboard().clipboard_text()
        {
            match PendingPaste::start(session, text) {
                Ok(pending) => *paste = Some(pending),
                Err(error) => eprintln!("Clipboard paste failed: {error}"),
            }
        }
    }

    fn release(&mut self, session: &mut Session) {
        if let Some((x, y)) = self.touch.take() {
            let _ = session.touch(x, y, false);
        }
        self.virtual_axes = None;
        if let Some((x, y)) = self.virtual_touch.take() {
            let _ = session.touch_contact(1, x, y, false);
        }
        for (_, (contact, x, y)) in std::mem::take(&mut self.fingers) {
            let _ = session.touch_contact(contact, x, y, false);
        }
        for (_, usage) in std::mem::take(&mut self.buttons) {
            let _ = session.button(usage, false);
        }
        if std::mem::take(&mut self.middle_home) {
            let _ = session.button(0x40, false);
        }
        if !self.keys.is_empty() {
            self.keys.clear();
            let _ = session.keyboard(&[]);
        }
    }

    fn send_virtual_touch(
        &mut self,
        session: &mut Session,
        geometry: Geometry,
        x: i32,
        y: i32,
        down: bool,
    ) -> Result<(), xdb_engine::Error> {
        if let Some((invert_x, invert_y)) = self.virtual_axes {
            let center = geometry.area.center();
            let x = if invert_x {
                center.x().saturating_mul(2).saturating_sub(x)
            } else {
                x
            };
            let y = if invert_y {
                center.y().saturating_mul(2).saturating_sub(y)
            } else {
                y
            };
            if let Some((u, v)) = geometry.point(x, y, true) {
                self.virtual_touch = Some((u, v));
                session.touch_contact(1, u, v, down)?;
            }
            if !down {
                self.virtual_axes = None;
                self.virtual_touch = None;
            }
        }
        Ok(())
    }

    fn event(
        &mut self,
        enabled: bool,
        session: &mut Session,
        event: Event,
        geometry: Geometry,
        window_size: (u32, u32),
    ) -> Result<(), xdb_engine::Error> {
        if !enabled {
            return Ok(());
        }
        if self.debug {
            match &event {
                Event::MouseButtonDown { x, y, .. }
                | Event::MouseButtonUp { x, y, .. }
                | Event::MouseMotion { x, y, .. } => {
                    if let Some((u, v)) =
                        geometry.point(x.floor() as i32, y.floor() as i32, self.touch.is_some())
                    {
                        println!("Input: window ({x:.1}, {y:.1}) -> device ({u:.4}, {v:.4})");
                    }
                }
                _ => {}
            }
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
                mouse_btn: MouseButton::Middle,
                ..
            } if !self.middle_home => {
                self.middle_home = true;
                session.button(0x40, true)?;
            }
            Event::MouseButtonUp {
                mouse_btn: MouseButton::Middle,
                ..
            } if self.middle_home => {
                session.button(0x40, false)?;
                self.middle_home = false;
            }
            Event::KeyDown {
                scancode: Some(scan),
                keymod,
                repeat,
                ..
            } if keymod.intersects(self.shortcut_mods)
                && !keymod.intersects(Mod::LSHIFTMOD | Mod::RSHIFTMOD)
                && !self.keys.contains(&(scan as u16)) =>
            {
                let code = scan as i32;
                if repeat {
                    if let Some(&usage @ (0xe9 | 0xea)) = self.buttons.get(&code) {
                        session.button(usage, true)?;
                    }
                } else if let Some(usage) = match scan {
                    Scancode::H => Some(0x40),
                    Scancode::P => Some(0x30),
                    Scancode::Up => Some(0xe9),
                    Scancode::Down => Some(0xea),
                    _ => None,
                } && !self.buttons.contains_key(&code)
                {
                    self.buttons.insert(code, usage);
                    session.button(usage, true)?;
                }
            }
            Event::KeyUp {
                scancode: Some(scan),
                ..
            } if self.buttons.contains_key(&(scan as i32)) => {
                let code = scan as i32;
                session.button(self.buttons[&code], false)?;
                self.buttons.remove(&code);
            }
            Event::FingerDown {
                touch_id,
                finger_id,
                x,
                y,
                ..
            }
            | Event::FingerMotion {
                touch_id,
                finger_id,
                x,
                y,
                ..
            }
            | Event::FingerUp {
                touch_id,
                finger_id,
                x,
                y,
                ..
            }
            | Event::FingerCanceled {
                touch_id,
                finger_id,
                x,
                y,
                ..
            } if session.supports_multi_touch() && self.touch.is_none() => {
                use sdl3::sys::touch::{
                    SDL_GetTouchDeviceType, SDL_TOUCH_DEVICE_DIRECT, SDL_TouchID,
                };
                // Trackpads and other indirect devices are handled by mouse input.
                if unsafe { SDL_GetTouchDeviceType(SDL_TouchID(touch_id)) }
                    != SDL_TOUCH_DEVICE_DIRECT
                {
                    return Ok(());
                }
                let key = (touch_id, finger_id);
                let position = || {
                    geometry.point(
                        (x.clamp(0.0, 1.0) * window_size.0.saturating_sub(1) as f32) as i32,
                        (y.clamp(0.0, 1.0) * window_size.1.saturating_sub(1) as f32) as i32,
                        self.fingers.contains_key(&key),
                    )
                };
                if matches!(event, Event::FingerDown { .. }) {
                    if !self.fingers.contains_key(&key)
                        && x.is_finite()
                        && y.is_finite()
                        && let Some((u, v)) = position()
                        && let Some(contact) =
                            (0..5).find(|id| !self.fingers.values().any(|f| f.0 == *id))
                    {
                        self.fingers.insert(key, (contact, u, v));
                        session.touch_contact(contact, u, v, true)?;
                    }
                } else if let Some(&(contact, last_x, last_y)) = self.fingers.get(&key) {
                    let (u, v) = if x.is_finite() && y.is_finite() {
                        position().unwrap_or((last_x, last_y))
                    } else {
                        (last_x, last_y)
                    };
                    let down = matches!(event, Event::FingerMotion { .. });
                    session.touch_contact(contact, u, v, down)?;
                    if down {
                        self.fingers.insert(key, (contact, u, v));
                    } else {
                        self.fingers.remove(&key);
                    }
                }
            }
            Event::MouseButtonDown {
                mouse_btn: MouseButton::Left,
                which,
                x,
                y,
                ..
            } if self.fingers.is_empty()
                && self.touch.is_none()
                && (!session.supports_multi_touch() || which != u32::MAX) =>
            {
                if let Some((u, v)) = geometry.point(x.floor() as i32, y.floor() as i32, false) {
                    let mods = unsafe { sdl3::sys::keyboard::SDL_GetModState() };
                    let mods = Mod::from_bits_truncate(mods.0);
                    let ctrl = mods.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD);
                    let shift = mods.intersects(Mod::LSHIFTMOD | Mod::RSHIFTMOD);
                    if session.supports_multi_touch() && (ctrl || shift) {
                        self.virtual_axes = Some((ctrl != shift, ctrl));
                    }
                    self.touch = Some((u, v));
                    session.touch(u, v, true)?;
                    self.send_virtual_touch(
                        session,
                        geometry,
                        x.floor() as i32,
                        y.floor() as i32,
                        true,
                    )?;
                }
            }
            Event::MouseMotion { which, x, y, .. }
                if self.touch.is_some()
                    && (!session.supports_multi_touch() || which != u32::MAX) =>
            {
                let (x, y) = (x.floor() as i32, y.floor() as i32);
                if let Some((u, v)) = geometry.point(x, y, true) {
                    self.touch = Some((u, v));
                    session.touch(u, v, true)?;
                    self.send_virtual_touch(session, geometry, x, y, true)?;
                }
            }
            Event::MouseButtonUp {
                mouse_btn: MouseButton::Left,
                which,
                x,
                y,
                ..
            } if self.touch.is_some() && (!session.supports_multi_touch() || which != u32::MAX) => {
                let (x, y) = (x.floor() as i32, y.floor() as i32);
                if let Some((u, v)) = geometry.point(x, y, true) {
                    session.touch(u, v, false)?;
                }
                self.touch = None;
                self.send_virtual_touch(session, geometry, x, y, false)?;
            }
            Event::KeyDown {
                scancode: Some(scan),
                keymod,
                repeat: false,
                ..
            }
            | Event::KeyUp {
                scancode: Some(scan),
                keymod,
                repeat: false,
                ..
            } => {
                if scan == Scancode::F11
                    || self.shortcut_mods.intersects(modifier_key(scan))
                    || (matches!(event, Event::KeyDown { .. })
                        && keymod.intersects(self.shortcut_mods))
                {
                    return Ok(());
                }
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

pub(crate) fn initial_window_size(
    video: &sdl3::VideoSubsystem,
    view: (u32, u32),
    (requested_w, requested_h): (u32, u32),
    scale: Option<f64>,
) -> Result<(u32, u32), &'static str> {
    let (w, h) = (f64::from(view.0), f64::from(view.1));
    let (width, height) = (f64::from(requested_w), f64::from(requested_h));
    let (width, height) = match (requested_w, requested_h) {
        (0, 0) => {
            let scale = scale.unwrap_or_else(|| {
                let bounds = video
                    .get_primary_display()
                    .and_then(|display| display.get_bounds());
                let (usable_w, usable_h) = bounds.map_or((840, 480), |rect| {
                    (rect.width(), rect.height().saturating_sub(60))
                });
                1.0_f64
                    .min(f64::from(usable_w) / w)
                    .min(f64::from(usable_h.max(1)) / h)
            });
            (w * scale, h * scale)
        }
        (0, _) => (height * w / h, height),
        (_, 0) => (width, width * h / w),
        _ => (width, height),
    };
    if width.max(height) > f64::from(i32::MAX) {
        return Err("Window dimensions are too large; reduce --scale or the requested window size");
    }
    Ok((width.max(1.0) as u32, height.max(1.0) as u32))
}

fn run(config: Options, orientations: Orientations) -> Result<(), Box<dyn Error>> {
    let sdl = sdl3::init()?;
    config.window.keep_screen_awake(&sdl)?;
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
                decode_video: !config.no_video
                    && ((!config.no_window && !config.no_video_playback)
                        || config.exit_after > 0
                        || config.hw_decode
                        || config.test_degrade.is_some()),
                orientation: orientations.record.angle.map(|angle| angle as u16),
            })
        })
        .transpose()?;
    let mut session = Session::open_video(
        0,
        &config.target.serial,
        "auto",
        &config.target.wifi,
        config.target.wifi_port,
        VideoOptions {
            hardware_decode: !config.no_hw_decode,
            recording,
            max_bitrate: config.video_bit_rate,
            max_fps: config.max_fps.unwrap_or_default(),
            skip_video: config.no_video,
            screenshots: config.video_source.as_deref() == Some("screenshot"),
            allow_screenshot_fallback: true,
            allow_shared_control: true,
        },
    )?;
    let mut source = FrameSource::new(
        config
            .test_degrade
            .as_ref()
            .map_or(&[], |marks| marks.0.as_slice()),
    );
    let decode_video = !config.no_video && recording.is_none_or(|recording| recording.decode_video);
    let first_frame = if decode_video {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(frame) = source.next(&mut session, 20)? {
                break Some(frame);
            }
            if Instant::now() >= deadline {
                return Err("No first frame from XDB within five seconds".into());
            }
        }
    } else {
        None
    };
    if config.debug_net
        && let Err(error) = session.enable_network_debug()
    {
        eprintln!("{error}");
    }
    if let Some(app) = config.start_app.as_deref().filter(|app| !app.is_empty()) {
        session.launch_app(app)?;
    }
    if let &[x0, y0, x1, y1] = config.test_touch.as_slice() {
        for step in 0..=20 {
            let t = f64::from(step) / 20.0;
            let (x, y) = (x0 + (x1 - x0) * t, y0 + (y1 - y0) * t);
            if let Err(error) = session.touch_contact(0, x, y, step < 20) {
                let _ = session.touch_contact(0, x, y, false);
                return Err(error.into());
            }
            if step < 20 {
                std::thread::sleep(Duration::from_millis(12));
            }
        }
        println!("--test-touch ({x0:.3},{y0:.3})->({x1:.3},{y1:.3}): injected");
        if config.record.is_some() {
            session.finish_media()?;
        }
        return Ok(session.close()?);
    }
    if let Some(button) = &config.test_button {
        session.button(button.usage, true)?;
        std::thread::sleep(Duration::from_millis(90));
        session.button(button.usage, false)?;
        println!("--test-button {}: pressed", button.name);
    }
    if let Some(text) = config.test_type.as_deref().filter(|text| !text.is_empty()) {
        session.type_text(text)?;
        println!("--test-type {text}: injected");
    }
    if config.no_audio_playback && recording.is_some_and(|recording| recording.audio) {
        session.start_recording_audio(config.audio_dup, config.audio_buffer)?;
        println!("Local audio playback disabled; recording original AAC without PCM decoding");
    }
    if config.no_window && config.no_video && config.no_audio && config.record.is_none() {
        return Ok(session.close()?);
    }
    if config.no_window || config.no_video || config.no_video_playback {
        return run_headless(&sdl, events, session, first_frame, source, &config);
    }
    let mut frame = first_frame.expect("windowed session decodes video");
    let video = sdl.video()?;
    let orientation = orientations.display;
    let mut flip = (orientation.flip, false);
    let initial = Geometry::transformed(&frame, (1, 1), orientation.angle, 0, flip, config.crop);
    let (width, height) = initial_window_size(
        &video,
        initial.view_size(),
        (config.window_width, config.window_height),
        config.scale,
    )?;
    let mut builder = config.window.builder(&video, width, height);
    if config.window_borderless {
        builder.borderless();
    }
    let mut window = builder.build()?;
    WindowOptions::set_icon(&mut window);
    if config.fullscreen {
        window.set_fullscreen(true)?;
    }
    let mut canvas = if config.diagnostics.has_verify() {
        sdl3::render::create_renderer(window, Some(c"software"))?
    } else {
        config.window.canvas(window)?
    };
    let creator = canvas.texture_creator();
    let mut texture =
        creator.create_texture_streaming(PixelFormat::BGRA32, frame.width, frame.height)?;
    let mut texture_size = (frame.width, frame.height);
    let mut playback = start_playback(&sdl, &mut session, &config)?;
    let mut input = Input {
        shortcut_mods: config.shortcut_mod,
        debug: config.debug_input,
        ..Input::default()
    };
    let mut geometry = Geometry::transformed(
        &frame,
        canvas.window().size(),
        orientation.angle,
        0,
        flip,
        config.crop,
    );
    let start = Instant::now();
    let mut rotation = 0;
    let mut paused = false;
    let mut refresh = false;
    let mut paste: Option<PendingPaste> = None;
    let mut stats = config.diagnostics.counter();
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
            let new_geometry = Geometry::transformed(
                &frame,
                canvas.window().size(),
                orientation.angle,
                rotation,
                flip,
                config.crop,
            );
            if new_geometry != geometry {
                if let Some(pending) = &paste {
                    pending.cancel();
                }
                input.release(&mut session);
                geometry = new_geometry;
            }
            texture.update(None, &frame.pixels, frame.row_pitch as usize)?;
            canvas.set_draw_color(config.window.background_color);
            canvas.clear();
            geometry.render(&mut canvas, &texture)?;
            if presented_serial != Some(frame.serial) {
                config.diagnostics.save(&canvas, count + 1)?;
            }
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
                stats.report(count, Some(source.sequence));
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
                    if let Some(action) = shortcut(&event, input.shortcut_mods) {
                        if action == Shortcut::Paste {
                            if !config.no_control && !paused {
                                input.begin_paste(canvas.window(), &session, &mut paste);
                            }
                            continue;
                        }
                        if let Some(pending) = &paste {
                            pending.cancel();
                        }
                        input.release(&mut session);
                        match action {
                            Shortcut::Paste => unreachable!(),
                            Shortcut::Quit => break 'running,
                            Shortcut::Fit(actual) => {
                                use sdl3::video::FullscreenType;
                                if canvas.window().fullscreen_state() == FullscreenType::Off {
                                    let size = if actual {
                                        geometry.view_size()
                                    } else {
                                        (geometry.area.width(), geometry.area.height())
                                    };
                                    canvas.window_mut().set_size(size.0, size.1)?;
                                }
                            }
                            Shortcut::Pause(resume) => {
                                paused = !resume;
                                refresh = paused;
                            }
                            Shortcut::RotateDevice => {
                                if !config.no_control {
                                    session.rotate_device()?;
                                }
                            }
                            Shortcut::Fullscreen => {
                                use sdl3::video::FullscreenType;
                                let enter =
                                    canvas.window().fullscreen_state() == FullscreenType::Off;
                                canvas.window_mut().set_fullscreen(enter)?;
                            }
                            Shortcut::Rotate(degrees) => {
                                rotation = (rotation + degrees).rem_euclid(360)
                            }
                            Shortcut::Flip(vertical) => {
                                if vertical ^ (geometry.angle % 180 != 0) {
                                    flip.1 = !flip.1;
                                } else {
                                    flip.0 = !flip.0;
                                }
                            }
                        }
                        continue 'running;
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
                    if (redraw
                        || matches!(
                            event,
                            Event::Window {
                                win_event: WindowEvent::FocusLost,
                                ..
                            } | Event::MouseButtonDown { .. }
                                | Event::FingerDown { .. }
                                | Event::KeyDown { repeat: false, .. }
                        ))
                        && let Some(pending) = &paste
                    {
                        pending.cancel();
                    }
                    input.event(
                        !config.no_control && !paused,
                        &mut session,
                        event,
                        geometry,
                        canvas.window().size(),
                    )?;
                    if redraw {
                        continue 'running;
                    }
                }
                if let Some(pending) = &paste
                    && pending.poll(&mut session, !paused && input.can_paste(canvas.window()))
                {
                    paste = None;
                }
                fill_playback(&mut playback, &mut session, config.record.is_some());
                if let Some(next) = source.next(&mut session, 8)? {
                    if !paused || std::mem::take(&mut refresh) {
                        frame = next;
                        break;
                    }
                }
            }
        }
        Ok(count)
    })();
    drop(paste);
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
    config.diagnostics.finish(count)?;
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
    if config.no_audio || config.no_audio_playback || session.uses_screenshots() {
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
    mut source: FrameSource<'_>,
    config: &Options,
) -> Result<(), Box<dyn Error>> {
    let mut background = if config.no_window {
        None
    } else {
        let video = sdl.video()?;
        let mut builder = config.window.builder(
            &video,
            if config.window_width == 0 {
                256
            } else {
                config.window_width
            },
            if config.window_height == 0 {
                256
            } else {
                config.window_height
            },
        );
        if config.window_borderless {
            builder.borderless();
        }
        let mut window = builder.build()?;
        WindowOptions::set_icon(&mut window);
        if config.fullscreen {
            window.set_fullscreen(true)?;
        }
        let mut canvas = config.window.canvas(window)?;
        canvas.set_draw_color(config.window.background_color);
        canvas.clear();
        canvas.present();
        Some(canvas)
    };
    let mut input = Input {
        shortcut_mods: config.shortcut_mod,
        debug: config.debug_input,
        ..Input::default()
    };
    let mut paste = None;
    let no_picture = Geometry {
        area: Rect::new(0, 0, 1, 1),
        source: Rect::new(0, 0, 1, 1),
        angle: 0,
        panel: (1, 1),
        pixel_angle: None,
        flip: (false, false),
    };
    let mut playback = match start_playback(sdl, &mut session, config) {
        Ok(playback) => playback,
        Err(error) => {
            let _ = session.finish_media();
            return Err(error);
        }
    };
    let mut stats = config.diagnostics.counter();
    let result = (|| -> Result<u64, Box<dyn Error>> {
        let decode_video = first_frame.is_some();
        let mut count = u64::from(decode_video);
        drop(first_frame);
        let start = Instant::now();
        let mut next_status = start;
        loop {
            stats.report(count, Some(source.sequence));
            if reached_time_limit(start, config.time_limit) {
                break;
            }
            if config.exit_after > 0 && count == config.exit_after {
                println!("Reached --exit-after {}", config.exit_after);
                break;
            }
            let mut quit = false;
            for event in events.poll_iter() {
                if matches!(
                    event,
                    Event::Quit { .. }
                        | Event::Window {
                            win_event: WindowEvent::CloseRequested,
                            ..
                        }
                ) {
                    quit = true;
                    break;
                }
                let Some(canvas) = &mut background else {
                    continue;
                };
                if let Some(action) = shortcut(&event, input.shortcut_mods) {
                    if action == Shortcut::Paste {
                        if !config.no_control {
                            input.begin_paste(canvas.window(), &session, &mut paste);
                        }
                        continue;
                    }
                    if let Some(pending) = &paste {
                        pending.cancel();
                    }
                    input.release(&mut session);
                    match action {
                        Shortcut::Quit => {
                            quit = true;
                            break;
                        }
                        Shortcut::RotateDevice if !config.no_control => session.rotate_device()?,
                        Shortcut::Fullscreen => {
                            let enter = canvas.window().fullscreen_state()
                                == sdl3::video::FullscreenType::Off;
                            canvas.window_mut().set_fullscreen(enter)?;
                        }
                        _ => {}
                    }
                    continue;
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
                if redraw {
                    canvas.clear();
                    canvas.present();
                }
                if (redraw
                    || matches!(
                        event,
                        Event::KeyDown { repeat: false, .. }
                            | Event::Window {
                                win_event: WindowEvent::FocusLost,
                                ..
                            }
                    ))
                    && let Some(pending) = &paste
                {
                    pending.cancel();
                }
                // Background windows only forward keyboard and consumer keys.
                if matches!(
                    event,
                    Event::KeyDown { .. } | Event::KeyUp { .. } | Event::Window { .. }
                ) {
                    input.event(
                        !config.no_control,
                        &mut session,
                        event,
                        no_picture,
                        canvas.window().size(),
                    )?;
                }
            }
            if quit {
                break;
            }
            if let Some(pending) = &paste
                && pending.poll(
                    &mut session,
                    background
                        .as_ref()
                        .is_some_and(|canvas| input.can_paste(canvas.window())),
                )
            {
                paste = None;
            }
            fill_playback(&mut playback, &mut session, config.record.is_some());
            if decode_video {
                if source.next(&mut session, 8)?.is_some() {
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
    drop(paste);
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
    if config.no_video && config.record.is_some() {
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
    fn local_shortcuts_respect_modifiers_and_ignore_repeat() {
        let action = |scan, keymod, repeat| {
            shortcut(
                &Event::KeyDown {
                    timestamp: 0,
                    window_id: 1,
                    keycode: None,
                    scancode: Some(scan),
                    keymod,
                    repeat,
                    which: 0,
                    raw: 0,
                },
                Mod::LALTMOD | Mod::LGUIMOD,
            )
        };
        assert_eq!(
            action(Scancode::Q, Mod::LALTMOD, false),
            Some(Shortcut::Quit)
        );
        assert_eq!(action(Scancode::Q, Mod::RALTMOD, false), None);
        assert_eq!(
            action(Scancode::F11, Mod::NOMOD, false),
            Some(Shortcut::Fullscreen)
        );
        assert_eq!(action(Scancode::F11, Mod::LSHIFTMOD, false), None);
        assert_eq!(
            action(Scancode::Left, Mod::LGUIMOD, false),
            Some(Shortcut::Rotate(-90))
        );
        assert_eq!(
            action(Scancode::Up, Mod::LGUIMOD | Mod::LSHIFTMOD, false),
            Some(Shortcut::Flip(true))
        );
        assert_eq!(action(Scancode::Q, Mod::LALTMOD, true), None);
    }

    #[test]
    fn padded_rotated_frame_rejects_bars_and_maps_points_to_panel() {
        let frame = Frame {
            serial: 1,
            width: 12,
            height: 22,
            row_pitch: 48,
            pixels: Vec::new(),
            screenshot: false,
            display: serde_json::json!({"width": 10, "height": 20, "orientation": "rot90"}),
        };
        let geometry = Geometry::transformed(&frame, (400, 400), None, 0, (false, false), None);
        assert_eq!(geometry.area, Rect::new(0, 100, 400, 200));
        assert_eq!(geometry.point(20, 20, false), None);
        assert_eq!(geometry.point(100, 250, false), Some((0.75, 0.75)));
        assert_eq!(geometry.point(500, 50, true), Some((0.0, 0.0)));
        let cropped = Geometry::transformed(
            &frame,
            (400, 400),
            Some(0),
            0,
            (true, false),
            Some([4, 8, 2, 4]),
        );
        assert_eq!(cropped.view_size(), (4, 8));
        assert_eq!(cropped.source, Rect::new(2, 4, 4, 8));
        assert_eq!(cropped.point(100, 0, false), Some((0.6, 0.2)));
        assert_eq!(cropped.point(200, 200, false), Some((0.4, 0.4)));
        let mut screenshot = Frame {
            width: 20,
            height: 10,
            screenshot: true,
            ..frame
        };
        let geometry =
            Geometry::transformed(&screenshot, (400, 400), None, 0, (false, false), None);
        assert_eq!(geometry.angle, 0);
        assert_eq!(geometry.point(100, 250, false), Some((0.75, 0.75)));
        screenshot.display["width"] = 11.into();
        let inconsistent =
            Geometry::transformed(&screenshot, (400, 400), None, 0, (false, false), None);
        assert_eq!(inconsistent.point(100, 250, false), None);
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
            source: Rect::new(0, 0, 100, 200),
            pixel_angle: Some(0),
            flip: (false, false),
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
            input
                .event(false, &mut session, event, geometry, (400, 400))
                .unwrap();
        }
        assert!(input.touch.is_none() && input.keys.is_empty());
        for event in batch.iter().cloned() {
            input
                .event(true, &mut session, event, geometry, (400, 400))
                .unwrap();
        }
        for event in batch.into_iter().take(2) {
            input
                .event(true, &mut session, event, geometry, (400, 400))
                .unwrap();
        }
        input.release(&mut session);
        let volume_down = |repeat| Event::KeyDown {
            timestamp: 0,
            window_id: 1,
            keycode: None,
            scancode: Some(Scancode::Up),
            keymod: Mod::LALTMOD,
            repeat,
            which: 0,
            raw: 0,
        };
        let volume_up = Event::KeyUp {
            timestamp: 0,
            window_id: 1,
            keycode: None,
            scancode: Some(Scancode::Up),
            keymod: Mod::NOMOD,
            repeat: false,
            which: 0,
            raw: 0,
        };
        for event in [
            volume_down(false),
            volume_down(true),
            volume_up,
            volume_down(false),
        ] {
            input
                .event(true, &mut session, event, geometry, (400, 400))
                .unwrap();
        }
        input.virtual_axes = Some((true, true));
        input
            .send_virtual_touch(&mut session, geometry, 70, 65, true)
            .unwrap();
        input.release(&mut session);
        session.rotate_device().unwrap();
        session.close().unwrap();
    }
    #[test]
    #[ignore = "requires the XDB loopback fixture"]
    fn clipboard_readback_and_stale_paste() {
        let port = std::env::var("SCRCTL_INPUT_FIXTURE_PORT")
            .unwrap()
            .parse()
            .unwrap();
        let mut session = Session::open(port, "mock-iphone", "wifi", false).unwrap();
        for (text, valid) in [("scrctl 夹具", true), ("stale", false)] {
            let pending = PendingPaste::start(&session, text.into()).unwrap();
            let deadline = Instant::now() + Duration::from_secs(3);
            // The invalid case must never send paste keystrokes, even if
            // synchronization runs before the first poll.
            while Instant::now() < deadline {
                if pending.poll(&mut session, valid) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            assert!(Instant::now() < deadline);
        }
        session.close().unwrap();
    }
}
