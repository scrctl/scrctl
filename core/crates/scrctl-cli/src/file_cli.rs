use std::{
    error::Error,
    ffi::OsString,
    path::PathBuf,
    time::{Duration, Instant},
};

use clap::Parser;
use ffmpeg_next as av;
use sdl3::{
    event::{Event, WindowEvent},
    pixels::PixelFormat,
    render::FRect,
};

#[derive(Parser)]
#[command(disable_help_flag = true)]
struct Options {
    #[arg(long)]
    play: PathBuf,
    #[arg(long)]
    no_window: bool,
    #[arg(long, conflicts_with = "no_hw_decode")]
    hw_decode: bool,
    #[arg(long)]
    no_hw_decode: bool,
    #[command(flatten)]
    window: crate::sdl_frontend::WindowOptions,
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
    let mut canvas = if options.no_window {
        None
    } else {
        Some(options.window.open(&sdl.video()?, 840, 540)?)
    };
    let creator = canvas.as_ref().map(|canvas| canvas.texture_creator());
    let mut texture = None;
    let mut scaler = None;
    let mut source_format = None;
    let start = Instant::now();
    let mut count = 0;
    let mut eof = false;
    let mut source_ended = false;

    'playback: loop {
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
        if let (Some(canvas), Some(creator)) = (&mut canvas, &creator) {
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

            let (out_w, out_h) = canvas.output_size()?;
            let scale = (out_w as f32 / width as f32).min(out_h as f32 / height as f32);
            let (w, h) = (width as f32 * scale, height as f32 * scale);
            canvas.set_draw_color(options.window.background_color);
            canvas.clear();
            canvas.copy(
                texture,
                None,
                FRect::new((out_w as f32 - w) / 2.0, (out_h as f32 - h) / 2.0, w, h),
            )?;
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
