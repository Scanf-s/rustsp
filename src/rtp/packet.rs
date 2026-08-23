// RTP header parsing (RFC 3550 section 5.1)  --  YOUR CODE
//
//  0                   1                   2                   3
//  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
// |V=2|P|X|  CC   |M|     PT      |       sequence number         |
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
// |                           timestamp                           |
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
// |                             SSRC                              |
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//
// Byte 0:  V (2 bits) | P (1) | X (1) | CC (4)
// Byte 1:  M (1 bit)  | PT (7)
// Byte 2, 3: sequence number
// Byte 4, 5, 6, 7: timestamp
// Byte 8, 9, 10, 11: SSRC
//
// Extract the bit fields yourself, with masks and shifts:
//   let version = (b[0] >> 6) & 0b11;
//   let marker  = (b[1] >> 7) & 0b1 == 1;
//   let pt      = b[1] & 0b0111_1111;
//
// The payload begins after:
//   the 12 byte fixed header
//   + 4 * CC bytes    (the contributing source list, which is usually empty)
//   + if X is set: 4 bytes + 4 * (the extension length in 32 bit words, which is
//                  stored in bytes 2..4 of the extension header)
//
// Fields that are longer than one byte are big endian, so use u16::from_be_bytes or
// u32::from_be_bytes.
//
// Return an error if the version is not 2, or if the packet is shorter than the
// header describes.
//
// Run `cargo test rtp::packet` while you work.

use crate::error::{protocol, Result};

#[derive(Debug, PartialEq, Eq)]
pub struct RtpPacket {
    pub marker: bool,
    pub payload_type: u8,
    pub sequence_number: u16,
    pub timestamp: u32,
    pub ssrc: u32,
    pub payload: Vec<u8>,
}

// parse a given RTP packet to RtpPacket struct.
// If invalid, it returns an error
pub fn parse(bytes: &[u8]) -> Result<RtpPacket> {

    // Check bytes are empty
    if bytes.len() == 0 {
        return protocol("empty bytes are given");
    }

    // RTP version
    let version = (bytes[0] >> 6) & 0b11; // 2bits
    if version != 0b10 {
        // Only supports version 2 (RTC 3550: https://www.rfc-editor.org/info/rfc3550/#section-5.1)
        return protocol(format!(
            "expected 'version 2' on RTP packet, got {:#04x}",
            version
        ));
    }
    let extension = (bytes[0] >> 4) &0b1; // 1 bit
    let cc: u8 = bytes[0] & 0b0000_1111; // 4 bits
    
    // Check if this packet contains adequate bytes regarding from cc
    let min_packet_bytes = 12 + cc as usize * 4;
    if bytes.len() < min_packet_bytes {
        return protocol(format!(
            "invalid packet data given, it needs {:#04x} bytes minimun but got {:#04x}",
            min_packet_bytes,
            bytes.len()
        ));
    }

    let marker = ((bytes[1] >> 7) & 0b1) != 0; // 1bit
    let payload_type = bytes[1] & 0b0111_1111; // 7 bit
    let sequence_number = u16::from_be_bytes([bytes[2], bytes[3]]);
    let timestamp = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    let ssrc = u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);

    // calculate payload location
    // 0                   1                   2                   3
    // 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
    // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    // |      defined by profile       |           length              |
    // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    // |                        header extension                       |
    // |                             ....                              |
    // The header extension contains a 16-bit length field that counts the number of 32-bit words in the extension
    // if length represents 0x02 -> a length of header extension is 2 * 4bytes = 8bytes

    // cc = number of data sources
    // each CSRC has 4 bytes
    // standard header 12 bytes + number of data sources * CSRC size (4 bytes)
    let mut offset = 12 + cc as usize * 4;
    if extension == 0b1 {
        // if extension is set, RTP extension header must be appended to the RTP header (after the offset bytes)
        // read extension length
        if bytes.len() < offset + 4 {
            return protocol(
                format!(
                    "extension header needs 4 bytes at offset {}, but the packet has {} bytes",
                    offset,
                    bytes.len()
                )
            );
        }
        let extension_length = u16::from_be_bytes([bytes[offset + 2], bytes[offset + 3]]);

        // skip profile 2 bytes + length 2 bytes + header extension (extension_length) * 4 bytes
        offset += 4 + extension_length as usize * 4;
    }

    if bytes.len() < offset {
        return protocol(
            format!(
                "to get payload from the given bytes, need more than {} bytes, but the packet has only {} bytes",
                offset,
                bytes.len()
            )
        );
    }
    let payload = bytes[offset..].to_vec();

    // Build prased RTPPacket
    let packet = RtpPacket{
        marker: marker,
        payload_type: payload_type,
        sequence_number: sequence_number,
        timestamp: timestamp,
        ssrc: ssrc,
        payload: payload,
    };

    Ok(packet)
}

#[cfg(test)]
mod tests {
    use super::*;

    // V=2, P=0, X=0, CC=0 -> 0x80
    // M=1, PT=96          -> 0x80 | 0x60 = 0xE0
    const PLAIN: &[u8] = &[
        0x80, 0xE0, 0x12, 0x34, // version/flags, marker/pt, seq = 0x1234
        0x11, 0x22, 0x33, 0x44, // timestamp
        0xAA, 0xBB, 0xCC, 0xDD, // ssrc
        0x61, 0x00, 0x01, // payload
    ];

    #[test]
    fn parses_a_plain_packet() {
        let p = parse(PLAIN).unwrap();
        assert!(p.marker);
        assert_eq!(p.payload_type, 96);
        assert_eq!(p.sequence_number, 0x1234);
        assert_eq!(p.timestamp, 0x1122_3344);
        assert_eq!(p.ssrc, 0xAABB_CCDD);
        assert_eq!(p.payload, vec![0x61, 0x00, 0x01]);
    }

    #[test]
    fn marker_bit_is_separate_from_payload_type() {
        // The payload type is still 96, but the marker bit is 0, so byte 1 holds
        // 0x60 instead of 0xE0.
        let mut bytes = PLAIN.to_vec();
        bytes[1] = 0x60;
        let p = parse(&bytes).unwrap();
        assert!(!p.marker);
        assert_eq!(p.payload_type, 96);
    }

    #[test]
    fn csrc_list_shifts_the_payload() {
        // CC = 2 makes byte 0 equal to 0x82, and 8 extra bytes come before the
        // payload.
        let bytes = [
            0x82, 0xE0, 0x00, 0x01, //
            0x00, 0x00, 0x00, 0x00, // timestamp
            0x00, 0x00, 0x00, 0x01, // ssrc
            0xC0, 0xC0, 0xC0, 0xC0, // csrc 1
            0xC1, 0xC1, 0xC1, 0xC1, // csrc 2
            0x41, 0x99, // payload
        ];
        let p = parse(&bytes).unwrap();
        assert_eq!(p.payload, vec![0x41, 0x99]);
    }

    #[test]
    fn extension_header_shifts_the_payload() {
        // X = 1 makes byte 0 equal to 0x90. The extension header holds 2 bytes of
        // profile, then 2 bytes of length counted in 32 bit words (here 1), and
        // after that 4 bytes of extension data.
        let bytes = [
            0x90, 0xE0, 0x00, 0x01, //
            0x00, 0x00, 0x00, 0x00, // timestamp
            0x00, 0x00, 0x00, 0x01, // ssrc
            0xBE, 0xDE, 0x00, 0x01, // ext profile, ext length = 1 word
            0xEE, 0xEE, 0xEE, 0xEE, // one word of extension data
            0x41, 0x99, // payload
        ];
        let p = parse(&bytes).unwrap();
        assert_eq!(p.payload, vec![0x41, 0x99]);
    }

    #[test]
    fn rejects_wrong_version() {
        let mut bytes = PLAIN.to_vec();
        bytes[0] = 0x40; // version 1
        assert!(parse(&bytes).is_err());
    }

    #[test]
    fn rejects_a_header_that_is_too_short() {
        assert!(parse(&PLAIN[..8]).is_err());
    }

    #[test]
    fn rejects_a_csrc_count_the_packet_is_too_short_for() {
        // The header announces CC = 15, which means 60 extra bytes, 
        // inside a packet of 15 bytes.
        let mut bytes = PLAIN.to_vec();
        bytes[0] = 0x8F; // 1000 1111
        assert!(parse(&bytes).is_err());
    }

    #[test]
    fn empty_payload_is_legal() {
        let p = parse(&PLAIN[..12]).unwrap();
        assert!(p.payload.is_empty());
    }
}
