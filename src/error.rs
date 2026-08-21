// Error type for the whole client.
//
// Three failure modes worth telling apart:
// - Io: the socket or the filesystem failed
// - Protocol: bytes arrived, but they did not mean what RTSP says they should
// - Status: a well formed response that refused the request (404, 459, 551, ...)

use std::fmt;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Protocol(String),
    Status { code: u16, reason: String },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "io: {e}"),
            Error::Protocol(m) => write!(f, "protocol: {m}"),
            Error::Status { code, reason } => write!(f, "server refused: {code} {reason}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// Shorthand for building a Protocol error.
pub fn protocol<T>(msg: impl Into<String>) -> Result<T> {
    Err(Error::Protocol(msg.into()))
}
