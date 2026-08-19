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

fn main() {
    // Week 1: start here. Open a TcpStream to localhost:8554
    // and send an OPTIONS request.
    println!("rustsp: not implemented yet");
}
