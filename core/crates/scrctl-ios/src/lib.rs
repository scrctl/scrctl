#![cfg(feature = "xdb")]

use serde_json::{Value, json};
use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::ptr::{self, NonNull};

unsafe extern "C" {
    fn xdb_client_connect(
        port: u16,
        event: Option<unsafe extern "C" fn(*const c_char, *mut c_void)>,
        userdata: *mut c_void,
    ) -> *mut c_void;
    fn xdb_client_server_version(client: *const c_void) -> *const c_char;
    fn xdb_client_destroy(client: *mut c_void);
    fn xdb_client_request(
        client: *mut c_void,
        method: *const c_char,
        params: *const c_char,
        response: *mut *mut c_void,
    ) -> c_int;
    fn xdb_last_error() -> *const c_char;
    fn xdb_response_json(response: *const c_void) -> *const c_char;
    fn xdb_response_data(response: *const c_void) -> *const u8;
    fn xdb_response_size(response: *const c_void) -> usize;
    fn xdb_response_destroy(response: *mut c_void);
}

#[derive(Debug)]
pub enum Error {
    Client(String),
    Server { code: String, message: String },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Client(message) => f.write_str(message),
            Self::Server { code, message } => write!(f, "{code}: {message}"),
        }
    }
}

impl std::error::Error for Error {}

fn last_error() -> Error {
    // The diagnostic is borrowed until the next SDK call.
    let message = unsafe {
        let value = xdb_last_error();
        if value.is_null() {
            "XDB client request failed".into()
        } else {
            CStr::from_ptr(value).to_string_lossy().into_owned()
        }
    };
    Error::Client(message)
}

struct Client(NonNull<c_void>);
// A client owns its socket and callbacks use no user data or thread-affine state.
// Exclusive Rust access serializes all SDK calls, including destruction.
unsafe impl Send for Client {}

struct Response(NonNull<c_void>);

impl Drop for Client {
    fn drop(&mut self) {
        unsafe { xdb_client_destroy(self.0.as_ptr()) }
    }
}

impl Drop for Response {
    fn drop(&mut self) {
        unsafe { xdb_response_destroy(self.0.as_ptr()) }
    }
}

impl Client {
    fn connect(port: u16) -> Result<Self, Error> {
        Self::connect_with_events(port, None)
    }

    fn connect_with_events(
        port: u16,
        event: Option<unsafe extern "C" fn(*const c_char, *mut c_void)>,
    ) -> Result<Self, Error> {
        NonNull::new(unsafe { xdb_client_connect(port, event, ptr::null_mut()) })
            .map(Self)
            .ok_or_else(last_error)
    }

    fn supports_multi_touch(&self) -> bool {
        // The version is borrowed from the live client.
        let version = unsafe { xdb_client_server_version(self.0.as_ptr()) };
        if version.is_null() {
            return false;
        }
        let Ok(version) = unsafe { CStr::from_ptr(version) }.to_str() else {
            return false;
        };
        let mut parts = version.split('.');
        let mut numbers = [0u32; 3];
        for n in &mut numbers {
            let Some(value) = parts.next().and_then(|p| p.parse().ok()) else {
                return false;
            };
            *n = value;
        }
        parts.next().is_none() && numbers >= [1, 4, 0]
    }

    fn request(&mut self, method: &CStr, params: Value) -> Result<(Value, Vec<u8>), Error> {
        let params = CString::new(params.to_string()).expect("JSON escapes NUL");
        let mut result = ptr::null_mut();
        let ok = unsafe {
            xdb_client_request(
                self.0.as_ptr(),
                method.as_ptr(),
                params.as_ptr(),
                &mut result,
            )
        };
        let response = NonNull::new(result).map(Response);
        if ok == 0 {
            return Err(last_error());
        }
        let response = response.ok_or_else(|| Error::Client("Missing XDB response".into()))?;
        let raw = unsafe { xdb_response_json(response.0.as_ptr()) };
        if raw.is_null() {
            return Err(Error::Client("Missing XDB response JSON".into()));
        }
        let mut json: Value = serde_json::from_slice(unsafe { CStr::from_ptr(raw) }.to_bytes())
            .map_err(|e| Error::Client(format!("Invalid XDB response: {e}")))?;
        if json["ok"].as_bool() != Some(true) {
            return Err(Error::Server {
                code: json["error"]["code"].as_str().unwrap_or("unknown").into(),
                message: json["error"]["message"]
                    .as_str()
                    .unwrap_or("XDB request rejected")
                    .into(),
            });
        }
        let size = unsafe { xdb_response_size(response.0.as_ptr()) };
        let data = if size == 0 {
            Vec::new()
        } else {
            let bytes = unsafe { xdb_response_data(response.0.as_ptr()) };
            if bytes.is_null() || size > isize::MAX as usize {
                return Err(Error::Client("Invalid XDB binary response".into()));
            }
            unsafe { std::slice::from_raw_parts(bytes, size) }.to_vec()
        };
        Ok((json["result"].take(), data))
    }
}

pub fn devices(port: u16, transport: &str, timeout_ms: u32) -> Result<Value, Error> {
    let mut client = Client::connect(port)?;
    let (devices, _) = client.request(
        c"devices",
        json!({
            "transport": transport, "timeout_ms": timeout_ms,
        }),
    )?;
    Ok(devices)
}

pub struct Frame {
    pub serial: u64,
    pub width: u32,
    pub height: u32,
    pub row_pitch: u32,
    pub display: Value,
    pub pixels: Vec<u8>,
}

#[derive(Clone, Copy)]
pub struct VideoRecording<'a> {
    pub path: &'a str,
    pub container: Option<&'a str>,
    pub audio: bool,
    pub video: bool,
    pub decode_video: bool,
    pub orientation: Option<u16>,
}

#[derive(Clone, Copy, Default)]
pub struct VideoOptions<'a> {
    pub hardware_decode: bool,
    pub recording: Option<VideoRecording<'a>>,
    pub max_bitrate: Option<u64>,
    pub max_fps: u16,
    pub skip_video: bool,
}

pub struct Session {
    client: Client,
    open: bool,
    port: u16,
    attachment: Option<String>,
    recording_pending: bool,
}

impl Session {
    pub fn open(
        port: u16,
        udid: &str,
        transport: &str,
        hardware_decode: bool,
    ) -> Result<Self, Error> {
        Self::open_target(port, udid, transport, "", 49152, hardware_decode)
    }

    pub fn open_target(
        port: u16,
        udid: &str,
        transport: &str,
        wifi: &str,
        wifi_port: u16,
        hardware_decode: bool,
    ) -> Result<Self, Error> {
        Self::open_video(
            port,
            udid,
            transport,
            wifi,
            wifi_port,
            VideoOptions {
                hardware_decode,
                ..VideoOptions::default()
            },
        )
    }

    pub fn open_video(
        port: u16,
        udid: &str,
        transport: &str,
        wifi: &str,
        wifi_port: u16,
        options: VideoOptions<'_>,
    ) -> Result<Self, Error> {
        let recording = options.recording;
        let decode = recording.is_none_or(|recording| recording.decode_video);
        if options.max_fps > 0 && !decode {
            return Err(Error::Client(
                "--max-fps requires decoded video output".into(),
            ));
        }
        let mut client = Client::connect(port)?;
        if options.max_bitrate.unwrap_or(0) > 0 || options.max_fps > 0 {
            let (info, _) = client.request(c"server.info", json!({}))?;
            let capabilities = &info["capabilities"];
            if (options.max_bitrate.unwrap_or(0) > 0 && capabilities["video_max_bitrate"] != true)
                || (options.max_fps > 0 && capabilities["video_max_fps"] != true)
            {
                return Err(Error::Client(
                    "xdb Server does not support the requested video bitrate or output frame-rate limit".into(),
                ));
            }
        }
        let mut params = json!({"udid": udid, "transport": transport});
        if !wifi.is_empty() {
            params["wifi"] = json!(wifi);
            params["port"] = json!(wifi_port);
        }
        let (opened, _) = client.request(c"connection.open", params)?;
        let mut session = Self {
            client,
            open: true,
            port,
            attachment: opened["attachment"].as_str().map(str::to_owned),
            recording_pending: recording.is_some(),
        };
        let mut video = json!({
            "decode": decode,
            "hardware_decode": decode && options.hardware_decode,
        });
        if let Some(max_bitrate) = options.max_bitrate {
            video["max_bitrate"] = json!(max_bitrate);
        }
        if options.max_fps > 0 {
            video["max_fps"] = json!(options.max_fps);
        }
        if let Some(recording) = recording {
            if let Some(format) = recording.container {
                session.client.request(
                    c"recording.configure",
                    json!({
                        "path": recording.path, "format": format, "video": recording.video, "audio": recording.audio,
                        "orientation": recording.orientation.unwrap_or(0),
                        "orientation_from_device": recording.orientation.is_none(),
                    }),
                )?;
            } else {
                video["record_path"] = json!(recording.path);
            }
        }
        if !options.skip_video && recording.is_none_or(|recording| recording.video) {
            session.client.request(c"video.start", video)?;
        }
        Ok(session)
    }

    pub fn next_frame(&mut self, since: u64, timeout_ms: u16) -> Result<Option<Frame>, Error> {
        let (metadata, pixels) = match self.client.request(
            c"video.frame",
            json!({"since": since, "timeout_ms": timeout_ms}),
        ) {
            Err(Error::Server { code, .. }) if code == "no_frame" => return Ok(None),
            result => result?,
        };
        let dimension = |name| {
            metadata[name]
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .unwrap_or(0)
        };
        let (width, height, row_pitch) = (
            dimension("width"),
            dimension("height"),
            dimension("row_pitch"),
        );
        let serial = metadata["serial"].as_u64().filter(|serial| *serial > since);
        if let Some(serial) = serial
            && metadata["format"].as_str() == Some("bgra")
            && width > 0
            && height > 0
            && u64::from(row_pitch) >= u64::from(width) * 4
            && u64::from(row_pitch) * u64::from(height) == pixels.len() as u64
        {
            return Ok(Some(Frame {
                serial,
                width,
                height,
                row_pitch,
                display: metadata["display"].clone(),
                pixels,
            }));
        }
        Err(Error::Client("Invalid XDB BGRA frame".into()))
    }

    pub fn supports_multi_touch(&self) -> bool {
        self.client.supports_multi_touch()
    }

    pub fn touch(&mut self, x: f64, y: f64, down: bool) -> Result<(), Error> {
        self.client
            .request(c"input.touch", json!({"x": x, "y": y, "down": down}))
            .map(|_| ())
    }

    pub fn touch_contact(&mut self, contact: u8, x: f64, y: f64, down: bool) -> Result<(), Error> {
        let multi_touch = self.supports_multi_touch();
        if contact >= 5 || (!multi_touch && contact != 0) {
            return Err(Error::Client(
                "Multi-touch requires XDB Server 1.4.0 or later".into(),
            ));
        }
        let mut params = json!({"x": x, "y": y, "down": down});
        if multi_touch {
            params["contact"] = json!(contact);
        }
        self.client.request(c"input.touch", params).map(|_| ())
    }

    pub fn button(&mut self, usage: u16, down: bool) -> Result<(), Error> {
        self.client
            .request(
                c"input.button",
                json!({"page": 12, "usage": usage, "down": down}),
            )
            .map(|_| ())
    }

    pub fn prepare_clipboard(
        &self,
        text: String,
        cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<std::sync::mpsc::Receiver<Result<(), Error>>, Error> {
        use std::sync::atomic::Ordering::Acquire;
        let attachment = self
            .attachment
            .clone()
            .ok_or_else(|| Error::Client("Missing XDB session attachment".into()))?;
        let port = self.port;
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("scrctl-clipboard".into())
            .spawn(move || {
                let sync = || -> Result<(), Error> {
                    let mut client = Client::connect(port)?;
                    client.request(c"connection.attach", json!({"attachment": attachment}))?;
                    if cancelled.load(Acquire) {
                        return Ok(());
                    }
                    client.request(c"clipboard.set", json!({"text": &text}))?;
                    if cancelled.load(Acquire) {
                        return Ok(());
                    }
                    let (readback, _) = client.request(c"clipboard.get", json!({}))?;
                    if readback["text"].as_str() != Some(&text) {
                        return Err(Error::Client(
                            "Device clipboard text did not match; paste skipped".into(),
                        ));
                    }
                    Ok(())
                };
                let _ = sender.send(sync());
            })
            .map_err(|error| Error::Client(error.to_string()))?;
        Ok(receiver)
    }

    pub fn paste_keyboard(&mut self) -> Result<(), Error> {
        self.keyboard(&[0xe3])?;
        let press = self.keyboard(&[0xe3, 0x19]);
        let release = self.keyboard(&[]);
        press.and(release)
    }

    pub fn rotate_device(&mut self) -> Result<(), Error> {
        self.client
            .request(c"display.rotate", json!({}))
            .map(|_| ())
    }

    pub fn keyboard(&mut self, usages: &[u16]) -> Result<(), Error> {
        self.client
            .request(c"input.keyboard", json!({"usages": usages}))
            .map(|_| ())
    }
}

impl Session {
    pub fn check_media(&mut self) -> Result<(), Error> {
        let (value, _) = self.client.request(c"media.status", json!({}))?;
        for field in ["error", "recording_error"] {
            if let Some(error) = value[field].as_str().filter(|value| !value.is_empty()) {
                return Err(Error::Client(error.to_owned()));
            }
        }
        Ok(())
    }

    pub fn finish_media(&mut self) -> Result<(), Error> {
        self.recording_pending = false;
        self.client.request(c"media.finish", json!({})).map(|_| ())
    }

    pub fn close(mut self) -> Result<(), Error> {
        self.close_connection()
    }

    fn close_connection(&mut self) -> Result<(), Error> {
        if !std::mem::replace(&mut self.open, false) {
            return Ok(());
        }
        self.client
            .request(c"connection.close", json!({}))
            .map(|_| ())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if self.recording_pending {
            let _ = self.finish_media();
        }
        let _ = self.close_connection();
    }
}

pub struct AudioReader {
    client: Client,
    rate: i32,
    channels: u8,
}

impl Session {
    pub fn start_audio(&mut self, duplicate: bool, buffer_ms: u32) -> Result<AudioReader, Error> {
        let attachment = self
            .attachment
            .as_deref()
            .ok_or_else(|| Error::Client("Missing XDB session attachment".into()))?;
        let mut client = Client::connect(self.port)?;
        client.request(c"connection.attach", json!({"attachment": attachment}))?;
        let (info, _) = self.client.request(
            c"audio.start",
            json!({"decode": true, "duplicate": duplicate, "buffer_ms": buffer_ms}),
        )?;
        let rate = info["sample_rate"]
            .as_i64()
            .and_then(|n| i32::try_from(n).ok())
            .filter(|n| *n > 0);
        let channels = info["channels"]
            .as_u64()
            .and_then(|n| u8::try_from(n).ok())
            .filter(|n| *n > 0);
        match (rate, channels) {
            (Some(rate), Some(channels)) => Ok(AudioReader {
                client,
                rate,
                channels,
            }),
            _ => {
                let _ = self.stop_audio();
                Err(Error::Client("Invalid XDB audio format".into()))
            }
        }
    }

    pub fn start_recording_audio(&mut self, duplicate: bool, buffer_ms: u32) -> Result<(), Error> {
        self.client
            .request(
                c"audio.start",
                json!({"decode": false, "duplicate": duplicate, "buffer_ms": buffer_ms}),
            )
            .map(|_| ())
    }

    pub fn stop_audio(&mut self) -> Result<(), Error> {
        self.client.request(c"audio.stop", json!({})).map(|_| ())
    }
}

impl AudioReader {
    pub fn format(&self) -> (i32, u8) {
        (self.rate, self.channels)
    }

    pub fn read(&mut self, frames: u32) -> Result<Vec<i16>, Error> {
        let (info, bytes) = self
            .client
            .request(c"audio.read", json!({"frames": frames}))?;
        let returned = info["frames"].as_u64().unwrap_or(u64::MAX);
        if returned > u64::from(frames)
            || returned * u64::from(self.channels) * 2 != bytes.len() as u64
        {
            return Err(Error::Client("Invalid XDB PCM frame count".into()));
        }
        Ok(bytes
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect())
    }
}

/// Run an operation on a device without starting a media stream.
pub fn device_commands(
    port: u16,
    udid: &str,
    wifi: &str,
    wifi_port: u16,
    list_apps: bool,
    copy: Option<&str>,
    paste: bool,
) -> Result<(), Error> {
    let mut client = Client::connect(port)?;
    client.request(
        c"connection.open",
        json!({"udid": udid, "wifi": wifi, "port": wifi_port}),
    )?;
    // The legacy commands release the association by closing the client socket.
    if list_apps {
        let (apps, _) = client.request(c"app.list", json!({}))?;
        for app in apps
            .as_array()
            .ok_or_else(|| Error::Client("Invalid XDB app list".into()))?
        {
            let field = |name| {
                app[name]
                    .as_str()
                    .ok_or_else(|| Error::Client("Invalid XDB app entry".into()))
            };
            println!("{}  {}", field("bundle_id")?, field("name")?);
        }
    }
    if let Some(text) = copy {
        client.request(c"clipboard.set", json!({"text": text}))?;
    }
    if paste {
        let (value, _) = client.request(c"clipboard.get", json!({}))?;
        let text = value["text"]
            .as_str()
            .ok_or_else(|| Error::Client("Invalid XDB clipboard response".into()))?;
        println!("{text}");
    }
    Ok(())
}

unsafe extern "C" fn print_management_event(text: *const c_char, _: *mut c_void) {
    use std::io::Write;

    if text.is_null() {
        return;
    }
    // XDB borrows the event JSON only for the duration of this callback.
    let Ok(event) = serde_json::from_slice::<Value>(unsafe { CStr::from_ptr(text) }.to_bytes())
    else {
        return;
    };
    let mut stderr = std::io::stderr().lock();
    if event["event"] == "pin" {
        let _ = writeln!(
            stderr,
            "Pairing code: {}",
            event["data"]["pin"].as_str().unwrap_or("")
        );
    } else if let Some(message) = event["data"]["message"].as_str() {
        let _ = writeln!(stderr, "{message}");
    }
}

pub fn pair_device(udid: &str, wifi: bool, repair: bool, timeout_ms: u32) -> Result<String, Error> {
    let mut client = Client::connect_with_events(0, Some(print_management_event))?;
    let (value, _) = client.request(
        c"pair",
        json!({
            "udid": udid, "transport": if wifi { "wifi" } else { "usb" },
            "repair": repair, "timeout_ms": timeout_ms,
        }),
    )?;
    Ok(string_field(&value, "udid")?.into())
}

fn string_field<'a>(value: &'a Value, field: &str) -> Result<&'a str, Error> {
    value[field]
        .as_str()
        .ok_or_else(|| Error::Client(format!("Missing XDB response field {field}")))
}

pub struct DdiOptions<'a> {
    pub udid: &'a str,
    pub wifi: &'a str,
    pub wifi_port: u16,
    pub directory: &'a str,
    pub download_timeout: u32,
}

pub fn download_ddi(
    options: DdiOptions<'_>,
    version: Option<&str>,
    model: &str,
) -> Result<Value, Error> {
    let mut client = Client::connect_with_events(0, Some(print_management_event))?;
    let info;
    let (version, model) = if let Some(version) = version {
        (version, model)
    } else {
        client.request(
            c"connection.open",
            json!({
                "udid": options.udid, "wifi": options.wifi, "port": options.wifi_port,
            }),
        )?;
        (info, _) = client.request(c"device.info", json!({}))?;
        client.request(c"connection.close", json!({}))?;
        (
            string_field(&info, "os_version")?,
            string_field(&info, "product_type")?,
        )
    };
    let (value, _) = client.request(
        c"ddi.download",
        json!({
            "os_version": version, "product_type": model, "cache_directory": options.directory,
            "timeout_seconds": options.download_timeout,
        }),
    )?;
    Ok(value)
}

pub fn install_ddi(
    options: DdiOptions<'_>,
    replace: bool,
    install_timeout: u32,
) -> Result<(), Error> {
    let mut client = Client::connect_with_events(0, Some(print_management_event))?;
    let discovered;
    let udid = if options.udid.is_empty() {
        (discovered, _) = client.request(c"devices", json!({
            "transport": if options.wifi.is_empty() { "auto" } else { "wifi" }, "timeout_ms": 3000,
        }))?;
        let Some([device]) = discovered["devices"].as_array().map(Vec::as_slice) else {
            return Err(Error::Client(
                "Select one identified device with -s before preparing DDI".into(),
            ));
        };
        string_field(device, "udid")
            .ok()
            .filter(|udid| !udid.is_empty())
            .ok_or_else(|| {
                Error::Client("Select one identified device with -s before preparing DDI".into())
            })?
    } else {
        options.udid
    };
    client.request(c"ddi.prepare", json!({
        "udid": udid, "wifi": options.wifi, "port": options.wifi_port,
        "cache_directory": options.directory, "replace": replace,
        "download_timeout_seconds": options.download_timeout, "install_timeout_seconds": install_timeout,
    }))?;
    Ok(())
}
