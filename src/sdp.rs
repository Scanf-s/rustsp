// SDP parsing (RFC 8866; the H.264 specific parts are in RFC 6184, section 8)
//
// What to extract from the DESCRIBE response body:
// - The payload type number from the "m=video ..." line (e.g. 96)
// - The track control URL from "a=control:". This becomes the target of the
//   SETUP request. If it is a relative path (e.g. "trackID=0"), append it
//   to the base URL.
// - From "a=fmtp:96 ... sprop-parameter-sets=<base64 SPS>,<base64 PPS>":
//   the SPS and PPS needed to initialize a decoder. Write them at the very
//   start of output.h264 in Annex B form so ffplay can play the file.
//   (They also show up periodically inside the stream itself.)
//
// split('\n') plus strip_prefix is all you need here.
// A parser combinator crate is optional, not required.
