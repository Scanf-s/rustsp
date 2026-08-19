// Building RTSP requests (RFC 2326, section 6)
//
// Request format. Every line ends with \r\n, and one empty line closes the headers:
//
//   OPTIONS rtsp://localhost:8554/test RTSP/1.0\r\n
//   CSeq: 1\r\n
//   User-Agent: rustsp\r\n
//   \r\n
//
// What to build: a single function that takes a method
// (OPTIONS/DESCRIBE/SETUP/PLAY/TEARDOWN) plus extra headers and
// assembles the request string. That is all you need.
//
// Required headers per method:
// - DESCRIBE: Accept: application/sdp
// - SETUP:    Transport: RTP/AVP/TCP;unicast;interleaved=0-1
// - PLAY:     Session: <value from the SETUP response>, Range: npt=0.000-
// - TEARDOWN: Session
