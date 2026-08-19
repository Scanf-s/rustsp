// RTP header parsing (RFC 3550, section 5.1)
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
// What to build: parse the fixed 12-byte header, then work out where the
// payload starts.
// - Reject the packet if version != 2
// - If CC (CSRC count) > 0, the payload starts at 12 + 4*CC
// - If the X (extension) bit is set, skip the extension header as well
// - Read u16/u32 fields with u16::from_be_bytes; no crate needed
//
// Start with the payload as an owned Vec<u8>. Once everything works,
// refactor it to a borrowed &[u8] with a lifetime and see what follows.
