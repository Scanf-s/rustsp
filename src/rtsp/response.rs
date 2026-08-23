// Parsing RTSP responses (RFC 2326 section 7)
//
//   RTSP/1.0 200 OK\r\n
//   CSeq: 1\r\n
//   Content-Length: 460\r\n     <- if present, exactly that many body bytes follow
//   \r\n
//   <body>
//
// Two rules keep this parser reliable:
// - The empty line marks the end of the headers. Do not read until the other side
//   closes the connection, because an RTSP server keeps the connection open for the
//   next request, so such a read would block forever.
// - The length of a body is always known in advance. RFC 2326 section 9.3 requires
//   Content-Length whenever a message carries a body, and RTSP has no chunked
//   encoding. That is why read_exact is always the correct choice and read() is not.

use std::io::BufRead;

use crate::error::{Error, Result, protocol};

#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub reason: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    /// Look up one header. Header names are case insensitive in RTSP, so the
    /// comparison ignores case as well.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    pub fn body_text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    /// Convert a refusal into an error, so that callers can use the `?` operator.
    pub fn ok(self) -> Result<Self> {
        if (200..300).contains(&self.status) {
            Ok(self)
        } else {
            Err(Error::Status {
                code: self.status,
                reason: self.reason,
            })
        }
    }
}

pub fn read<R: BufRead>(reader: &mut R) -> Result<Response> {
    // --- status line: "RTSP/1.0 200 OK" ---
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        return protocol("connection closed before a status line arrived");
    }
    let line = line.trim_end_matches(['\r', '\n']);

    let mut parts = line.splitn(3, ' ');
    let version = parts.next().unwrap_or_default();
    if !version.starts_with("RTSP/") {
        return protocol(format!("expected an RTSP status line, got {line:?}"));
    }
    let status: u16 = match parts.next().and_then(|c| c.parse().ok()) {
        Some(c) => c,
        None => return protocol(format!("no status code in {line:?}")),
    };
    let reason = parts.next().unwrap_or_default().to_string();

    // --- headers, up to the empty line ---
    let mut headers = Vec::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return protocol("connection closed inside the headers");
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        // Split on the first colon only, because a value can contain further
        // colons, for example inside a URL or a timestamp.
        match line.split_once(':') {
            Some((name, value)) => {
                headers.push((name.trim().to_string(), value.trim().to_string()))
            }
            None => return protocol(format!("header line has no colon: {line:?}")),
        }
    }

    // --- body, exactly Content-Length bytes ---
    let len: usize = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("Content-Length"))
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);

    let mut body = vec![0u8; len];
    if len > 0 {
        reader.read_exact(&mut body)?;
    }

    Ok(Response {
        status,
        reason,
        headers,
        body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Read};

    #[test]
    fn options_response_without_body() {
        let raw = "RTSP/1.0 200 OK\r\n\
                   CSeq: 1\r\n\
                   Public: DESCRIBE, SETUP, PLAY, TEARDOWN\r\n\
                   \r\n";
        let r = read(&mut Cursor::new(raw)).unwrap();
        assert_eq!(r.status, 200);
        assert_eq!(r.reason, "OK");
        assert_eq!(r.header("cseq"), Some("1")); // the name is matched without case
        assert!(r.body.is_empty());
    }

    #[test]
    fn body_is_read_by_length_and_nothing_more() {
        // The 5 bytes after the body have to stay in the reader, because the next
        // response starts there.
        let raw = "RTSP/1.0 200 OK\r\n\
                   Content-Length: 4\r\n\
                   \r\n\
                   v=0\nLEFTO";
        let mut cursor = Cursor::new(raw);
        let r = read(&mut cursor).unwrap();
        assert_eq!(r.body, b"v=0\n");

        let mut rest = String::new();
        cursor.read_to_string(&mut rest).unwrap();
        assert_eq!(rest, "LEFTO");
    }

    #[test]
    fn header_value_may_contain_colons() {
        let raw = "RTSP/1.0 200 OK\r\n\
                   RTP-Info: url=rtsp://h/s/track;seq=9810092\r\n\
                   \r\n";
        let r = read(&mut Cursor::new(raw)).unwrap();
        assert_eq!(
            r.header("RTP-Info"),
            Some("url=rtsp://h/s/track;seq=9810092")
        );
    }

    #[test]
    fn refusal_becomes_an_error() {
        let raw = "RTSP/1.0 459 Aggregate operation not allowed\r\n\r\n";
        let r = read(&mut Cursor::new(raw)).unwrap();
        assert_eq!(r.status, 459);
        assert!(r.ok().is_err());
    }
}
