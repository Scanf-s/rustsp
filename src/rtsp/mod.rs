// RTSP session state (RFC 2326)
//
// The Session owns the connection, so it also holds the two values that belong to
// the connection rather than to a single request:
//   - the CSeq counter, which counts per connection and not globally
//     (see RFC 2326 section 12.17)
//   - the Session id that the server returns in the SETUP response
//
// A caller never builds a request string and never updates CSeq. A caller only
// states what it wants:
//   session.describe()?;
//   session.setup(&track_url)?;
//
// A single BufReader owns the TcpStream for the whole connection, and every write
// goes through get_mut(). There is no way to reach the raw stream, and that is
// intentional: reading the stream directly would skip the bytes that BufReader has
// already buffered. This becomes important from week 3, when interleaved RTP
// arrives on the same socket.

pub mod request;
pub mod response;

use std::io::{BufReader, Write};
use std::net::TcpStream;
use std::time::Duration;

use crate::error::{protocol, Result};
use crate::sdp::{self, Sdp};
use response::Response;

/// The current point in the session lifecycle. RTSP keeps state, so the set of
/// methods that the server accepts depends on this value (RFC 2326 Appendix A).
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum State {
    Init,
    Ready,
    Playing,
}

pub struct Session {
    conn: BufReader<TcpStream>,
    /// The presentation URL, which is also called the aggregate URL. PLAY, PAUSE
    /// and TEARDOWN are sent to this URL.
    base_url: String,
    cseq: u32,
    session_id: Option<String>,
    state: State,
    /// Print every request and every response to stderr. This trace is the main
    /// learning tool, because what you expected should match what appears here.
    pub trace: bool,
}

impl Session {
    pub fn connect(url: &str) -> Result<Self> {
        let authority = authority_of(url)?;
        let stream = TcpStream::connect(&authority)?;
        // Report an error quickly instead of waiting forever. Almost every framing
        // mistake in RTSP appears as a read that never finishes, not as an error.
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;

        Ok(Session {
            conn: BufReader::new(stream),
            base_url: url.trim_end_matches('/').to_string(),
            cseq: 0,
            session_id: None,
            state: State::Init,
            trace: false,
        })
    }

    pub fn state(&self) -> State {
        self.state
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    /// Give the buffered reader to the transport layer after PLAY has succeeded.
    /// From that point on, the same connection carries interleaved binary data.
    pub fn reader(&mut self) -> &mut BufReader<TcpStream> {
        &mut self.conn
    }

    /// Send one request and read one response. The CSeq and Session headers are
    /// added here.
    pub fn request(&mut self, method: &str, url: &str, extra: &[(&str, &str)]) -> Result<Response> {
        self.cseq += 1;

        let mut headers: Vec<(&str, &str)> = extra.to_vec();
        if let Some(id) = self.session_id.as_deref() {
            headers.push(("Session", id));
        }
        headers.push(("User-Agent", "rustsp"));

        let raw = request::build(method, url, self.cseq, &headers);
        if self.trace {
            for line in raw.lines().filter(|l| !l.is_empty()) {
                eprintln!("--> {line}");
            }
        }

        self.conn.get_mut().write_all(raw.as_bytes())?;

        let resp = response::read(&mut self.conn)?;
        if self.trace {
            eprintln!("<-- RTSP/1.0 {} {}", resp.status, resp.reason);
            for (k, v) in &resp.headers {
                eprintln!("<-- {k}: {v}");
            }
            if !resp.body.is_empty() {
                eprintln!("<-- ({} byte body)", resp.body.len());
            }
        }

        // The response has to repeat the CSeq value that was sent, because that is
        // how a response is matched with its request.
        if let Some(echo) = resp.header("CSeq") {
            if echo.trim().parse::<u32>() != Ok(self.cseq) {
                return protocol(format!("CSeq mismatch: sent {}, got {echo}", self.cseq));
            }
        }

        Ok(resp)
    }

    pub fn options(&mut self) -> Result<Response> {
        let url = self.base_url.clone();
        self.request("OPTIONS", &url, &[])?.ok()
    }

    pub fn describe(&mut self) -> Result<Sdp> {
        let url = self.base_url.clone();
        let resp = self
            .request("DESCRIBE", &url, &[("Accept", "application/sdp")])?
            .ok()?;
        sdp::parse(&resp.body_text())
    }

    /// SETUP is sent to the track URL, which comes from a=control, and never to
    /// the aggregate URL. If it is sent to the aggregate URL, the server answers
    /// with 459 (RFC 2326 section 14.2). Here the client asks for TCP interleaved
    /// transport, with RTP on channel 0 and RTCP on channel 1.
    pub fn setup(&mut self, track_url: &str) -> Result<Response> {
        let resp = self
            .request(
                "SETUP",
                track_url,
                &[("Transport", "RTP/AVP/TCP;unicast;interleaved=0-1")],
            )?
            .ok()?;

        // "Session: 12345678;timeout=60": keep the id and remove the parameters.
        let id = resp
            .header("Session")
            .map(|v| v.split(';').next().unwrap_or(v).trim().to_string());
        match id {
            Some(id) => self.session_id = Some(id),
            None => return protocol("SETUP response had no Session header"),
        }
        self.state = State::Ready;
        Ok(resp)
    }

    /// PLAY is sent to the aggregate URL and needs the Session header. Now that
    /// SETUP has run, `request` adds that header automatically.
    pub fn play(&mut self) -> Result<Response> {
        if self.state != State::Ready {
            return protocol(format!("PLAY requires state Ready, was {:?}", self.state));
        }
        let url = self.base_url.clone();
        let resp = self.request("PLAY", &url, &[("Range", "npt=0.000-")])?.ok()?;
        self.state = State::Playing;
        Ok(resp)
    }

    pub fn teardown(&mut self) -> Result<Response> {
        let url = self.base_url.clone();
        let resp = self.request("TEARDOWN", &url, &[])?.ok()?;
        self.session_id = None;
        self.state = State::Init;
        Ok(resp)
    }

    #[allow(dead_code)] // week 3
    /// Keep the session alive. An RTSP session expires when the client sends
    /// nothing for a while, and OPTIONS is the usual method for keeping it open
    /// (GET_PARAMETER is the other one). This is only safe to call when no
    /// interleaved data is being transferred.
    pub fn keepalive(&mut self) -> Result<Response> {
        let url = self.base_url.clone();
        self.request("OPTIONS", &url, &[])?.ok()
    }
}

/// Turn "rtsp://127.0.0.1:8554/test" into "127.0.0.1:8554". When the URL states no
/// port, 554 is used, because that is the default port for RTSP.
fn authority_of(url: &str) -> Result<String> {
    let Some(rest) = url.strip_prefix("rtsp://") else {
        return protocol(format!("url must start with rtsp:// : {url}"));
    };
    let authority = rest.split('/').next().unwrap_or(rest);
    if authority.is_empty() {
        return protocol(format!("url has no host: {url}"));
    }
    Ok(if authority.contains(':') {
        authority.to_string()
    } else {
        format!("{authority}:554")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authority_parsing() {
        assert_eq!(authority_of("rtsp://127.0.0.1:8554/test").unwrap(), "127.0.0.1:8554");
        assert_eq!(authority_of("rtsp://cam.local/stream").unwrap(), "cam.local:554");
        assert!(authority_of("http://x/y").is_err());
    }
}

#[cfg(test)]
mod handshake_tests {
    //! Runs the complete handshake against a mock RTSP server, so that the session
    //! logic can be checked without mediamtx or ffmpeg running.

    use super::*;
    use std::io::{BufRead, BufReader as IoBufReader};
    use std::net::TcpListener;
    use std::thread;

    const SDP: &str = "v=0\r\n\
        o=- 0 0 IN IP4 127.0.0.1\r\n\
        s=Stream\r\n\
        t=0 0\r\n\
        a=control:*\r\n\
        m=video 0 RTP/AVP 96\r\n\
        a=rtpmap:96 H264/90000\r\n\
        a=fmtp:96 sprop-parameter-sets=Z0IAKeKQCgC3YC3AWA==,aM48gA==\r\n\
        a=control:trackID=0\r\n";

    /// Answer one handshake and return every request that arrived, exactly as it
    /// was received.
    fn mock_server() -> (String, thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        let handle = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = IoBufReader::new(stream.try_clone().unwrap());
            let mut writer = stream;
            let mut seen = Vec::new();

            loop {
                let mut request = String::new();
                let mut method = String::new();
                let mut cseq = String::new();

                // Read one request: the request line, the headers, the empty line.
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap() == 0 {
                        return seen;
                    }
                    request.push_str(&line);
                    let trimmed = line.trim_end();
                    if method.is_empty() {
                        method = trimmed.split(' ').next().unwrap_or("").to_string();
                    } else if let Some(v) = trimmed.strip_prefix("CSeq:") {
                        cseq = v.trim().to_string();
                    }
                    if trimmed.is_empty() {
                        break;
                    }
                }
                seen.push(request);

                let response = match method.as_str() {
                    "OPTIONS" => format!(
                        "RTSP/1.0 200 OK\r\nCSeq: {cseq}\r\n\
                         Public: DESCRIBE, SETUP, PLAY, TEARDOWN\r\n\r\n"
                    ),
                    "DESCRIBE" => format!(
                        "RTSP/1.0 200 OK\r\nCSeq: {cseq}\r\n\
                         Content-Type: application/sdp\r\n\
                         Content-Length: {}\r\n\r\n{SDP}",
                        SDP.len()
                    ),
                    "SETUP" => format!(
                        "RTSP/1.0 200 OK\r\nCSeq: {cseq}\r\n\
                         Transport: RTP/AVP/TCP;unicast;interleaved=0-1\r\n\
                         Session: 12345678;timeout=60\r\n\r\n"
                    ),
                    "PLAY" => format!(
                        "RTSP/1.0 200 OK\r\nCSeq: {cseq}\r\nSession: 12345678\r\n\
                         RTP-Info: url=rtsp://x/test/trackID=0;seq=9810;rtptime=345\r\n\r\n"
                    ),
                    "TEARDOWN" => format!("RTSP/1.0 200 OK\r\nCSeq: {cseq}\r\n\r\n"),
                    _ => format!("RTSP/1.0 501 Not Implemented\r\nCSeq: {cseq}\r\n\r\n"),
                };

                writer.write_all(response.as_bytes()).unwrap();
                if method == "TEARDOWN" {
                    return seen;
                }
            }
        });

        (format!("rtsp://127.0.0.1:{port}/test"), handle)
    }

    #[test]
    fn full_handshake() {
        let (url, server) = mock_server();
        let mut session = Session::connect(&url).unwrap();

        let options = session.options().unwrap();
        assert!(options.header("Public").unwrap().contains("SETUP"));
        assert_eq!(session.state(), State::Init);

        let sdp = session.describe().unwrap();
        let video = sdp.video().unwrap();
        assert_eq!(video.payload_type, 96);
        assert_eq!(video.control.as_deref(), Some("trackID=0"));
        assert!(video.sps.is_some() && video.pps.is_some());

        let track = crate::sdp::resolve_control(session.base_url(), "trackID=0");
        session.setup(&track).unwrap();
        assert_eq!(session.session_id(), Some("12345678")); // ";timeout=60" removed
        assert_eq!(session.state(), State::Ready);

        let play = session.play().unwrap();
        assert!(play.header("RTP-Info").unwrap().contains("seq=9810"));
        assert_eq!(session.state(), State::Playing);

        session.teardown().unwrap();
        assert_eq!(session.state(), State::Init);
        assert_eq!(session.session_id(), None);

        // Now check what was really sent over the connection.
        let requests = server.join().unwrap();
        let methods: Vec<&str> = requests
            .iter()
            .map(|r| r.split(' ').next().unwrap())
            .collect();
        assert_eq!(methods, ["OPTIONS", "DESCRIBE", "SETUP", "PLAY", "TEARDOWN"]);

        // CSeq starts at 1 and increases by one for every request.
        for (i, r) in requests.iter().enumerate() {
            assert!(
                r.contains(&format!("CSeq: {}\r\n", i + 1)),
                "request {i} had the wrong CSeq:\n{r}"
            );
            assert!(r.ends_with("\r\n\r\n"), "request {i} was not terminated once");
        }

        // There is no Session header until SETUP has answered. After that, every
        // request carries it.
        assert!(!requests[0].contains("Session:"));
        assert!(!requests[2].contains("Session:")); // the SETUP request itself
        assert!(requests[3].contains("Session: 12345678\r\n")); // PLAY
        assert!(requests[4].contains("Session: 12345678\r\n")); // TEARDOWN

        // SETUP is sent to the track URL, and PLAY to the aggregate URL.
        assert!(requests[2].starts_with("SETUP rtsp://127.0.0.1:"));
        assert!(requests[2].lines().next().unwrap().contains("/test/trackID=0"));
        assert!(requests[3].lines().next().unwrap().ends_with("/test RTSP/1.0"));
    }

    #[test]
    fn play_before_setup_is_rejected_locally() {
        let (url, _server) = mock_server();
        let mut session = Session::connect(&url).unwrap();
        // SETUP has not run, so there is no Session id. The client detects this by
        // itself, instead of sending a request that the server would answer with
        // 454 Session Not Found.
        assert!(session.play().is_err());
    }
}
