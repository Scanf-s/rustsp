// TCP interleaved framing (RFC 2326, section 10.12)
//
// After PLAY, binary frames arrive on the same socket in this shape:
//
//   +---------+---------+-----------------+---------------------------+
//   | '$'0x24 | channel | length (u16 BE) | payload (length bytes)    |
//   +---------+---------+-----------------+---------------------------+
//
// channel: if you negotiated interleaved=0-1 in SETUP, then 0 = RTP and 1 = RTCP.
//
// What to build: a function that reads one frame with read_exact and
// returns (channel, Vec<u8>).
//
// Watch out: if the first byte is not '$', it may be an RTSP response the
// server sent on its own (e.g. a reply to a keepalive). Treat it as an error
// for now and add proper handling later.
