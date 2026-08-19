// Parsing RTSP responses (RFC 2326, section 7)
//
// Response format:
//   RTSP/1.0 200 OK\r\n
//   CSeq: 1\r\n
//   Content-Length: 460\r\n     <- if present, a body (e.g. SDP) of that length follows
//   \r\n
//   <body>
//
// What to build:
// - Parse the status line (version / status code / reason phrase)
// - Read the headers into a map (header names are case insensitive,
//   so normalizing them to lowercase is a good idea)
// - Read exactly Content-Length bytes of body with read_exact
//
// Pitfall: reading the body with read() will hand you partial data,
// because TCP does not respect message boundaries. Always use read_exact.
