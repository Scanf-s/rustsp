// SDP parsing (RFC 8866; the H.264 specific attributes are RFC 6184 section 8)
//
// An SDP document is a flat list of "key=value" lines. Every line after an "m=" line
// belongs to that media section, until the next "m=" line begins. This client needs
// four pieces of information:
//
//   m=video 0 RTP/AVP 96              -> the media kind and the payload type. The
//                                        port is only a placeholder, because the
//                                        real one is negotiated in SETUP.
//   a=control:trackID=0               -> the URL that SETUP has to be sent to
//   a=rtpmap:96 H264/90000            -> the meaning of dynamic payload type 96
//   a=fmtp:96 sprop-parameter-sets=Z0IAKY..,aM48gA==
//                                     -> SPS and PPS in base64, which a decoder
//                                        needs before it can start

use crate::error::{Result, protocol};

#[derive(Debug, Default)]
pub struct Media {
    pub kind: String,
    pub payload_type: u8,
    pub control: Option<String>,
    pub encoding: Option<String>,
    pub clock_rate: Option<u32>,
    pub sps: Option<Vec<u8>>,
    pub pps: Option<Vec<u8>>,
}

#[derive(Debug, Default)]
pub struct Sdp {
    pub session_control: Option<String>,
    pub media: Vec<Media>,
}

impl Sdp {
    pub fn video(&self) -> Option<&Media> {
        self.media.iter().find(|m| m.kind == "video")
    }
}

pub fn parse(text: &str) -> Result<Sdp> {
    let mut sdp = Sdp::default();

    for raw in text.lines() {
        let line = raw.trim_end_matches('\r');
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };

        match key {
            // m=<media> <port> <proto> <fmt ...>
            "m" => {
                let mut f = value.split_whitespace();
                let kind = f.next().unwrap_or_default().to_string();
                let _port = f.next();
                let _proto = f.next();
                let payload_type = f.next().and_then(|p| p.parse().ok()).unwrap_or(0);
                sdp.media.push(Media {
                    kind,
                    payload_type,
                    ..Default::default()
                });
            }
            "a" => {
                let (name, rest) = match value.split_once(':') {
                    Some((n, r)) => (n, r),
                    None => (value, ""),
                };
                match name {
                    "control" => match sdp.media.last_mut() {
                        // As long as no m= line has appeared, a=control describes
                        // the session and gives the aggregate URL. After an m= line
                        // it describes that one track.
                        Some(m) => m.control = Some(rest.to_string()),
                        None => sdp.session_control = Some(rest.to_string()),
                    },
                    "rtpmap" => {
                        // rtpmap:96 H264/90000
                        if let Some(m) = sdp.media.last_mut()
                            && let Some((_pt, enc)) = rest.split_once(' ')
                        {
                            let mut parts = enc.split('/');
                            m.encoding = parts.next().map(|s| s.to_string());
                            m.clock_rate = parts.next().and_then(|s| s.parse().ok());
                        }
                    }
                    "fmtp" => {
                        // fmtp:<pt> <param>;<param>...
                        // The payload type comes first and is not a parameter, so
                        // it has to be removed before the rest is split on ';'. If
                        // it stays, the parameter that is listed first remains
                        // attached to it and is never recognized.
                        if let Some(m) = sdp.media.last_mut() {
                            let params = rest.split_once(' ').map_or("", |(_pt, p)| p);
                            for param in params.split(';') {
                                let param = param.trim();
                                if let Some(sets) = param.strip_prefix("sprop-parameter-sets=") {
                                    let mut it = sets.split(',');
                                    m.sps = it.next().and_then(base64_decode);
                                    m.pps = it.next().and_then(base64_decode);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    if sdp.media.is_empty() {
        return protocol("SDP contained no m= line");
    }
    Ok(sdp)
}

/// Build the full track URL from an `a=control` value and the presentation URL.
/// The value can be absolute ("rtsp://host/stream/trackID=0"), a short relative
/// token ("trackID=0"), or "*", which means the presentation URL itself.
pub fn resolve_control(base: &str, control: &str) -> String {
    if control.starts_with("rtsp://") {
        return control.to_string();
    }
    if control == "*" || control.is_empty() {
        return base.to_string();
    }
    format!(
        "{}/{}",
        base.trim_end_matches('/'),
        control.trim_start_matches('/')
    )
}

/// A very small base64 decoder. It is here only so that the project needs no
/// dependencies. There is nothing about RTSP to learn from it.
fn base64_decode(input: &str) -> Option<Vec<u8>> {
    fn sextet(c: u8) -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => (c - b'A') as u32,
            b'a'..=b'z' => (c - b'a') as u32 + 26,
            b'0'..=b'9' => (c - b'0') as u32 + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        })
    }

    let mut out = Vec::new();
    let mut acc: u32 = 0;
    let mut bits = 0u32;
    for byte in input.bytes() {
        if byte == b'=' {
            break;
        }
        if byte.is_ascii_whitespace() {
            continue;
        }
        acc = (acc << 6) | sextet(byte)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    // This is the SDP that mediamtx really sends for an H.264 stream.
    const MEDIAMTX_SDP: &str = "v=0\r\n\
        o=- 0 0 IN IP4 127.0.0.1\r\n\
        s=Stream\r\n\
        c=IN IP4 0.0.0.0\r\n\
        t=0 0\r\n\
        a=control:*\r\n\
        m=video 0 RTP/AVP 96\r\n\
        a=rtpmap:96 H264/90000\r\n\
        a=fmtp:96 packetization-mode=1; sprop-parameter-sets=Z0IAKeKQCgC3YC3AWA==,aM48gA==\r\n\
        a=control:trackID=0\r\n";

    #[test]
    fn finds_the_video_track() {
        let sdp = parse(MEDIAMTX_SDP).unwrap();
        let v = sdp.video().expect("a video track");
        assert_eq!(v.payload_type, 96);
        assert_eq!(v.encoding.as_deref(), Some("H264"));
        assert_eq!(v.clock_rate, Some(90_000));
        assert_eq!(v.control.as_deref(), Some("trackID=0"));
    }

    #[test]
    fn extracts_sps_and_pps() {
        let sdp = parse(MEDIAMTX_SDP).unwrap();
        let v = sdp.video().unwrap();
        // The NAL type sits in the lowest 5 bits: 7 means SPS, 8 means PPS.
        assert_eq!(v.sps.as_ref().unwrap()[0] & 0x1F, 7);
        assert_eq!(v.pps.as_ref().unwrap()[0] & 0x1F, 8);
    }

    #[test]
    fn session_control_is_separate_from_track_control() {
        let sdp = parse(MEDIAMTX_SDP).unwrap();
        assert_eq!(sdp.session_control.as_deref(), Some("*"));
        assert_eq!(sdp.video().unwrap().control.as_deref(), Some("trackID=0"));
    }

    #[test]
    fn sprop_is_found_even_as_the_first_fmtp_parameter() {
        // The payload type ("96 ") comes before the first parameter, so a simple
        // split on ';' leaves it attached to the parameter that is listed first.
        let sdp = parse(
            "m=video 0 RTP/AVP 96\r\n\
             a=fmtp:96 sprop-parameter-sets=Z0IAKeKQCgC3YC3AWA==,aM48gA==\r\n",
        )
        .unwrap();
        let v = sdp.video().unwrap();
        assert_eq!(v.sps.as_ref().unwrap()[0] & 0x1F, 7);
        assert_eq!(v.pps.as_ref().unwrap()[0] & 0x1F, 8);
    }

    #[test]
    fn control_urls_resolve() {
        let base = "rtsp://127.0.0.1:8554/test";
        assert_eq!(
            resolve_control(base, "trackID=0"),
            "rtsp://127.0.0.1:8554/test/trackID=0"
        );
        assert_eq!(resolve_control(base, "*"), base);
        assert_eq!(resolve_control(base, "rtsp://other/x"), "rtsp://other/x");
    }
}
