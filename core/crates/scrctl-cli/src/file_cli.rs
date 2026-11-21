use std::{
    error::Error,
    ffi::OsString,
    path::PathBuf,
    time::{Duration, Instant},
};

use crate::sdl_frontend::{
    DisplayOrientation, Geometry, initial_window_size, parse_crop, parse_display_orientation,
    parse_scale,
};
use clap::Parser;
use ffmpeg_next as av;
use sdl3::{
    event::{Event, WindowEvent},
    pixels::PixelFormat,
};

#[derive(Parser)]
#[command(disable_help_flag = true)]
struct Options {
    #[arg(long)]
    play: PathBuf,
    #[arg(long)]
    no_window: bool,
    #[command(flatten)]
    diagnostics: crate::diagnostics::Diagnostics,
    #[arg(long, conflicts_with = "no_hw_decode")]
    hw_decode: bool,
    #[arg(long)]
    no_hw_decode: bool,
    #[command(flatten)]
    window: crate::sdl_frontend::WindowOptions,
    #[arg(short = 'f', long)]
    fullscreen: bool,
    #[arg(long)]
    window_borderless: bool,
    #[arg(long, default_value_t = 0)]
    window_width: u32,
    #[arg(long, default_value_t = 0)]
    window_height: u32,
    #[arg(long, value_parser = parse_scale)]
    scale: Option<f64>,
    #[arg(long, value_parser = parse_crop)]
    crop: Option<[u32; 4]>,
    #[arg(long, visible_alias = "orientation", value_parser = parse_display_orientation)]
    display_orientation: Option<DisplayOrientation>,
    #[arg(long, default_value_t = 0)]
    time_limit: u64,
    #[arg(long, default_value_t = 0)]
    exit_after: u64,
}

pub fn run_if_supported(args: &[OsString]) -> Option<i32> {
    let options = Options::try_parse_from(
        std::iter::once(OsString::from("scrctl")).chain(args.iter().cloned()),
    )
    .ok()?;
    if options.play.as_os_str().is_empty() || options.play.to_str().is_none() {
        return None;
    }
    if options.diagnostics.has_verify()
        && (options.no_window || options.diagnostics.verify_at().is_none())
    {
        return None;
    }
    Some(match run(options) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("{error}");
            1
        }
    })
}

fn run(options: Options) -> Result<(), Box<dyn Error>> {
    let path = &options.play;
    match path.metadata() {
        Ok(meta) if meta.is_file() => {}
        Ok(_) => {
            return Err(format!("Playback requires a regular file: {}", path.display()).into());
        }
        Err(_) => return Err(format!("Cannot open file {}", path.display()).into()),
    }
    av::init()?;
    av::util::log::set_level(av::util::log::Level::Quiet);
    println!("Playing file: {}", path.display());
    let hardware = cfg!(target_os = "macos") && options.hw_decode;
    println!(
        "Decoder backend: {}",
        if hardware {
            "VideoToolbox"
        } else {
            "libavcodec/hevc"
        }
    );

    // Keep the local playback contract restricted to Annex-B HEVC.
    let parameter_error = "Playback file has no complete HEVC parameter sets";
    let mut input = av::format::input(path).map_err(|_| parameter_error)?;
    if input.format().name() != "hevc" {
        return Err(parameter_error.into());
    }
    let stream = input
        .streams()
        .best(av::media::Type::Video)
        .ok_or(parameter_error)?;
    let stream_index = stream.index();
    let mut context = av::codec::context::Context::from_parameters(stream.parameters())?;
    #[cfg(target_os = "macos")]
    if hardware {
        enable_videotoolbox(&mut context)?;
    }
    let mut decoder = context.decoder().video()?;

    let sdl = sdl3::init()?;
    options.window.keep_screen_awake(&sdl)?;
    let mut events = sdl.event_pump()?;
    let video = if options.no_window {
        None
    } else {
        Some(sdl.video()?)
    };
    let orientation = options.display_orientation.unwrap_or_default();
    // Delay window creation until the first frame supplies the coded size.
    let mut canvas = None;
    let creator = std::cell::OnceCell::new();
    let mut texture = None;
    let mut scaler = None;
    let mut source_format = None;
    let start = Instant::now();
    let mut count = 0;
    let mut stats = options.diagnostics.counter();
    let mut eof = false;
    let mut source_ended = false;

    'playback: loop {
        stats.report(count, None);
        if options.time_limit > 0 && start.elapsed() >= Duration::from_secs(options.time_limit) {
            println!("Reached --time-limit {} seconds", options.time_limit);
            break;
        }
        if events.poll_iter().any(|event| {
            matches!(
                event,
                Event::Quit { .. }
                    | Event::Window {
                        win_event: WindowEvent::CloseRequested,
                        ..
                    }
            )
        }) {
            break;
        }
        let mut frame = av::frame::Video::empty();
        match decoder.receive_frame(&mut frame) {
            Ok(()) => {}
            Err(av::Error::Eof) => {
                source_ended = true;
                break;
            }
            Err(av::Error::Other { errno }) if errno == av::error::EAGAIN && !eof => {
                let mut packet = av::Packet::empty();
                match packet.read(&mut input) {
                    Ok(()) if packet.stream() == stream_index => decoder.send_packet(&packet)?,
                    Ok(()) => {}
                    Err(av::Error::Eof) => {
                        decoder.send_eof()?;
                        eof = true;
                    }
                    Err(_) => {
                        return Err(format!("Cannot read playback file: {}", path.display()).into());
                    }
                }
                continue;
            }
            Err(error) => return Err(error.into()),
        }

        if hardware && frame.format() != av::format::Pixel::VIDEOTOOLBOX {
            return Err("VideoToolbox did not return a hardware frame".into());
        }
        let geometry_frame = xdb_engine::Frame {
            serial: count + 1,
            width: frame.width(),
            height: frame.height(),
            row_pitch: 0,
            display: serde_json::Value::Null,
            screenshot: false,
            pixels: Vec::new(),
        };
        if let Some(video) = &video
            && canvas.is_none()
        {
            let layout = Geometry::transformed(
                &geometry_frame,
                (1, 1),
                orientation.angle,
                0,
                (orientation.flip, false),
                options.crop,
            );
            let (width, height) = initial_window_size(
                video,
                layout.view_size(),
                (options.window_width, options.window_height),
                options.scale,
            )?;
            let mut builder = options.window.builder(video, width, height);
            if options.window_borderless {
                builder.borderless();
            }
            let mut window = builder.build()?;
            if options.fullscreen {
                window.set_fullscreen(true)?;
            }
            let window_canvas = if options.diagnostics.has_verify() {
                sdl3::render::create_renderer(window, Some(c"software"))?
            } else {
                options.window.canvas(window)?
            };
            let _ = creator.set(window_canvas.texture_creator());
            canvas = Some(window_canvas);
        }
        if let (Some(canvas), Some(creator)) = (&mut canvas, creator.get()) {
            if hardware {
                let mut software = av::frame::Video::empty();
                // Transfer the hardware surface while the input frame is alive.
                let status = unsafe {
                    av::ffi::av_hwframe_transfer_data(software.as_mut_ptr(), frame.as_ptr(), 0)
                };
                if status < 0 {
                    return Err(av::Error::from(status).into());
                }
                frame = software;
            }
            let (width, height) = (frame.width(), frame.height());
            let format = (frame.format(), width, height);
            if source_format != Some(format) {
                scaler = Some(av::software::scaling::Context::get(
                    format.0,
                    width,
                    height,
                    av::format::Pixel::BGRA,
                    width,
                    height,
                    av::software::scaling::Flags::BILINEAR,
                )?);
                texture =
                    Some(creator.create_texture_streaming(PixelFormat::BGRA32, width, height)?);
                source_format = Some(format);
            }
            let mut bgra = av::frame::Video::empty();
            scaler.as_mut().unwrap().run(&frame, &mut bgra)?;
            let texture = texture.as_mut().unwrap();
            texture.update(None, bgra.data(0), bgra.stride(0))?;

            let layout = Geometry::transformed(
                &geometry_frame,
                canvas.window().size(),
                orientation.angle,
                0,
                (orientation.flip, false),
                options.crop,
            );
            canvas.set_draw_color(options.window.background_color);
            canvas.clear();
            layout.render(canvas, texture)?;
            options.diagnostics.save(canvas, count + 1)?;
            canvas.present();
        }
        count += 1;
        if !options.no_window {
            let target = Duration::from_secs_f64(count as f64 / 60.0);
            std::thread::sleep(target.saturating_sub(start.elapsed()));
        }
        if count == options.exit_after {
            println!("Reached --exit-after {}", options.exit_after);
            break 'playback;
        }
    }
    if count == 0 {
        return Err("Playback file produced no video frames".into());
    }
    options.diagnostics.finish(count)?;
    if source_ended {
        println!("Source ended");
    }
    println!("Finished: processed {count} frames");
    Ok(())
}

#[cfg(target_os = "macos")]
fn enable_videotoolbox(context: &mut av::codec::context::Context) -> Result<(), av::Error> {
    use av::ffi::{AVCodecContext, AVHWDeviceType, AVPixelFormat};
    use std::ptr;

    unsafe extern "C" fn select_format(
        _: *mut AVCodecContext,
        mut formats: *const AVPixelFormat,
    ) -> AVPixelFormat {
        // Libavcodec provides a list terminated by AV_PIX_FMT_NONE.
        while unsafe { *formats } != AVPixelFormat::AV_PIX_FMT_NONE {
            if unsafe { *formats } == AVPixelFormat::AV_PIX_FMT_VIDEOTOOLBOX {
                return AVPixelFormat::AV_PIX_FMT_VIDEOTOOLBOX;
            }
            formats = unsafe { formats.add(1) };
        }
        AVPixelFormat::AV_PIX_FMT_NONE
    }

    // The unopened context owns this AVBufferRef and releases it on drop.
    let status = unsafe {
        let raw = context.as_mut_ptr();
        (*raw).get_format = Some(select_format);
        av::ffi::av_hwdevice_ctx_create(
            &mut (*raw).hw_device_ctx,
            AVHWDeviceType::AV_HWDEVICE_TYPE_VIDEOTOOLBOX,
            ptr::null(),
            ptr::null_mut(),
            0,
        )
    };
    if status < 0 {
        Err(av::Error::from(status))
    } else {
        Ok(())
    }
}
