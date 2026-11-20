//! Cast streaming (what Chrome's "Cast…" uses): the sender side's messages and packets.
//!
//! * `OFFER` / `ANSWER` JSON on the `webrtc` namespace agree on the stream (codec, SSRC, key);
//! * every video frame is encrypted with AES-128-CTR and cut into Cast RTP packets (an RTP header,
//!   then Cast's own 7-byte header: key-frame flag, frame id, packet id, last packet id, reference
//!   frame id) sent over UDP to the port the display named in its `ANSWER`;
//! * RTCP sender reports (on the same UDP port) tell the display how RTP time maps to wall time.
//!
//! This module is pure (no sockets, no processes) so it can be tested exactly.

use aes::cipher::{KeyIvInit, StreamCipher};
use serde_json::{json, Value};

pub const NS_WEBRTC: &str = "urn:x-cast:com.google.cast.webrtc";
/// VP8 in Cast's own RTP profile.
pub const PAYLOAD_TYPE: u8 = 96;
/// Biggest packet payload (the whole packet stays well under a 1500-byte link MTU).
pub const MAX_PAYLOAD: usize = 1100;
/// RTP clock for video.
pub const RTP_CLOCK: u32 = 90_000;
const RTP_HEADER: usize = 12;

type Aes128Ctr = ctr::Ctr128BE<aes::Aes128>;

/// The stream's AES key and IV mask (random, sent to the display inside the `OFFER`).
#[derive(Clone)]
pub struct Keys {
    pub key: [u8; 16],
    pub iv_mask: [u8; 16],
}

impl Keys {
    pub fn random() -> Self {
        use ring::rand::{SecureRandom, SystemRandom};
        let rng = SystemRandom::new();
        let mut k = Keys {
            key: [0; 16],
            iv_mask: [0; 16],
        };
        rng.fill(&mut k.key).expect("system randomness");
        rng.fill(&mut k.iv_mask).expect("system randomness");
        k
    }

    /// The counter block of a frame: the mask with the frame id XORed into bytes 8..12.
    pub fn iv(&self, frame_id: u32) -> [u8; 16] {
        let mut iv = self.iv_mask;
        for (b, f) in iv[8..12].iter_mut().zip(frame_id.to_be_bytes()) {
            *b ^= f;
        }
        iv
    }

    /// Encrypts or decrypts (the same operation) one whole frame.
    pub fn apply(&self, frame_id: u32, data: &mut [u8]) {
        let mut c = Aes128Ctr::new((&self.key).into(), (&self.iv(frame_id)).into());
        c.apply_keystream(data);
    }
}

pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// One encoded video frame.
#[derive(Debug, Clone)]
pub struct EncodedFrame {
    pub data: Vec<u8>,
    pub key: bool,
}

/// Cuts frames into Cast RTP packets.
pub struct Packetizer {
    ssrc: u32,
    keys: Keys,
    seq: u16,
    frame_id: u32,
    last_key_id: u32,
    pub packets: u32,
    pub octets: u32,
}

impl Packetizer {
    pub fn new(ssrc: u32, keys: Keys) -> Self {
        Self {
            ssrc,
            keys,
            seq: 0,
            frame_id: 0,
            last_key_id: 0,
            packets: 0,
            octets: 0,
        }
    }

    /// The id the next frame will carry.
    pub fn next_frame_id(&self) -> u32 {
        self.frame_id
    }

    /// The packets of one frame (encrypted), `rtp_ts` on the 90 kHz clock.
    pub fn packetize(&mut self, frame: &EncodedFrame, rtp_ts: u32) -> Vec<Vec<u8>> {
        let id = self.frame_id;
        self.frame_id = self.frame_id.wrapping_add(1);
        if frame.key {
            self.last_key_id = id;
        }
        // A key frame depends on itself; a delta frame on the one before it.
        let reference = if frame.key { id } else { id.wrapping_sub(1) };
        let mut data = frame.data.clone();
        self.keys.apply(id, &mut data);
        let chunks: Vec<&[u8]> = if data.is_empty() {
            vec![&[][..]]
        } else {
            data.chunks(MAX_PAYLOAD).collect()
        };
        let last = (chunks.len() - 1) as u16;
        chunks
            .iter()
            .enumerate()
            .map(|(i, chunk)| {
                let mut p = Vec::with_capacity(RTP_HEADER + 7 + chunk.len());
                p.push(0x80);
                p.push(PAYLOAD_TYPE | if i as u16 == last { 0x80 } else { 0 });
                p.extend_from_slice(&self.seq.to_be_bytes());
                p.extend_from_slice(&rtp_ts.to_be_bytes());
                p.extend_from_slice(&self.ssrc.to_be_bytes());
                // Cast header: key-frame flag, "has reference frame id" flag, no extensions.
                p.push(if frame.key { 0x80 } else { 0 } | 0x40);
                p.push(id as u8);
                p.extend_from_slice(&(i as u16).to_be_bytes());
                p.extend_from_slice(&last.to_be_bytes());
                p.push(reference as u8);
                p.extend_from_slice(chunk);
                self.seq = self.seq.wrapping_add(1);
                self.packets += 1;
                self.octets += chunk.len() as u32;
                p
            })
            .collect()
    }
}

/// An RTCP sender report (28 bytes): how the 90 kHz RTP clock relates to NTP time.
pub fn sender_report(
    ssrc: u32,
    ntp_secs: u32,
    ntp_frac: u32,
    rtp_ts: u32,
    packets: u32,
    octets: u32,
) -> Vec<u8> {
    let mut p = Vec::with_capacity(28);
    p.push(0x80); // version 2, no padding, no report blocks
    p.push(200); // sender report
    p.extend_from_slice(&6u16.to_be_bytes()); // length in 32-bit words minus one
    p.extend_from_slice(&ssrc.to_be_bytes());
    p.extend_from_slice(&ntp_secs.to_be_bytes());
    p.extend_from_slice(&ntp_frac.to_be_bytes());
    p.extend_from_slice(&rtp_ts.to_be_bytes());
    p.extend_from_slice(&packets.to_be_bytes());
    p.extend_from_slice(&octets.to_be_bytes());
    p
}

/// NTP time (seconds since 1900, 32-bit fraction) of a Unix time.
pub fn ntp_from_unix(secs: u64, nanos: u32) -> (u32, u32) {
    let s = (secs + 2_208_988_800) as u32;
    let frac = ((nanos as u64) << 32) / 1_000_000_000;
    (s, frac as u32)
}

/// What the video stream asks of the display.
#[derive(Debug, Clone, Copy)]
pub struct VideoOffer {
    pub ssrc: u32,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub kbps: u32,
    /// Delay the display adds before showing a frame (a little buffer against jitter), ms.
    pub target_delay_ms: u32,
}

/// The `OFFER` message: one VP8 video stream, encrypted.
pub fn offer(seq: u64, v: &VideoOffer, keys: &Keys) -> Value {
    json!({
        "type": "OFFER",
        "seqNum": seq,
        "offer": {
            "castMode": "mirroring",
            "receiverGetStatus": true,
            "supportedStreams": [{
                "index": 0,
                "type": "video_source",
                "codecName": "vp8",
                "rtpProfile": "cast",
                "rtpPayloadType": PAYLOAD_TYPE,
                "ssrc": v.ssrc,
                "targetDelay": v.target_delay_ms,
                "aesKey": hex(&keys.key),
                "aesIvMask": hex(&keys.iv_mask),
                "timeBase": format!("1/{RTP_CLOCK}"),
                "maxBitRate": v.kbps * 1000,
                "maxFrameRate": format!("{}/1", v.fps),
                "resolutions": [{"width": v.width, "height": v.height}],
                "renderMode": "video",
                "errorRecoveryMode": "castv2",
            }],
        },
    })
}

/// What the display answered.
#[derive(Debug, PartialEq, Eq)]
pub struct Answer {
    pub udp_port: u16,
    /// The SSRCs the display uses for its own RTCP (parallel to the streams we sent).
    pub ssrcs: Vec<u32>,
}

/// Reads an `ANSWER`; `Err` carries the display's own reason for refusing.
pub fn parse_answer(v: &Value) -> Result<Answer, String> {
    if v["result"].as_str() == Some("error") || v["error"].is_object() {
        return Err(format!(
            "{} ({})",
            v["error"]["description"].as_str().unwrap_or("refused"),
            v["error"]["code"]
        ));
    }
    let a = &v["answer"];
    let port = a["udpPort"]
        .as_u64()
        .filter(|p| (1..=65535).contains(p))
        .ok_or("the answer has no UDP port")?;
    Ok(Answer {
        udp_port: port as u16,
        ssrcs: a["ssrcs"]
            .as_array()
            .map(|l| {
                l.iter()
                    .filter_map(|x| x.as_u64().map(|n| n as u32))
                    .collect()
            })
            .unwrap_or_default(),
    })
}

/// A packet from the display is RTCP (types 200–211), not RTP.
pub fn is_rtcp(p: &[u8]) -> bool {
    p.len() >= 8 && p[0] >> 6 == 2 && (200..=211).contains(&p[1])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys() -> Keys {
        Keys {
            key: *b"0123456789abcdef",
            iv_mask: [0xa5; 16],
        }
    }

    #[test]
    fn packets_carry_the_frame_and_decrypt_back() {
        let k = keys();
        let mut p = Packetizer::new(1234, k.clone());
        let original: Vec<u8> = (0..5000u32).map(|i| (i * 7) as u8).collect();
        let packets = p.packetize(
            &EncodedFrame {
                data: original.clone(),
                key: true,
            },
            90_000,
        );
        assert_eq!(packets.len(), 5); // 5000 / 1100, rounded up
        let mut joined = vec![];
        for (i, pk) in packets.iter().enumerate() {
            assert_eq!(pk[0], 0x80);
            let last = i == packets.len() - 1;
            assert_eq!(pk[1] & 0x80 != 0, last, "marker only on the last packet");
            assert_eq!(pk[1] & 0x7f, PAYLOAD_TYPE);
            assert_eq!(u16::from_be_bytes([pk[2], pk[3]]), i as u16); // sequence
            assert_eq!(u32::from_be_bytes(pk[4..8].try_into().unwrap()), 90_000);
            assert_eq!(u32::from_be_bytes(pk[8..12].try_into().unwrap()), 1234);
            assert_eq!(pk[12], 0xc0, "key frame + has reference id");
            assert_eq!(pk[13], 0); // frame id
            assert_eq!(u16::from_be_bytes([pk[14], pk[15]]), i as u16); // packet id
            assert_eq!(u16::from_be_bytes([pk[16], pk[17]]), 4); // last packet id
            assert_eq!(pk[18], 0); // a key frame refers to itself
            assert!(pk.len() <= 1500);
            joined.extend_from_slice(&pk[19..]);
        }
        assert_ne!(joined, original, "the payload must be encrypted");
        k.apply(0, &mut joined);
        assert_eq!(joined, original);
        assert_eq!(p.packets, 5);
        assert_eq!(p.octets, 5000);
    }

    #[test]
    fn delta_frames_refer_to_the_previous_one_and_use_their_own_iv() {
        let k = keys();
        let mut p = Packetizer::new(1, k.clone());
        let key = p.packetize(
            &EncodedFrame {
                data: vec![1; 10],
                key: true,
            },
            0,
        );
        let delta = p.packetize(
            &EncodedFrame {
                data: vec![1; 10],
                key: false,
            },
            3000,
        );
        assert_eq!(key[0][12], 0xc0);
        assert_eq!(delta[0][12], 0x40); // delta + has reference
        assert_eq!(delta[0][13], 1); // frame id 1
        assert_eq!(delta[0][18], 0); // refers to frame 0
                                     // The same plaintext encrypts differently in different frames.
        assert_ne!(key[0][19..], delta[0][19..]);
        assert_ne!(k.iv(0), k.iv(1));
        // Only bytes 8..12 of the IV change with the frame id.
        assert_eq!(k.iv(0)[..8], k.iv(7)[..8]);
        assert_eq!(k.iv(0)[12..], k.iv(7)[12..]);
    }

    #[test]
    fn an_empty_frame_still_makes_one_packet() {
        let mut p = Packetizer::new(1, keys());
        let pk = p.packetize(
            &EncodedFrame {
                data: vec![],
                key: false,
            },
            0,
        );
        assert_eq!(pk.len(), 1);
        assert_eq!(pk[0].len(), 19);
        assert_eq!(pk[0][1] & 0x80, 0x80);
    }

    #[test]
    fn sender_report_layout() {
        let r = sender_report(0x01020304, 10, 20, 30, 40, 50);
        assert_eq!(r.len(), 28);
        assert_eq!(r[..4], [0x80, 200, 0, 6]);
        assert_eq!(u32::from_be_bytes(r[4..8].try_into().unwrap()), 0x01020304);
        assert_eq!(u32::from_be_bytes(r[24..28].try_into().unwrap()), 50);
        assert!(is_rtcp(&r));
        // An RTP packet (payload type 96) is not RTCP.
        assert!(!is_rtcp(&[0x80, 96, 0, 0, 0, 0, 0, 0, 0, 0]));
    }

    #[test]
    fn ntp_conversion() {
        assert_eq!(ntp_from_unix(0, 0), (2_208_988_800, 0));
        assert_eq!(ntp_from_unix(1, 500_000_000), (2_208_988_801, 1 << 31));
    }

    #[test]
    fn offer_has_what_the_display_needs() {
        let v = VideoOffer {
            ssrc: 7,
            width: 1280,
            height: 720,
            fps: 30,
            kbps: 4000,
            target_delay_ms: 400,
        };
        let o = offer(5, &v, &keys());
        assert_eq!(o["type"], "OFFER");
        assert_eq!(o["seqNum"], 5);
        let s = &o["offer"]["supportedStreams"][0];
        assert_eq!(s["codecName"], "vp8");
        assert_eq!(s["rtpProfile"], "cast");
        assert_eq!(s["rtpPayloadType"], 96);
        assert_eq!(s["ssrc"], 7);
        assert_eq!(s["aesKey"].as_str().unwrap().len(), 32);
        assert_eq!(s["aesIvMask"].as_str().unwrap().len(), 32);
        assert_eq!(s["timeBase"], "1/90000");
        assert_eq!(s["resolutions"][0]["width"], 1280);
        assert_eq!(s["maxBitRate"], 4_000_000);
    }

    #[test]
    fn answers_are_read_and_refusals_carry_their_reason() {
        let ok = json!({"type":"ANSWER","result":"ok","answer":{"udpPort":43210,"ssrcs":[8]}});
        assert_eq!(
            parse_answer(&ok),
            Ok(Answer {
                udp_port: 43210,
                ssrcs: vec![8]
            })
        );
        let no =
            json!({"type":"ANSWER","result":"error","error":{"code":1,"description":"bad offer"}});
        assert!(parse_answer(&no).unwrap_err().contains("bad offer"));
        assert!(parse_answer(&json!({"result":"ok","answer":{}})).is_err());
        assert!(parse_answer(&json!({"result":"ok","answer":{"udpPort":0}})).is_err());
    }

    #[test]
    fn keys_are_random() {
        let (a, b) = (Keys::random(), Keys::random());
        assert_ne!(a.key, b.key);
        assert_ne!(a.iv_mask, b.iv_mask);
    }
}
