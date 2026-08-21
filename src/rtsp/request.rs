// Building RTSP requests (RFC 2326 section 6)
//
// The whole point of this module is that the mandatory parts are GENERATED and only
// the variable parts are passed in. The line terminator exists in exactly one place
// below, so a framing bug can only ever live in one line of code.
//
//   OPTIONS rtsp://127.0.0.1:8554/test RTSP/1.0\r\n
//   CSeq: 1\r\n
//   \r\n

/// Assemble one request. `headers` carries only what this method needs
/// (Accept, Transport, Range, ...); CSeq is added here, Session by the caller.
pub fn build(method: &str, url: &str, cseq: u32, headers: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(128);
    out.push_str(method);
    out.push(' ');
    out.push_str(url);
    out.push_str(" RTSP/1.0\r\n");

    out.push_str("CSeq: ");
    out.push_str(&cseq.to_string());
    out.push_str("\r\n");

    for (name, value) in headers {
        out.push_str(name);
        out.push_str(": ");
        out.push_str(value);
        out.push_str("\r\n");
    }

    out.push_str("\r\n"); // the blank line that ends the headers
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_request_has_exact_bytes() {
        let r = build("OPTIONS", "rtsp://127.0.0.1:8554/test", 1, &[]);
        assert_eq!(r, "OPTIONS rtsp://127.0.0.1:8554/test RTSP/1.0\r\nCSeq: 1\r\n\r\n");
    }

    #[test]
    fn nothing_follows_the_blank_line() {
        // The trailing-bytes bug: anything after \r\n\r\n gets parsed as the
        // beginning of the NEXT request on the same connection.
        let r = build("DESCRIBE", "rtsp://h/s", 2, &[("Accept", "application/sdp")]);
        assert!(r.ends_with("\r\n\r\n"));
        assert!(!r.ends_with("\r\n\r\n\r\n"));
    }

    #[test]
    fn headers_appear_in_order_after_cseq() {
        let r = build("SETUP", "rtsp://h/s/trackID=0", 3, &[
            ("Transport", "RTP/AVP/TCP;unicast;interleaved=0-1"),
            ("Session", "12345678"),
        ]);
        assert_eq!(
            r,
            "SETUP rtsp://h/s/trackID=0 RTSP/1.0\r\n\
             CSeq: 3\r\n\
             Transport: RTP/AVP/TCP;unicast;interleaved=0-1\r\n\
             Session: 12345678\r\n\
             \r\n"
        );
    }
}
