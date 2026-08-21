// TCP interleaved framing (RFC 2326 section 10.12)  --  YOUR CODE
//
// After PLAY, media arrives on the SAME connection as the RTSP text, wrapped in a
// 4 byte header:
//
//   +---------+---------+-----------------+---------------------------+
//   | '$'0x24 | channel | length (u16 BE) | payload (length bytes)    |
//   +---------+---------+-----------------+---------------------------+
//
// We negotiated interleaved=0-1 in SETUP, so channel 0 is RTP and channel 1 is RTCP.
//
// Implementation notes:
// - read_exact, never read(). The length is stated, so partial reads are just a bug
//   waiting to happen.
// - The length is big endian. u16::from_be_bytes([hi, lo]) - no crate needed.
// - If the first byte is not '$', the server sent an RTSP response instead (a reply
//   to a keepalive, or an announcement). Return an error for now; handling that
//   properly means peeking and switching between the text and binary parsers.
//
// Run `cargo test transport` while you work. All tests below should go green.

#[allow(unused_imports)] // Read is needed once you call read_exact
use std::io::{BufRead, Read};

#[allow(unused_imports)]
use crate::error::{protocol, Result};

pub const MAGIC: u8 = b'$';
pub const CHANNEL_RTP: u8 = 0;
#[allow(dead_code)] // used once you handle RTCP
pub const CHANNEL_RTCP: u8 = 1;

#[derive(Debug, PartialEq, Eq)]
pub struct Frame {
    pub channel: u8,
    pub payload: Vec<u8>,
}

/// Read exactly one interleaved frame.
pub fn read_frame<R: BufRead>(reader: &mut R) -> Result<Frame> {
    let _ = (reader, MAGIC);
    todo!("week 3: read the 4 byte header, then read_exact the payload")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn reads_one_rtp_frame() {
        //            $     ch0   len = 4          payload
        let bytes = [0x24, 0x00, 0x00, 0x04, 0xDE, 0xAD, 0xBE, 0xEF];
        let frame = read_frame(&mut Cursor::new(&bytes[..])).unwrap();
        assert_eq!(frame.channel, CHANNEL_RTP);
        assert_eq!(frame.payload, vec![0xDE, 0xAD, 0xBE, 0xEF]);
    }

    #[test]
    fn length_is_big_endian() {
        // 0x0102 = 258, not 0x0201 = 513
        let mut bytes = vec![0x24, 0x00, 0x01, 0x02];
        bytes.extend(std::iter::repeat(0xAB).take(258));
        let frame = read_frame(&mut Cursor::new(&bytes[..])).unwrap();
        assert_eq!(frame.payload.len(), 258);
    }

    #[test]
    fn consecutive_frames_do_not_bleed_into_each_other() {
        let bytes = [
            0x24, 0x00, 0x00, 0x02, 0x11, 0x22, // RTP frame
            0x24, 0x01, 0x00, 0x03, 0x33, 0x44, 0x55, // RTCP frame
        ];
        let mut cursor = Cursor::new(&bytes[..]);

        let first = read_frame(&mut cursor).unwrap();
        assert_eq!(first.channel, CHANNEL_RTP);
        assert_eq!(first.payload, vec![0x11, 0x22]);

        let second = read_frame(&mut cursor).unwrap();
        assert_eq!(second.channel, CHANNEL_RTCP);
        assert_eq!(second.payload, vec![0x33, 0x44, 0x55]);
    }

    #[test]
    fn zero_length_frame_is_legal() {
        let bytes = [0x24, 0x00, 0x00, 0x00];
        let frame = read_frame(&mut Cursor::new(&bytes[..])).unwrap();
        assert!(frame.payload.is_empty());
    }

    #[test]
    fn rejects_a_non_dollar_first_byte() {
        // This is what an RTSP response looks like arriving where a frame was expected.
        let bytes = b"RTSP/1.0 200 OK\r\n\r\n";
        assert!(read_frame(&mut Cursor::new(&bytes[..])).is_err());
    }

    #[test]
    fn truncated_payload_is_an_error_not_a_short_frame() {
        // Header claims 8 bytes, only 3 follow. read() would silently return 3.
        let bytes = [0x24, 0x00, 0x00, 0x08, 0x01, 0x02, 0x03];
        assert!(read_frame(&mut Cursor::new(&bytes[..])).is_err());
    }
}
