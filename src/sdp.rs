// SDP parsing (RFC 8866; the H.264 specific attributes are RFC 6184 section 8)
//
// SDP is a flat list of "key=value" lines. Everything after an "m=" line belongs to
// that media section until the next "m=" line. We only need four things:
//
//   m=video 0 RTP/AVP 96              -> media kind and payload type (port is a
//                                        placeholder; the real one comes from SETUP)
//   a=control:trackID=0               -> the URL to send SETUP to
//   a=rtpmap:96 H264/90000            -> what the dynamic payload type 96 means
//   a=fmtp:96 sprop-parameter-sets=Z0IAKY..,aM48gA==
//                                     -> base64 SPS and PPS, needed to start a decoder

use crate::error::{protocol, Result};

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
        let Some((key, value)) = line.split_once('=') else { continue };

        match key {
            // m=<media> <port> <proto> <fmt ...>
            "m" => {
                let mut f = value.split_whitespace();
                let kind = f.next().unwrap_or_default().to_string();
                let _port = f.next();
                let _proto = f.next();
                let payload_type = f.next().and_then(|p| p.parse().ok()).unwrap_or(0);
                sdp.media.push(Media { kind, payload_type, ..Default::default() });
            }
            "a" => {
                let (name, rest) = match value.split_once(':') {
                    Some((n, r)) => (n, r),
                    None => (value, ""),
                };
                match name {
                    "control" => match sdp.media.last_mut() {
                        // Before any m= line, a=control belongs to the session
                        // (the aggregate URL). After one, it names that track.
                        Some(m) => m.control = Some(rest.to_string()),
                        None => sdp.session_control = Some(rest.to_string()),
                    },
                    "rtpmap" => {
                        // rtpmap:96 H264/90000
                        if let Some(m) = sdp.media.last_mut() {
                            if let Some((_pt, enc)) = rest.split_once(' ') {
                                let mut parts = enc.split('/');
                                m.encoding = parts.next().map(|s| s.to_string());
                                m.clock_rate = parts.next().and_then(|s| s.parse().ok());
                            }
                        }
                    }
                    "fmtp" => {
                        // fmtp:<pt> <param>;<param>...
                        // The payload type comes first and is not a parameter, so
                        // strip it before splitting on ';'. Miss this and any
                        // parameter that happens to be listed first is invisible.
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

/// Resolve a track's `a=control` value against the presentation URL.
/// It may be absolute ("rtsp://host/stream/trackID=0"), a bare relative token
/// ("trackID=0"), or "*" meaning "the presentation URL itself".
pub fn resolve_control(base: &str, control: &str) -> String {
    if control.starts_with("rtsp://") {
        return control.to_string();
    }
    if control == "*" || control.is_empty() {
        return base.to_string();
    }
    format!("{}/{}", base.trim_end_matches('/'), control.trim_start_matches('/'))
}

/// Minimal base64 decoder. Only here so the project stays dependency free;
/// there is nothing to learn about RTSP in it.
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

    // What mediamtx actually sends for an H.264 stream.
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
        // NAL type is the low 5 bits: 7 = SPS, 8 = PPS.
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
        // The payload type ("96 ") sits before the first parameter, so a naive
        // split on ';' leaves it glued to whatever comes first.
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
        assert_eq!(resolve_control(base, "trackID=0"), "rtsp://127.0.0.1:8554/test/trackID=0");
        assert_eq!(resolve_control(base, "*"), base);
        assert_eq!(resolve_control(base, "rtsp://other/x"), "rtsp://other/x");
    }
}
