//! Wire format: y-sync protocol v1 messages, each framed by its length as a variable-length
//! unsigned integer (LEB128), the same way the protocol nests its own buffers.
//!
//! A connection starts with a hello (a custom message, [`MSG_HELLO`]) from each side carrying the
//! share code and peer ids; only after a matching hello do the peers exchange sync step 1 (state
//! vector), sync step 2 (the missing changes) and, from then on, updates and awareness.

use std::io::{self, Read, Write};

use serde::{Deserialize, Serialize};
use yrs::sync::Message;
use yrs::updates::decoder::Decode;
use yrs::updates::encoder::Encode;

/// Custom message tag of the hello.
pub const MSG_HELLO: u8 = 100;
/// Largest frame accepted, in bytes.
pub const MAX_FRAME: usize = 256 << 20;
/// Version of the hello.
const HELLO_VERSION: u32 = 1;
/// Application tag in the hello, so other y-sync speakers are turned away.
const APP: &str = "azam";

/// First message of every connection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hello {
    /// Always `azam`.
    pub app: String,
    /// Hello version.
    pub version: u32,
    /// Share code of the session; empty when joining by address without a code.
    pub code: String,
    /// Id of the sending peer (its network replica).
    pub peer: u64,
    /// Awareness client id of the sending peer.
    pub awareness: u64,
    /// Random number picked per connection, to choose between two connections to one peer.
    #[serde(default)]
    pub nonce: u64,
}

impl Hello {
    /// A hello for session `code` from `peer`.
    #[must_use]
    pub fn new(code: &str, peer: u64, awareness: u64) -> Self {
        Self {
            app: APP.to_owned(),
            version: HELLO_VERSION,
            code: code.to_owned(),
            peer,
            awareness,
            nonce: 0,
        }
    }

    /// The hello as a message.
    #[must_use]
    pub fn to_message(&self) -> Message {
        Message::Custom(MSG_HELLO, serde_json::to_vec(self).unwrap_or_default())
    }

    /// The hello in `msg`, if it is one from azam.
    #[must_use]
    pub fn from_message(msg: &Message) -> Option<Self> {
        match msg {
            Message::Custom(MSG_HELLO, data) => {
                serde_json::from_slice::<Self>(data).ok().filter(|h| h.app == APP)
            }
            _ => None,
        }
    }
}

/// Encodes `msg` as one frame.
#[must_use]
pub fn frame(msg: &Message) -> Vec<u8> {
    let body = msg.encode_v1();
    let mut out = Vec::with_capacity(body.len() + 5);
    write_var(&mut out, body.len() as u64);
    out.extend_from_slice(&body);
    out
}

/// Writes `msg` as one frame.
///
/// # Errors
/// Fails if the write fails.
pub fn write_message(w: &mut impl Write, msg: &Message) -> io::Result<()> {
    w.write_all(&frame(msg))?;
    w.flush()
}

/// Reads one frame's body.
///
/// # Errors
/// Fails on end of stream, a read error or a frame larger than [`MAX_FRAME`].
pub fn read_frame(r: &mut impl Read) -> io::Result<Vec<u8>> {
    let len = read_var(r)?;
    let len =
        usize::try_from(len).map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "frame too large"))?;
    if len > MAX_FRAME {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "frame too large"));
    }
    let mut buf = vec![0; len];
    r.read_exact(&mut buf)?;
    Ok(buf)
}

/// Decodes a frame's body as a message.
///
/// # Errors
/// Fails if the bytes are not a y-sync message.
pub fn decode(body: &[u8]) -> Result<Message, String> {
    Message::decode_v1(body).map_err(|e| e.to_string())
}

fn write_var(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let byte = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

fn read_var(r: &mut impl Read) -> io::Result<u64> {
    let mut v = 0u64;
    for shift in (0..64).step_by(7) {
        let mut b = [0u8; 1];
        r.read_exact(&mut b)?;
        v |= u64::from(b[0] & 0x7f) << shift;
        if b[0] & 0x80 == 0 {
            return Ok(v);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "length prefix too long",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use yrs::sync::SyncMessage;

    #[test]
    fn frames_round_trip() {
        let hello = Hello::new("abc", 7, 9);
        let msgs = [
            hello.to_message(),
            Message::Sync(SyncMessage::Update(vec![1; 300])),
            Message::Sync(SyncMessage::SyncStep1(yrs::StateVector::default())),
        ];
        let mut buf = Vec::new();
        for m in &msgs {
            write_message(&mut buf, m).expect("writes");
        }
        let mut r = buf.as_slice();
        for m in &msgs {
            let got = decode(&read_frame(&mut r).expect("reads")).expect("decodes");
            assert_eq!(&got, m);
        }
        assert_eq!(Hello::from_message(&msgs[0]), Some(hello));
    }
}
