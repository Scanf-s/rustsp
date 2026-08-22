// Error type for the whole client.
//
// There are three kinds of failure, and it is useful to keep them apart:
// - Io: the socket or the file system reported an error
// - Protocol: the bytes arrived, but they do not follow the rules of RTSP
// - Status: the message was correct RTSP, but the server refused the request
//   (404, 459, 551 and so on)

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

/// A short way to build a Protocol error.
pub fn protocol<T>(msg: impl Into<String>) -> Result<T> {
    Err(Error::Protocol(msg.into()))
}
