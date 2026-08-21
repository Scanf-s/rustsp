// H.264 depacketization (RFC 6184 section 5)  --  YOUR CODE
//
// This is the heart of week 4. One RTP payload may be a whole NAL unit, several NAL
// units, or one slice of a big NAL unit. The low 5 bits of the first payload byte say
// which:
//
//   1..=23  Single NAL unit    the payload IS the NAL unit; emit it as is
//   24      STAP-A             several NALs packed as [u16 BE length][NAL] repeated
//   28      FU-A               one NAL split across packets; reassemble
//
//   (25..=27 and 29 exist too. mediamtx does not send them, so reject them.)
//
// FU-A layout:
//
//   payload[0] = FU indicator:  F(1) | NRI(2) | type=28(5)
//   payload[1] = FU header:     S(1) | E(1)   | R(1) | type(5)
//   payload[2..] = the fragment
//
//   S = start of the NAL unit, E = end of it.
//   On the START fragment, rebuild the original NAL header as
//       (indicator & 0xE0) | type
//   which puts F and NRI back from the indicator and the real type from the FU header.
//   Then append payload[2..] of every fragment until you see E.
//
// Things that must NOT produce a broken NAL unit:
//   - A middle or end fragment arriving with no start seen: drop it.
//   - A sequence number gap while reassembling: throw the partial NAL away. A hole
//     in the middle of a slice is not something a decoder can recover from, and
//     emitting it corrupts the picture.
//
// The caller adds the Annex B start code (00 00 00 01) before writing to the file;
// this module returns bare NAL units.
//
// Run `cargo test rtp::h264` while you work.

use crate::error::Result;

pub const START_CODE: [u8; 4] = [0x00, 0x00, 0x00, 0x01];

pub const NAL_STAP_A: u8 = 24;
pub const NAL_FU_A: u8 = 28;

/// Reassembles NAL units across RTP packets. It has to be a struct rather than a
/// function because FU-A state spans packets.
#[derive(Debug, Default)]
pub struct Depacketizer {
    /// The NAL unit being rebuilt from FU-A fragments, if any.
    partial: Option<Vec<u8>>,
    /// Sequence number of the last fragment accepted, to detect gaps.
    last_seq: Option<u16>,
}

impl Depacketizer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one RTP payload in. Returns however many complete NAL units it produced:
    /// zero (a fragment landed), one (single NAL, or an FU-A completed), or several
    /// (STAP-A).
    pub fn push(&mut self, sequence_number: u16, payload: &[u8]) -> Result<Vec<Vec<u8>>> {
        let _ = (sequence_number, payload, &self.partial, &self.last_seq);
        let _ = (NAL_STAP_A, NAL_FU_A);
        todo!("week 4: dispatch on the NAL type and reassemble FU-A")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 0x41 = 0b0100_0001 -> F=0, NRI=2, type=1 (non IDR slice)
    #[test]
    fn single_nal_passes_through_unchanged() {
        let mut d = Depacketizer::new();
        let out = d.push(100, &[0x41, 0xAA, 0xBB]).unwrap();
        assert_eq!(out, vec![vec![0x41, 0xAA, 0xBB]]);
    }

    // 0x78 = 0b0111_1000 -> F=0, NRI=3, type=24 (STAP-A)
    #[test]
    fn stap_a_yields_every_packed_nal() {
        let mut d = Depacketizer::new();
        let payload = [
            0x78, // STAP-A indicator
            0x00, 0x04, 0x67, 0x42, 0x00, 0x1E, // len 4, SPS (type 7)
            0x00, 0x04, 0x68, 0xCE, 0x38, 0x80, // len 4, PPS (type 8)
        ];
        let out = d.push(100, &payload).unwrap();
        assert_eq!(out, vec![
            vec![0x67, 0x42, 0x00, 0x1E],
            vec![0x68, 0xCE, 0x38, 0x80],
        ]);
    }

    // FU-A indicator 0x7C = F=0, NRI=3, type=28.
    // FU header: S=0x80, E=0x40, then the real NAL type in the low 5 bits.
    // Reconstructed NAL header = (0x7C & 0xE0) | 5 = 0x65 (IDR slice).
    #[test]
    fn fu_a_reassembles_across_three_fragments() {
        let mut d = Depacketizer::new();

        assert!(d.push(10, &[0x7C, 0x85, 0xAA, 0xBB]).unwrap().is_empty()); // S=1
        assert!(d.push(11, &[0x7C, 0x05, 0xCC]).unwrap().is_empty()); // middle
        let out = d.push(12, &[0x7C, 0x45, 0xDD]).unwrap(); // E=1

        assert_eq!(out, vec![vec![0x65, 0xAA, 0xBB, 0xCC, 0xDD]]);
    }

    #[test]
    fn fu_a_without_a_start_fragment_is_dropped() {
        let mut d = Depacketizer::new();
        // We joined mid NAL: first thing we ever see is a middle fragment.
        assert!(d.push(11, &[0x7C, 0x05, 0xCC]).unwrap().is_empty());
        assert!(d.push(12, &[0x7C, 0x45, 0xDD]).unwrap().is_empty());
    }

    #[test]
    fn a_sequence_gap_discards_the_partial_nal() {
        let mut d = Depacketizer::new();
        d.push(10, &[0x7C, 0x85, 0xAA]).unwrap(); // start
        // seq 11 never arrived; 12 shows up instead.
        assert!(d.push(12, &[0x7C, 0x45, 0xDD]).unwrap().is_empty());
    }

    #[test]
    fn recovers_on_the_next_start_fragment_after_a_gap() {
        let mut d = Depacketizer::new();
        d.push(10, &[0x7C, 0x85, 0xAA]).unwrap();
        d.push(12, &[0x7C, 0x45, 0xDD]).unwrap(); // dropped, gap

        d.push(13, &[0x7C, 0x85, 0x11]).unwrap();
        let out = d.push(14, &[0x7C, 0x45, 0x22]).unwrap();
        assert_eq!(out, vec![vec![0x65, 0x11, 0x22]]);
    }

    #[test]
    fn sequence_number_wraps_around_at_65535() {
        let mut d = Depacketizer::new();
        d.push(65_535, &[0x7C, 0x85, 0xAA]).unwrap();
        // 0 follows 65535; wrapping_add(1) says so, plain + 1 does not.
        let out = d.push(0, &[0x7C, 0x45, 0xBB]).unwrap();
        assert_eq!(out, vec![vec![0x65, 0xAA, 0xBB]]);
    }

    #[test]
    fn single_nal_in_the_middle_clears_any_partial_state() {
        let mut d = Depacketizer::new();
        d.push(10, &[0x7C, 0x85, 0xAA]).unwrap(); // start a fragment
        let out = d.push(11, &[0x41, 0x99]).unwrap(); // complete NAL arrives instead
        assert_eq!(out, vec![vec![0x41, 0x99]]);
        // The abandoned fragment must not resurface later.
        assert!(d.push(12, &[0x7C, 0x45, 0xDD]).unwrap().is_empty());
    }

    #[test]
    fn empty_payload_is_an_error() {
        let mut d = Depacketizer::new();
        assert!(d.push(10, &[]).is_err());
    }

    #[test]
    fn truncated_stap_a_is_an_error() {
        let mut d = Depacketizer::new();
        // Says the first NAL is 8 bytes long, then supplies 2.
        assert!(d.push(10, &[0x78, 0x00, 0x08, 0x67, 0x42]).is_err());
    }
}
