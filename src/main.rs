// rustsp: an RTSP client built from scratch, no protocol libraries (learning project)
//
// Goal: connect to rtsp://localhost:8554/test and walk through
//   OPTIONS -> DESCRIBE -> SETUP -> PLAY -> (receive RTP) -> TEARDOWN
// by hand, then reassemble the H.264 NAL units and save them to output.h264.
// Definition of done: `ffplay output.h264` plays the video.

mod rtsp;
mod sdp;
mod transport;
mod rtp;

use std::io::{prelude::*, BufReader};
use std::net::TcpStream;

fn main() -> std::io::Result<()> {
    // Week 1: start here. Open a TcpStream to localhost:8554
    // and send an OPTIONS request.

    // Open the TCP connection to the host and port (RTSP needs TCP connection first)
    let mut stream = TcpStream::connect("127.0.0.1:8554")?;
    println!("Successfully connected to the server");

    // Send OPTIONS request
    // https://datatracker.ietf.org/doc/html/rfc2326#section-10.1
    let request = "OPTIONS rtsp://127.0.0.1:8554 RTSP/1.0\r\n\
                   CSeq: 1\r\n\
                   \r\n\
                   ";
    println!("REQUEST: {:?}", request);

    // Send the raw bytes over the TCP stream
    stream.write_all(request.as_bytes())?;
    stream.flush()?;

    // Read the server's response headers
    let mut reader = BufReader::new(&stream);
    let mut response = String::new();
    reader.read_line(&mut response)?;
    println!("Status Line: {}", response.trim());
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        if line == "\r\n" || line.is_empty() {
            break; // End of headers reached
        }
        println!("Header: {}", line.trim());
    }

    Ok(())
}
