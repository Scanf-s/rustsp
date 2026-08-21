# rustsp

An RTSP client written from scratch to learn the protocol. Connects to mediamtx,
walks OPTIONS -> DESCRIBE -> SETUP -> PLAY -> TEARDOWN by hand, receives interleaved
RTP on the same TCP connection, and writes Annex B H.264 to `output.h264`.

Done when `ffplay output.h264` plays.

## Ground rules

**No protocol crates.** Not `retina`, `rtsp-types`, `sdp-types`, `ffmpeg-next` or
`openh264`: they would do the very thing this project exists to learn. The crate list is
currently empty and there is no need for it to grow. Bytes go through
`u16::from_be_bytes`, not `byteorder`. Base64 for `sprop-parameter-sets` is a 20 line
function in `src/sdp.rs`.

**Blocking std IO on purpose.** `std::net::TcpStream`, not tokio. Porting to tokio is a
later exercise, not a starting point.

**Comments in English**, natural B2-C1 level, plain punctuation. No em-dashes, no middle
dots. Conversation with the user is in Korean; comments and docs are in English.

**Division of labor.** The user implements the protocol-shaped logic, because the bugs
there are the lesson. Claude writes plumbing, test vectors and tooling, where a bug only
costs time. If asked to implement one of the modules listed as theirs below, say so first
rather than writing it.

## Layout

Written and verified:

| File | Role |
|---|---|
| `src/error.rs` | `Io` / `Protocol` / `Status` |
| `src/rtsp/request.rs` | request builder; the CRLF terminator lives in exactly one line |
| `src/rtsp/response.rs` | status line, headers, `Content-Length` body via `read_exact` |
| `src/rtsp/mod.rs` | `Session`: connection, CSeq, Session id, state machine, wire trace |
| `src/sdp.rs` | payload type, `a=control`, `a=rtpmap`, SPS/PPS |
| `src/main.rs` | the handshake, plus wiring for the media loop |

The user's to implement, stubbed with `todo!()` and failing tests:

| File | Task | Tests |
|---|---|---|
| `src/transport.rs` | `read_frame`: `$` + channel + u16 BE length + payload | 6 |
| `src/rtp/packet.rs` | `parse`: 12 byte header, bit fields, payload offset with CC and X | 8 |
| `src/rtp/h264.rs` | `Depacketizer::push`: Single NAL, STAP-A, FU-A reassembly | 10 |

Baseline: `cargo test` gives 15 passing, 24 failing, all failures inside those three.

## Invariants that must not be broken

- **One `BufReader` owns the `TcpStream` for the whole session.** Writes go through
  `get_mut()`. Reading the raw stream would skip bytes already sitting in the buffer,
  which corrupts interleaved RTP in a way that is very hard to trace.
- **`read_exact`, never `read`,** for anything length prefixed. A short read must be an
  error, not a short frame.
- **Never read until the peer closes.** An RTSP server keeps the connection open for the
  next request, so `read_to_string` blocks forever.
- **Nothing after the blank line of a request.** Stray bytes are parsed as the start of
  the next request, so the failure surfaces one request later than its cause.

## Running

```bash
docker compose up -d        # mediamtx plus an ffmpeg test pattern publishing into it
cargo run                   # handshake, with a wire trace on stderr
RUSTSP_MEDIA=1 cargo run    # also run the media loop, once the three modules exist
RUSTSP_TRACE=0 cargo run    # quiet
docker compose down
```

Stream is at `rtsp://127.0.0.1:8554/test`. Container only: nothing needs installing on the
host, not even ffmpeg. `mediamtx.yml` leaves only RTSP enabled, which trims the log to the
RTSP exchange and stops a self-signed MoQ certificate appearing in the working directory.

Only 8554/tcp is published, which is all TCP interleaved transport needs. Adding the UDP
transport later means publishing UDP port ranges, which container networking handles
badly; the commented ports in `docker-compose.yml` are the starting point.

## What the live server does that RFC 2326 examples do not

Observed against mediamtx v1.20.1, and worth remembering because the examples mislead:

- **Session ids are 32 character hex strings**, not integers
  (`0b7fbd8faad549fd912d8eaab6eafba2;timeout=55`). Never parse one as a number.
- **`timeout=55`**: the session expires after 55 seconds of silence, so a long media loop
  has to call `Session::keepalive()` inside that window.
- **`RTP-Info` carries only `url=`**, with no `seq=` or `rtptime=`. RFC 2326 section 14
  shows both. The depacketizer cannot seed itself from them and must work it out from the
  first packet.
- **`Public` omits `OPTIONS`**, from a server that just answered an OPTIONS request. Treat
  `Public` as a hint, never gate a method on it.
- The SETUP response adds `ssrc=` to `Transport`, which is a useful cross check when
  `rtp::packet::parse` starts producing output.

## Reference

- RFC 2326 (RTSP 1.0) is the implementation target; mediamtx speaks it. RFC 7826 (RTSP
  2.0) is not deployed anywhere but explains concepts more clearly.
- CSeq is mandatory on every request and response (RFC 2326 section 12.17) yet appears in
  none of the four header grammar lists. Editorial gap; do not derive required headers
  from the grammar in section 6.
- RTP is RFC 3550 section 5.1. H.264 packetization is RFC 6184 section 5.
