use std::sync::{Arc, Mutex};

use scrctl_ios::Session;

#[derive(Clone, Copy, uniffi::Enum)]
pub enum IosTransport {
    Auto,
    Usb,
    Wifi,
}

impl IosTransport {
    fn name(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Usb => "usb",
            Self::Wifi => "wifi",
        }
    }
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum IosError {
    #[error("{detail}")]
    Client { detail: String },
    #[error("{code}: {detail}")]
    Server { code: String, detail: String },
    #[error("iOS session is closed")]
    Closed,
}

impl From<scrctl_ios::Error> for IosError {
    fn from(error: scrctl_ios::Error) -> Self {
        match error {
            scrctl_ios::Error::Client(message) => Self::Client { detail: message },
            scrctl_ios::Error::Server { code, message } => Self::Server {
                code,
                detail: message,
            },
        }
    }
}

#[derive(uniffi::Record)]
pub struct IosFrame {
    pub serial: u64,
    pub width: u32,
    pub height: u32,
    pub row_pitch: u32,
    pub display_width: Option<u32>,
    pub display_height: Option<u32>,
    pub orientation: Option<String>,
    pub pixels: Vec<u8>,
}

/// Returns the documented XDB devices JSON result, including all candidates and warnings.
#[uniffi::export]
pub fn ios_list_devices(
    port: u16,
    transport: IosTransport,
    timeout_ms: u32,
) -> Result<String, IosError> {
    Ok(scrctl_ios::devices(port, transport.name(), timeout_ms.min(60_000))?.to_string())
}

#[derive(uniffi::Object)]
pub struct IosSession {
    session: Mutex<Option<Session>>,
}

impl IosSession {
    fn with_session<T>(
        &self,
        call: impl FnOnce(&mut Session) -> Result<T, scrctl_ios::Error>,
    ) -> Result<T, IosError> {
        let mut guard = self.session.lock().map_err(|_| IosError::Closed)?;
        Ok(call(guard.as_mut().ok_or(IosError::Closed)?)?)
    }
}

#[uniffi::export]
impl IosSession {
    #[uniffi::constructor]
    pub fn open(
        port: u16,
        udid: String,
        transport: IosTransport,
        hardware_decode: bool,
    ) -> Result<Arc<Self>, IosError> {
        let session = Session::open(port, &udid, transport.name(), hardware_decode)?;
        Ok(Arc::new(Self {
            session: Mutex::new(Some(session)),
        }))
    }

    /// The timeout bounds time waiting for a new frame on the server.
    pub fn next_frame(&self, since: u64, timeout_ms: u16) -> Result<Option<IosFrame>, IosError> {
        let frame = self.with_session(|session| session.next_frame(since, timeout_ms.min(1000)))?;
        Ok(frame.map(|frame| {
            let dim = |key| {
                frame.display[key]
                    .as_u64()
                    .and_then(|n| u32::try_from(n).ok())
            };
            IosFrame {
                serial: frame.serial,
                width: frame.width,
                height: frame.height,
                row_pitch: frame.row_pitch,
                display_width: dim("width"),
                display_height: dim("height"),
                orientation: frame.display["orientation"].as_str().map(str::to_owned),
                pixels: frame.pixels,
            }
        }))
    }

    pub fn touch(&self, x: f64, y: f64, down: bool) -> Result<(), IosError> {
        self.with_session(|session| session.touch(x, y, down))
    }

    pub fn keyboard(&self, usages: Vec<u16>) -> Result<(), IosError> {
        self.with_session(|session| session.keyboard(&usages))
    }

    pub fn shutdown(&self) -> Result<(), IosError> {
        let mut guard = self.session.lock().map_err(|_| IosError::Closed)?;
        if let Some(session) = guard.take() {
            session.close()?;
        }
        Ok(())
    }
}
