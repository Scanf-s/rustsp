// rustsp: an RTSP client built from scratch, no protocol libraries (learning project)
//
// Handshake:  OPTIONS -> DESCRIBE -> SETUP -> PLAY -> TEARDOWN
// Media:      interleaved RTP on the same TCP connection, depacketized to output.h264
//
// The handshake already works. The media path still needs three functions, and
// those are the ones you write:
//   transport::read_frame   (src/transport.rs)
//   rtp::packet::parse      (src/rtp/packet.rs)
//   rtp::h264::Depacketizer (src/rtp/h264.rs)
//
// Run the handshake only:      cargo run
// Run the media loop as well:  RUSTSP_MEDIA=1 cargo run
// Silence the wire trace:      RUSTSP_TRACE=0 cargo run

mod error;
mod rtp;
mod rtsp;
mod sdp;
mod transport;

use std::fs::File;
use std::io::{BufWriter, Write};

use error::Result;
use rtsp::Session;

const URL: &str = "rtsp://127.0.0.1:8554/test";
const OUTPUT: &str = "output.h264";

fn main() {
    if let Err(e) = run() {
        eprintln!("\nerror: {e}");
        if matches!(e, error::Error::Io(ref io) if io.kind() == std::io::ErrorKind::ConnectionRefused)
        {
            eprintln!("hint: nothing is listening on 8554. Start the test rig:");
            eprintln!("      docker compose up -d");
        }
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let mut session = Session::connect(URL)?;
    session.trace = std::env::var("RUSTSP_TRACE").as_deref() != Ok("0");

    // --- OPTIONS: which methods does this server support? ---
    let options = session.options()?;
    println!(
        "server: {}",
        options.header("Server").unwrap_or("(no Server header)")
    );
    println!(
        "public: {}",
        options.header("Public").unwrap_or("(no Public header)")
    );

    // --- DESCRIBE: what does this stream contain? ---
    let sdp = session.describe()?;
    let video = sdp
        .video()
        .ok_or_else(|| error::Error::Protocol("the SDP has no video track".into()))?;

    println!(
        "\nvideo track: payload type {}, {} @ {} Hz",
        video.payload_type,
        video.encoding.as_deref().unwrap_or("?"),
        video.clock_rate.unwrap_or(0)
    );
    println!(
        "  a=control: {}",
        video.control.as_deref().unwrap_or("(none)")
    );
    match (&video.sps, &video.pps) {
        (Some(sps), Some(pps)) => {
            println!(
                "  SPS {} bytes, PPS {} bytes (from sprop-parameter-sets)",
                sps.len(),
                pps.len()
            )
        }
        _ => println!("  no SPS/PPS in the SDP; they will arrive in the stream as STAP-A"),
    }

    // SETUP is sent to the track URL, which can be relative to the presentation URL.
    let track_url =
        sdp::resolve_control(session.base_url(), video.control.as_deref().unwrap_or("*"));
    let sps = video.sps.clone();
    let pps = video.pps.clone();

    // --- SETUP: agree on a transport and receive a Session id ---
    let setup = session.setup(&track_url)?;
    println!("\nsetup ok");
    println!("  transport: {}", setup.header("Transport").unwrap_or("?"));
    println!("  session:   {}", session.session_id().unwrap_or("?"));
    println!("  state:     {:?}", session.state());

    // --- PLAY: the media starts to arrive on this same connection ---
    let play = session.play()?;
    println!("\nplay ok, state {:?}", session.state());
    if let Some(info) = play.header("RTP-Info") {
        // seq and rtptime describe what the first RTP packet will contain.
        println!("  RTP-Info: {info}");
    }

    if std::env::var("RUSTSP_MEDIA").is_ok() {
        receive_media(&mut session, sps.as_deref(), pps.as_deref())?;
    } else {
        println!(
            "\nhandshake complete. Media loop skipped.\n\
             Implement transport::read_frame, rtp::packet::parse and rtp::h264::Depacketizer,\n\
             then run:  RUSTSP_MEDIA=1 cargo run"
        );
    }

    // --- TEARDOWN ---
    session.teardown()?;
    println!("\nteardown ok, state {:?}", session.state());
    Ok(())
}

/// Read interleaved frames until enough video has been collected, and write the
/// result to output.h264 in Annex B format. This function is only the wiring around
/// the three functions that you implement.
fn receive_media(session: &mut Session, sps: Option<&[u8]>, pps: Option<&[u8]>) -> Result<()> {
    use rtp::h264::{Depacketizer, START_CODE};

    const FRAMES_TO_COLLECT: usize = 300;

    let mut file = BufWriter::new(File::create(OUTPUT)?);

    // A decoder needs SPS and PPS before it can decode any slice. When the SDP
    // contained them, they are written first, so that the file can be played from
    // the very first byte.
    for param in [sps, pps].into_iter().flatten() {
        file.write_all(&START_CODE)?;
        file.write_all(param)?;
    }

    let mut depacketizer = Depacketizer::new();
    let mut frames = 0usize;
    let mut nals = 0usize;
    let mut bytes = 0usize;

    println!("\ncollecting {FRAMES_TO_COLLECT} interleaved frames...");

    while frames < FRAMES_TO_COLLECT {
        let frame = transport::read_frame(session.reader())?;
        frames += 1;

        // Channel 1 carries RTCP, which is not needed in order to write this file.
        if frame.channel != transport::CHANNEL_RTP {
            continue;
        }

        let packet = rtp::packet::parse(&frame.payload)?;
        for nal in depacketizer.push(packet.sequence_number, &packet.payload)? {
            let nal_type = nal[0] & 0x1F;
            if session.trace {
                eprintln!(
                    "rtp seq={} ts={} marker={} -> NAL type {} ({} bytes)",
                    packet.sequence_number,
                    packet.timestamp,
                    packet.marker,
                    nal_type,
                    nal.len()
                );
            }
            file.write_all(&START_CODE)?;
            file.write_all(&nal)?;
            nals += 1;
            bytes += nal.len() + START_CODE.len();
        }
    }

    file.flush()?;
    println!("wrote {nals} NAL units, {bytes} bytes to {OUTPUT}");
    println!("check it with:  ffplay {OUTPUT}");
    Ok(())
}
