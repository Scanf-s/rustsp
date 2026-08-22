// TCP interleaved framing (RFC 2326 section 10.12)  --  YOUR CODE
//
// After PLAY, the media arrives on the same connection as the RTSP text, behind a
// 4 byte header:
//
//   +---------+---------+-----------------+---------------------------+
//   | '$'0x24 | channel | length (u16 BE) | payload (length bytes)    |
//   +---------+---------+-----------------+---------------------------+
//
// SETUP asked for interleaved=0-1, so channel 0 carries RTP and channel 1 carries
// RTCP.
//
// Notes for the implementation:
// - Use read_exact and never read(). The header states the length, so a partial read
//   can only hide a bug.
// - The length is big endian. u16::from_be_bytes([hi, lo]) is enough, and no crate
//   is needed for it.
// - If the first byte is not '$', the server sent an RTSP response instead, for
//   example an answer to a keepalive or an announcement. Return an error for now. A
//   complete solution would look at the next byte without consuming it, and then
//   choose between the text parser and the binary parser.
//
// Run `cargo test transport` while you work on this. Every test below should pass.

// A bound of `R: BufRead` already brings the methods of Read into scope, because
// BufRead has Read as a supertrait. So read_exact needs no import of its own here.
use std::io::BufRead;

use crate::error::{protocol, Result};

pub const MAGIC: u8 = b'$'; // 0x24 (says that the bytes which follow are binary, not text)
pub const CHANNEL_RTP: u8 = 0; // negotiated RTP channel no.
#[allow(dead_code)]
pub const CHANNEL_RTCP: u8 = 1; // negotiated RTP session control channel no.

#[derive(Debug, PartialEq, Eq)]
pub struct Frame {
    pub channel: u8,
    pub payload: Vec<u8>,
}

/// Read exactly one interleaved frame.
/// Every frame is returned, whatever its channel, 
/// so the caller decides which channels it wants to keep.
pub fn read_frame<R: BufRead>(reader: &mut R) -> Result<Frame> {
    // read the whole 4 byte header at once
    let mut header_buffer = [0; 4];
    reader.read_exact(&mut header_buffer)?;

    // Check first byte contains MAGIC -> else return an error
    if header_buffer[0] != MAGIC {
        return protocol(format!(
            "expected '$' at the start of an interleaved frame, got {:#04x}",
            header_buffer[0]
        ));
    }

    // Get channel id from second byte
    let channel = header_buffer[1];

    // Read payload length (big endian) from third and fourth byte
    let payload_length = u16::from_be_bytes([header_buffer[2], header_buffer[3]]) as usize;

    // allocate a buffer of exactly the announced length
    let mut payload_buf = vec![0; payload_length];
    reader.read_exact(&mut payload_buf)?;

    // Build Frame and return it
    let frame = Frame {
        channel,
        payload: payload_buf,
    };
    Ok(frame)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufReader, Cursor};

    #[test]
    fn reads_one_rtp_frame() {
        //                     $   | ch0 | len = 4  |        payload
        let bytes = [0x24, 0x00, 0x00, 0x04, 0xDE, 0xAD, 0xBE, 0xEF];
        let frame = read_frame(&mut Cursor::new(&bytes[..])).unwrap();
        assert_eq!(frame.channel, CHANNEL_RTP);
        assert_eq!(frame.payload, vec![0xDE, 0xAD, 0xBE, 0xEF]);
    }

    #[test]
    fn length_is_big_endian() {
        // 0x0102 = 258, not 0x0201 = 513
        let mut bytes = vec![0x24, 0x00, 0x01, 0x02];
        bytes.extend(std::iter::repeat_n(0xAB, 258));
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
        // This is an RTSP response arriving where a frame was expected.
        let bytes = b"RTSP/1.0 200 OK\r\n\r\n";
        assert!(read_frame(&mut Cursor::new(&bytes[..])).is_err());
    }

    #[test]
    fn truncated_payload_is_an_error_not_a_short_frame() {
        // The header announces 8 bytes, but only 3 follow. read() would return
        // those 3 bytes without reporting anything.
        let bytes = [0x24, 0x00, 0x00, 0x08, 0x01, 0x02, 0x03];
        assert!(read_frame(&mut Cursor::new(&bytes[..])).is_err());
    }

    #[test]
    fn a_truncated_header_is_an_error_not_an_empty_frame() {
        // Only 3 of the 4 header bytes arrive, so read_exact has to fail. The test
        // wraps the data in a BufReader on purpose, because Cursor and BufReader
        // behave differently once read_exact fails. Cursor writes nothing into the
        // buffer, while BufReader keeps the bytes it did manage to read. So with a
        // BufReader the buffer holds [24, 00, 00, 00], and code that ignored the
        // error would see a valid '$', channel 0 and length 0, and would report an
        // empty frame instead of a broken connection.
        let bytes = [0x24, 0x00, 0x00];
        let mut reader = BufReader::new(Cursor::new(&bytes[..]));
        assert!(read_frame(&mut reader).is_err());
    }
}
