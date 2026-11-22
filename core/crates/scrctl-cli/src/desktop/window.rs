use std::collections::HashMap;
use std::error::Error;
use std::io;
use std::time::Duration;

use rfb_engine::session::{Event, Rect, Session, SessionError};
use sdl3::event::{Event as SdlEvent, WindowEvent};
use sdl3::keyboard::{Keycode, Mod, Scancode};
use sdl3::mouse::{MouseButton, MouseWheelDirection};
use sdl3::pixels::{Color, PixelFormat};
use sdl3::render::FRect;
use tokio::time::{self, MissedTickBehavior};

use crate::sdl_frontend::WindowOptions;

type Size = (u16, u16);

pub(super) async fn run(session: &Session, view_only: bool) -> Result<(), Box<dyn Error>> {
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let mut events = sdl.event_pump()?;
    let mut size = time::timeout(Duration::from_secs(15), super::first_frame(session)).await??;
    let initial = Viewport::fit((1200, 800), size);
    let title = session.name().replace('\0', "");
    let mut window = video
        .window(
            &title,
            initial.width.ceil() as u32,
            initial.height.ceil() as u32,
        )
        .resizable()
        .high_pixel_density()
        .build()?;
    WindowOptions::set_icon(&mut window);
    let mut canvas = window.into_canvas();
    let creator = canvas.texture_creator();
    let mut texture = None;
    let mut texture_size = (0, 0);
    let mut dirty = true;
    let mut redraw = true;
    let mut input = Input {
        enabled: !view_only,
        focused: canvas.window().has_input_focus(),
        ..Input::default()
    };
    let mut tick = time::interval(Duration::from_millis(16));
    tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let interrupt = tokio::signal::ctrl_c();
    tokio::pin!(interrupt);

    let result: Result<(), Box<dyn Error>> = async {
        loop {
            tokio::select! {
                signal = &mut interrupt => return signal.map_err(Into::into),
                event = session.next_event() => match event {
                    Some(Event::Frame { .. } | Event::Resized { .. } | Event::DesktopLayout { .. }) => dirty = true,
                    Some(Event::Disconnected { reason, .. }) => return Err(io::Error::other(reason).into()),
                    None => return Err(io::Error::other("Desktop session ended").into()),
                    _ => {}
                },
                _ = tick.tick() => {
                    if dirty {
                        size = session.size().ok_or_else(|| io::Error::other("missing framebuffer size"))?;
                        let (width, height) = size;
                        if size != texture_size {
                            texture = Some(creator.create_texture_streaming(
                                PixelFormat::BGRA32, width.into(), height.into(),
                            )?);
                            texture_size = size;
                        }
                        let pixels = session.copy_region(Rect { x: 0, y: 0, width, height })
                            .ok_or_else(|| io::Error::other("missing framebuffer pixels"))?;
                        texture.as_mut().expect("texture exists").update(None, &pixels, usize::from(width) * 4)?;
                        dirty = false;
                        redraw = true;
                    }
                    for event in events.poll_iter() {
                        if matches!(event, SdlEvent::Quit { .. } | SdlEvent::Window {
                            win_event: WindowEvent::CloseRequested, ..
                        }) {
                            return Ok(());
                        }
                        if matches!(event, SdlEvent::Window { win_event:
                            WindowEvent::Resized(..) | WindowEvent::PixelSizeChanged(..)
                            | WindowEvent::Exposed | WindowEvent::Restored, .. }) {
                            redraw = true;
                        }
                        let viewport = Viewport::fit(canvas.window().size(), size);
                        time::timeout(Duration::from_secs(1), input.event(session, event, viewport)).await??;
                    }
                    if redraw {
                        let (width, height) = canvas.output_size()?;
                        let viewport = Viewport::fit((width, height), size);
                        canvas.set_draw_color(Color::BLACK);
                        canvas.clear();
                        if let Some(texture) = &texture {
                            canvas.copy(texture, None, FRect::new(viewport.x, viewport.y, viewport.width, viewport.height))?;
                        }
                        canvas.present();
                        redraw = false;
                    }
                }
            }
        }
    }.await;
    let cleanup = time::timeout(Duration::from_secs(1), input.release(session)).await;
    result?;
    cleanup??;
    Ok(())
}

#[derive(Clone, Copy)]
struct Viewport {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    remote: Size,
}

impl Viewport {
    fn fit((width, height): (u32, u32), remote: Size) -> Self {
        let (rw, rh) = (f32::from(remote.0), f32::from(remote.1));
        let (window_w, window_h) = (width as f32, height as f32);
        let scale = (window_w / rw).min(window_h / rh);
        let (width, height) = (rw * scale, rh * scale);
        Self {
            x: (window_w - width) / 2.0,
            y: (window_h - height) / 2.0,
            width,
            height,
            remote,
        }
    }

    fn position(self, x: f32, y: f32, clamp: bool) -> Option<(u16, u16)> {
        let (x, y) = ((x - self.x) / self.width, (y - self.y) / self.height);
        if !x.is_finite()
            || !y.is_finite()
            || (!clamp && !(0.0..1.0).contains(&x))
            || (!clamp && !(0.0..1.0).contains(&y))
        {
            return None;
        }
        let axis = |position: f32, extent: u16| {
            ((position.clamp(0.0, 1.0) * f32::from(extent)) as u16).min(extent.saturating_sub(1))
        };
        Some((axis(x, self.remote.0), axis(y, self.remote.1)))
    }
}

#[derive(Default)]
struct Input {
    enabled: bool,
    focused: bool,
    buttons: u8,
    position: (u16, u16),
    keys: HashMap<Scancode, u32>,
}

impl Input {
    async fn event(
        &mut self,
        session: &Session,
        event: SdlEvent,
        viewport: Viewport,
    ) -> Result<(), SessionError> {
        if !self.enabled {
            return Ok(());
        }
        match event {
            SdlEvent::Window {
                win_event: WindowEvent::FocusGained,
                ..
            } => self.focused = true,
            SdlEvent::Window {
                win_event: WindowEvent::FocusLost | WindowEvent::Hidden | WindowEvent::Minimized,
                ..
            } => {
                self.focused = false;
                self.release(session).await?;
            }
            _ if !self.focused => {}
            SdlEvent::KeyDown {
                keycode,
                scancode: Some(scan),
                keymod,
                ..
            } => {
                if let Some(value) = event_keysym(scan, keycode, keymod) {
                    let value = self.keys.get(&scan).copied().unwrap_or(value);
                    session.send_key(true, value).await?;
                    self.keys.insert(scan, value);
                }
            }
            SdlEvent::KeyUp {
                scancode: Some(scan),
                ..
            } => {
                if let Some(&value) = self.keys.get(&scan) {
                    session.send_key(false, value).await?;
                    self.keys.remove(&scan);
                }
            }
            SdlEvent::MouseMotion { x, y, .. } => {
                if let Some(position) = viewport.position(x, y, self.buttons != 0) {
                    self.send_pointer(session, self.buttons, position).await?;
                }
            }
            SdlEvent::MouseButtonDown {
                mouse_btn, x, y, ..
            } => {
                let button = button_mask(mouse_btn);
                if button != 0
                    && let Some(position) = viewport.position(x, y, false)
                {
                    self.send_pointer(session, self.buttons | button, position)
                        .await?;
                }
            }
            SdlEvent::MouseButtonUp {
                mouse_btn, x, y, ..
            } => {
                let button = button_mask(mouse_btn);
                if self.buttons & button != 0 {
                    let position = viewport.position(x, y, true).unwrap_or(self.position);
                    self.send_pointer(session, self.buttons & !button, position)
                        .await?;
                }
            }
            SdlEvent::MouseWheel {
                integer_x,
                integer_y,
                direction,
                mouse_x,
                mouse_y,
                ..
            } => {
                if let Some((x, y)) = viewport.position(mouse_x, mouse_y, self.buttons != 0) {
                    let sign = if direction == MouseWheelDirection::Flipped {
                        -1
                    } else {
                        1
                    };
                    session
                        .send_scroll_with_buttons(
                            self.buttons,
                            x,
                            y,
                            integer_x.clamp(-32, 32) * sign,
                            integer_y.clamp(-32, 32) * -sign,
                        )
                        .await?;
                    self.position = (x, y);
                }
            }
            _ => {}
        }
        Ok(())
    }

    async fn send_pointer(
        &mut self,
        session: &Session,
        mask: u8,
        position: (u16, u16),
    ) -> Result<(), SessionError> {
        session.send_pointer(mask, position.0, position.1).await?;
        self.position = position;
        self.buttons = mask;
        Ok(())
    }

    async fn release(&mut self, session: &Session) -> Result<(), SessionError> {
        if self.buttons != 0 {
            let position = session.size().map_or(self.position, |(width, height)| {
                (
                    self.position.0.min(width.saturating_sub(1)),
                    self.position.1.min(height.saturating_sub(1)),
                )
            });
            self.send_pointer(session, 0, position).await?;
        }
        while let Some((&scan, &key)) = self.keys.iter().next() {
            session.send_key(false, key).await?;
            self.keys.remove(&scan);
        }
        Ok(())
    }
}

fn button_mask(button: MouseButton) -> u8 {
    match button {
        MouseButton::Left => 1,
        MouseButton::Middle => 2,
        MouseButton::Right => 4,
        _ => 0,
    }
}

fn event_keysym(scan: Scancode, key: Option<Keycode>, modifiers: Mod) -> Option<u32> {
    // SDL's safe keycode enum covers named keys, but not every Unicode
    // key value a layout can produce. This call runs on the SDL thread.
    let translated = unsafe {
        sdl3::sys::keyboard::SDL_GetKeyFromScancode(
            sdl3::sys::scancode::SDL_Scancode(scan as i32),
            sdl3::sys::keycode::SDL_Keymod(modifiers.bits()),
            false,
        )
    }
    .0;
    printable_keysym(translated).or_else(|| key.and_then(keysym))
}

fn printable_keysym(raw: u32) -> Option<u32> {
    match raw {
        0x20..=0x7e | 0xa0..=0xff => Some(raw),
        0x100..=0xd7ff | 0xe000..=0x10ffff => Some(0x0100_0000 | raw),
        _ => None,
    }
}

fn keysym(key: Keycode) -> Option<u32> {
    use Keycode::*;
    let raw = key as u32;
    Some(match key {
        Backspace => 0xff08,
        Tab => 0xff09,
        Return => 0xff0d,
        Pause => 0xff13,
        ScrollLock => 0xff14,
        Escape => 0xff1b,
        Home => 0xff50,
        Left => 0xff51,
        Up => 0xff52,
        Right => 0xff53,
        Down => 0xff54,
        PageUp => 0xff55,
        PageDown => 0xff56,
        End => 0xff57,
        PrintScreen => 0xff61,
        Insert => 0xff63,
        Menu | Application => 0xff67,
        NumLockClear => 0xff7f,
        KpEnter => 0xff8d,
        KpMultiply => 0xffaa,
        KpPlus => 0xffab,
        KpMinus => 0xffad,
        KpPeriod => 0xffae,
        KpDivide => 0xffaf,
        Kp0 => 0xffb0,
        _ if (Kp1 as u32..=Kp9 as u32).contains(&raw) => 0xffb1 + raw - Kp1 as u32,
        KpEquals => 0xffbd,
        _ if (F1 as u32..=F12 as u32).contains(&raw) => 0xffbe + raw - F1 as u32,
        _ if (F13 as u32..=F24 as u32).contains(&raw) => 0xffca + raw - F13 as u32,
        LShift => 0xffe1,
        RShift => 0xffe2,
        LCtrl => 0xffe3,
        RCtrl => 0xffe4,
        CapsLock => 0xffe5,
        LAlt => 0xffe9,
        RAlt => 0xffea,
        LGui => 0xffeb,
        RGui => 0xffec,
        Delete => 0xffff,
        _ => return printable_keysym(raw),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt, duplex};

    #[test]
    fn viewport_preserves_aspect_and_clamps_drag_outside_image() {
        let viewport = Viewport::fit((800, 800), (1600, 800));
        assert_eq!(viewport.position(400.0, 400.0, false), Some((800, 400)));
        assert_eq!(viewport.position(400.0, 100.0, false), None);
        assert_eq!(viewport.position(800.0, 700.0, true), Some((1599, 799)));
        let resized = Viewport::fit((800, 800), (800, 1600));
        assert_eq!(resized.position(100.0, 400.0, false), None);
        assert_eq!(resized.position(400.0, 400.0, false), Some((400, 800)));
    }

    #[test]
    fn maps_rfb_keysyms() {
        assert_eq!(keysym(Keycode::A), Some(0x61));
        assert_eq!(keysym(Keycode::RCtrl), Some(0xffe4));
        assert_eq!(keysym(Keycode::Kp9), Some(0xffb9));
        assert_eq!(keysym(Keycode::F12), Some(0xffc9));
        assert_eq!(keysym(Keycode::F13), Some(0xffca));
        assert_eq!(printable_keysym(0x4f60), Some(0x0100_4f60));
    }

    #[tokio::test]
    async fn forwards_input_and_releases_on_focus_loss_or_close() {
        for (view_only, focus_loss) in [(false, true), (false, false), (true, false)] {
            let (client, mut peer) = duplex(4096);
            let server = tokio::spawn(async move {
                peer.write_all(b"RFB 003.008\n").await.unwrap();
                let mut version = [0; 12];
                peer.read_exact(&mut version).await.unwrap();
                assert_eq!(&version, b"RFB 003.008\n");
                peer.write_all(&[1, 1]).await.unwrap();
                assert_eq!(peer.read_u8().await.unwrap(), 1);
                peer.write_u32(0).await.unwrap();
                assert_eq!(peer.read_u8().await.unwrap(), 1);
                peer.write_all(&[
                    0, 160, 0, 80, // extent
                    32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 16, 8, 0, 0, 0, 0, // pixel format
                    0, 0, 0, 4, b'T', b'e', b's', b't',
                ])
                .await
                .unwrap();
                let mut setup = [0; 24];
                peer.read_exact(&mut setup).await.unwrap();
                assert_eq!(setup[20], 2);
                let encodings = u16::from_be_bytes([setup[22], setup[23]]);
                peer.read_exact(&mut vec![0; usize::from(encodings) * 4 + 10])
                    .await
                    .unwrap();
                let mut input = Vec::new();
                peer.read_to_end(&mut input).await.unwrap();
                input
            });
            let session =
                Session::connect_stream(client, rfb_engine::session::SessionConfig::new("test", 1))
                    .await
                    .unwrap();
            let mut input = Input {
                enabled: !view_only,
                focused: true,
                ..Input::default()
            };
            let viewport = Viewport::fit((160, 160), (160, 80));
            let button = |y| SdlEvent::MouseButtonDown {
                timestamp: 0,
                window_id: 0,
                which: 0,
                mouse_btn: MouseButton::Left,
                clicks: 1,
                x: 40.0,
                y,
            };
            let wheel = SdlEvent::MouseWheel {
                timestamp: 0,
                window_id: 0,
                which: 0,
                x: 0.0,
                y: 1.0,
                direction: MouseWheelDirection::Normal,
                mouse_x: 80.0,
                mouse_y: 80.0,
                integer_x: 0,
                integer_y: 1,
            };
            let key = SdlEvent::KeyDown {
                timestamp: 0,
                window_id: 0,
                keycode: Some(Keycode::RCtrl),
                scancode: Some(Scancode::RCtrl),
                keymod: Mod::NOMOD,
                repeat: false,
                which: 0,
                raw: 0,
            };
            for event in [button(10.0), button(60.0), wheel, key] {
                input.event(&session, event, viewport).await.unwrap();
            }
            if focus_loss {
                let blur = SdlEvent::Window {
                    timestamp: 0,
                    window_id: 0,
                    win_event: WindowEvent::FocusLost,
                };
                input.event(&session, blur, viewport).await.unwrap();
                input.event(&session, button(60.0), viewport).await.unwrap();
            }
            input.release(&session).await.unwrap();
            session.disconnect().await;
            let written = time::timeout(Duration::from_secs(2), server)
                .await
                .unwrap()
                .unwrap();
            if view_only {
                assert!(written.is_empty());
            } else {
                assert_eq!(
                    written,
                    [
                        5, 1, 0, 40, 0, 20, // click, letterbox offset removed
                        5, 9, 0, 80, 0, 40, // wheel-up press preserves left
                        5, 1, 0, 80, 0, 40, // wheel release preserves left
                        4, 1, 0, 0, 0, 0, 0xff, 0xe4, // right control down
                        5, 0, 0, 80, 0, 40, // release all buttons
                        4, 0, 0, 0, 0, 0, 0xff, 0xe4, // release held key
                    ]
                );
            }
        }
    }
}
