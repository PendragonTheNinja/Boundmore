//! The wire protocol (ADR-0004).
//!
//! Two channels share one QUIC connection:
//!
//! - **Reliable and ordered**: one bidirectional stream carrying [`ClientMsg`]
//!   and [`ServerMsg`] frames. World state travels here (chunks, block
//!   changes, edit results), and ordering matters: a block change must never
//!   overtake the chunk it applies to.
//! - **Unreliable datagrams**: [`ClientDatagram`] inputs and [`ServerDatagram`]
//!   snapshots. Movement is sent many times a second and every packet is
//!   superseded by the next, so a lost one is never resent. Inputs carry the
//!   last few ticks redundantly so a single loss costs nothing.
//!
//! Everything is `postcard` (compact varints). Reliable frames are prefixed with
//! a little-endian `u32` length. Every message from the other side is untrusted:
//! decoders bound sizes, and the server validates content.

use bm_core::{BlockDef, BlockId, BlockPos, ChunkPos, MoveConfig, PlanetShape, PlayerInput, PlayerState};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

/// Bump on any change to a message below. Client and server must match.
pub const PROTOCOL_VERSION: u32 = 1;

/// QUIC application protocol name; also refuses non-Boundmore peers early.
pub const ALPN: &[u8] = b"boundmore/1";

/// Largest reliable frame accepted (a chunk is at most ~300 KiB encoded).
pub const MAX_FRAME: usize = 1 << 20;

/// Inputs repeated per datagram. At 60 Hz, 6 inputs survive 5 consecutive
/// lost packets (~83 ms) without the server missing a tick.
pub const INPUT_REDUNDANCY: usize = 6;

pub const MAX_NAME_LEN: usize = 24;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ClientMsg {
    Hello {
        protocol: u32,
        name: String,
    },
    Edit {
        seq: u32,
        pos: BlockPos,
        action: EditAction,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EditAction {
    Break,
    Place(BlockId),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Welcome {
    pub player_id: u32,
    pub planet: PlanetShape,
    pub planet_name: String,
    /// The block catalogue. The client renders whatever the server says a
    /// block is (ADR-0003); it ships no copy of its own that could disagree.
    pub blocks: Vec<BlockDef>,
    pub movement: MoveConfig,
    pub spawn: PlayerState,
    /// Server tick the spawn state belongs to.
    pub tick: u64,
    /// Horizontal chunk radius the server streams around the player.
    pub view_radius: u8,
    /// Furthest a player may edit from their eye, in blocks.
    pub reach: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ServerMsg {
    Welcome(Box<Welcome>),
    Refused {
        reason: String,
    },
    /// Encoded with `Chunk::encode`.
    Chunk {
        pos: ChunkPos,
        data: Vec<u8>,
    },
    /// The server stops sending changes for this chunk; drop it.
    Forget {
        pos: ChunkPos,
    },
    BlockChanged {
        pos: BlockPos,
        block: BlockId,
    },
    /// Answer to the sender's `Edit`, carrying what the block now is: the
    /// predicted change if accepted, the true block if refused.
    EditResult {
        seq: u32,
        pos: BlockPos,
        block: BlockId,
        accepted: bool,
    },
    PlayerJoined {
        id: u32,
        name: String,
    },
    PlayerLeft {
        id: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ClientDatagram {
    /// `inputs[i]` is the input for tick sequence `latest - i`.
    Input { latest: u32, inputs: Vec<PlayerInput> },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct RemotePlayer {
    pub id: u32,
    /// Feet position, canonical. `f32` is ample on a planet a few km around.
    pub pos: [f32; 3],
    pub yaw: u16,
    pub pitch: i16,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ServerDatagram {
    /// World state at `tick`. Large player counts split one tick across
    /// several snapshots; `you` is repeated in each.
    Snapshot {
        tick: u64,
        /// Highest input sequence applied to `you`.
        ack: u32,
        you: PlayerState,
        others: Vec<RemotePlayer>,
    },
}

/// Remote players per snapshot datagram, keeping each under the ~1200-byte
/// path MTU QUIC assumes.
pub const PLAYERS_PER_SNAPSHOT: usize = 40;

#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("frame of {0} bytes exceeds the limit")]
    TooLarge(usize),
    #[error("malformed message: {0}")]
    Malformed(#[from] postcard::Error),
}

pub fn encode<T: Serialize>(msg: &T) -> Vec<u8> {
    postcard::to_stdvec(msg).expect("protocol types always serialize")
}

pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, DecodeError> {
    if bytes.len() > MAX_FRAME {
        return Err(DecodeError::TooLarge(bytes.len()));
    }
    Ok(postcard::from_bytes(bytes)?)
}

/// A length-prefixed frame for the reliable stream.
pub fn encode_frame<T: Serialize>(msg: &T) -> Vec<u8> {
    let body = encode(msg);
    let mut out = Vec::with_capacity(4 + body.len());
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

/// Cleans a player-chosen name: printable, trimmed, bounded.
pub fn clean_name(name: &str) -> Option<String> {
    let s: String = name
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '_' || *c == '-')
        .take(MAX_NAME_LEN)
        .collect();
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    (s.chars().count() >= 2).then_some(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::DVec3;

    #[test]
    fn messages_round_trip() {
        let msgs = vec![
            ServerMsg::Chunk {
                pos: ChunkPos::new(-1, 2, 3),
                data: vec![1, 2, 3],
            },
            ServerMsg::EditResult {
                seq: 9,
                pos: BlockPos::new(1, 2, 3),
                block: BlockId(4),
                accepted: false,
            },
        ];
        for m in msgs {
            assert_eq!(decode::<ServerMsg>(&encode(&m)).unwrap(), m);
        }
    }

    #[test]
    fn full_snapshot_fits_a_datagram() {
        let snap = ServerDatagram::Snapshot {
            tick: u64::MAX,
            ack: u32::MAX,
            you: PlayerState {
                pos: DVec3::new(1e6, 1e3, 1e6),
                vel: DVec3::splat(-50.0),
                on_ground: true,
            },
            others: vec![
                RemotePlayer {
                    id: u32::MAX,
                    pos: [1e6; 3],
                    yaw: u16::MAX,
                    pitch: i16::MIN,
                };
                PLAYERS_PER_SNAPSHOT
            ],
        };
        let n = encode(&snap).len();
        assert!(n <= 1150, "{n} bytes");
    }

    #[test]
    fn redundant_inputs_are_small() {
        let d = ClientDatagram::Input {
            latest: 1_000_000,
            inputs: vec![
                PlayerInput {
                    forward: 1,
                    strafe: -1,
                    jump: true,
                    sprint: true,
                    yaw: 65535,
                    pitch: -16000,
                };
                INPUT_REDUNDANCY
            ],
        };
        assert!(encode(&d).len() < 80);
    }

    #[test]
    fn garbage_does_not_panic() {
        for i in 0..2000u32 {
            let bytes: Vec<u8> = (0..(i % 64)).map(|j| (i.wrapping_mul(31) ^ (j * 17)) as u8).collect();
            let _ = decode::<ServerMsg>(&bytes);
            let _ = decode::<ClientMsg>(&bytes);
            let _ = decode::<ClientDatagram>(&bytes);
        }
    }

    #[test]
    fn names_are_cleaned() {
        assert_eq!(clean_name("  Pen  dragon\n"), Some("Pen dragon".into()));
        assert_eq!(clean_name("<script>"), Some("script".into()));
        assert_eq!(clean_name("x"), None);
        assert_eq!(clean_name(&"a".repeat(100)).unwrap().len(), MAX_NAME_LEN);
    }
}
