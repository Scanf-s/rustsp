// H.264 depacketization (RFC 6184 section 5)  --  YOUR CODE
//
// This is the central part of week 4. One RTP payload can hold a complete NAL unit,
// several NAL units, or one piece of a large NAL unit. The lowest 5 bits of the first
// payload byte say which case it is:
//
//   1..=23  Single NAL unit    the payload is the NAL unit, so return it unchanged
//   24      STAP-A             several NAL units, each one stored as a u16 big
//                              endian length followed by the NAL unit itself
//   28      FU-A               one NAL unit that is split over several packets, so
//                              it has to be reassembled
//
//   (The types 25..=27 and 29 exist as well. mediamtx does not send them, so this
//   module returns an error for them.)
//
// FU-A layout:
//
//   payload[0] = FU indicator:  F(1) | NRI(2) | type=28(5)
//   payload[1] = FU header:     S(1) | E(1)   | R(1) | type(5)
//   payload[2..] = the fragment
//
//   S marks the first fragment of the NAL unit, and E marks the last one.
//   When the start fragment arrives, build the original NAL header again as
//       (indicator & 0xE0) | type
//   This takes F and NRI from the indicator and the real type from the FU header.
//   After that, append payload[2..] of every following fragment until E is set.
//
// Two situations must never produce a damaged NAL unit:
//   - A middle or end fragment arrives although no start fragment was seen. Drop it.
//   - A sequence number is missing during the reassembly. Throw the incomplete NAL
//     unit away. A decoder cannot repair a gap inside a slice, and returning such a
//     NAL unit only damages the picture.
//
// The caller writes the Annex B start code (00 00 00 01) into the file, so this
// module returns NAL units without a start code.
//
// Run `cargo test rtp::h264` while you work.

use crate::error::Result;

pub const START_CODE: [u8; 4] = [0x00, 0x00, 0x00, 0x01];

pub const NAL_STAP_A: u8 = 24;
pub const NAL_FU_A: u8 = 28;

/// Reassembles NAL units that are spread over several RTP packets. This has to be a
/// struct and not a plain function, because the FU-A state has to survive between
/// packets.
#[derive(Debug, Default)]
pub struct Depacketizer {
    /// The NAL unit that is currently being rebuilt from FU-A fragments, if there
    /// is one.
    partial: Option<Vec<u8>>,
    /// The sequence number of the last accepted fragment, used to detect a gap.
    last_seq: Option<u16>,
}

impl Depacketizer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Pass in one RTP payload. Returns every complete NAL unit that this payload
    /// produced: none when only a fragment arrived, one for a single NAL unit or for
    /// a finished FU-A, and several for a STAP-A.
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
        assert_eq!(
            out,
            vec![vec![0x67, 0x42, 0x00, 0x1E], vec![0x68, 0xCE, 0x38, 0x80],]
        );
    }

    // FU-A indicator 0x7C = F=0, NRI=3, type=28.
    // FU header: S=0x80, E=0x40, then the real NAL type in the low 5 bits.
    // The rebuilt NAL header is (0x7C & 0xE0) | 5 = 0x65, which is an IDR slice.
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
        // The client started to receive inside a NAL unit, so the first fragment
        // it ever sees is a middle fragment.
        assert!(d.push(11, &[0x7C, 0x05, 0xCC]).unwrap().is_empty());
        assert!(d.push(12, &[0x7C, 0x45, 0xDD]).unwrap().is_empty());
    }

    #[test]
    fn a_sequence_gap_discards_the_partial_nal() {
        let mut d = Depacketizer::new();
        d.push(10, &[0x7C, 0x85, 0xAA]).unwrap(); // start
        // Sequence number 11 never arrived, and 12 arrives instead.
        assert!(d.push(12, &[0x7C, 0x45, 0xDD]).unwrap().is_empty());
    }

    #[test]
    fn recovers_on_the_next_start_fragment_after_a_gap() {
        let mut d = Depacketizer::new();
        d.push(10, &[0x7C, 0x85, 0xAA]).unwrap();
        d.push(12, &[0x7C, 0x45, 0xDD]).unwrap(); // dropped because of the gap

        d.push(13, &[0x7C, 0x85, 0x11]).unwrap();
        let out = d.push(14, &[0x7C, 0x45, 0x22]).unwrap();
        assert_eq!(out, vec![vec![0x65, 0x11, 0x22]]);
    }

    #[test]
    fn sequence_number_wraps_around_at_65535() {
        let mut d = Depacketizer::new();
        d.push(65_535, &[0x7C, 0x85, 0xAA]).unwrap();
        // 0 comes directly after 65535. wrapping_add(1) gives that result, and a
        // plain + 1 does not.
        let out = d.push(0, &[0x7C, 0x45, 0xBB]).unwrap();
        assert_eq!(out, vec![vec![0x65, 0xAA, 0xBB]]);
    }

    #[test]
    fn single_nal_in_the_middle_clears_any_partial_state() {
        let mut d = Depacketizer::new();
        d.push(10, &[0x7C, 0x85, 0xAA]).unwrap(); // the start of a fragmented NAL
        let out = d.push(11, &[0x41, 0x99]).unwrap(); // a complete NAL arrives instead
        assert_eq!(out, vec![vec![0x41, 0x99]]);
        // The abandoned fragment must not appear again later.
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
        // The header says that the first NAL unit is 8 bytes long, but only 2
        // bytes follow.
        assert!(d.push(10, &[0x78, 0x00, 0x08, 0x67, 0x42]).is_err());
    }
}
