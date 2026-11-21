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
        NonNull::new(unsafe { xdb_client_connect(port, None, ptr::null_mut()) })
            .map(Self)
            .ok_or_else(last_error)
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

pub struct Frame {
    pub serial: u64,
    pub width: u32,
    pub height: u32,
    pub row_pitch: u32,
    pub display: Value,
    pub pixels: Vec<u8>,
}

pub struct Session {
    client: Client,
}

impl Session {
    pub fn open(
        port: u16,
        udid: &str,
        transport: &str,
        hardware_decode: bool,
    ) -> Result<Self, Error> {
        let mut client = Client::connect(port)?;
        client.request(
            c"connection.open",
            json!({"udid": udid, "transport": transport}),
        )?;
        let mut session = Self { client };
        session.client.request(
            c"video.start",
            json!({"decode": true, "hardware_decode": hardware_decode}),
        )?;
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

    pub fn touch(&mut self, x: f64, y: f64, down: bool) -> Result<(), Error> {
        self.client
            .request(c"input.touch", json!({"x": x, "y": y, "down": down}))
            .map(|_| ())
    }

    pub fn keyboard(&mut self, usages: &[u16]) -> Result<(), Error> {
        self.client
            .request(c"input.keyboard", json!({"usages": usages}))
            .map(|_| ())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.client.request(c"connection.close", json!({}));
    }
}
