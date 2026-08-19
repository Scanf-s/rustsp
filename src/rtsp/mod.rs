// RTSP session (RFC 2326)
//
// What to build here:
// - A session state machine: Init -> Ready (after SETUP) -> Playing (after PLAY)
// - A CSeq counter that goes up by 1 on every request
// - Storage for the Session header (taken from the SETUP response and
//   attached to every request after that)
//
// Watch out: create exactly ONE BufReader around the TcpStream and keep it
// for the whole session. Headers are read with read_line and interleaved RTP
// with read_exact, but both must go through the same BufReader. If you read
// from the raw stream after buffering, the bytes left in the buffer are lost.

pub mod request;
pub mod response;
// pub mod auth; // add this when you connect to a camera that needs Digest auth (RFC 2617)
