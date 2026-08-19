// H.264 depacketization (RFC 6184, section 5)
//
// The lower 5 bits of the first payload byte give the NAL unit type:
// - 1..=23 : Single NAL. The whole payload is one NAL unit. Pass it through.
// - 24 (STAP-A): several NAL units packed as [u16 BE length][NAL]...
//   SPS and PPS usually arrive this way.
// - 28 (FU-A): one large NAL unit split into fragments. Needs reassembly:
//     payload[0] = FU indicator (take F/NRI from here)
//     payload[1] = FU header: S(start) | E(end) | R | type(5 bits)
//   On the start fragment, rebuild the NAL header as
//   (indicator & 0xE0) | type, then keep appending payload[2..] from the
//   following fragments. The end fragment completes the NAL unit.
//
// Output: prepend the Annex B start code 00 00 00 01 to every NAL unit and
// write it to the file. The result is an .h264 file that ffplay can read.
//
// This is the number one candidate for unit tests. Write fragment packets
// as array literals and check the reassembled output. Cover the failure
// cases too, so you have a safety net when refactoring later:
// - normal reassembly
// - a middle fragment arriving without a start (dropping it is correct)
// - a sequence number gap during reassembly (discard that NAL unit)
