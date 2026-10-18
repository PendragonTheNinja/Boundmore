//! Boundmore core: voxel data, coordinates, planets and the movement shared by
//! server and client. Headless by rule: nothing here may depend on graphics,
//! windowing, sockets or the filesystem.

pub mod block;
pub mod chunk;
pub mod coords;
pub mod movement;
pub mod planet;
pub mod raycast;

pub use block::{BlockDef, BlockFlags, BlockId, BlockRegistry, RegistryError};
pub use chunk::{CHUNK_FORMAT_VERSION, Chunk, ChunkDecodeError};
pub use coords::{BlockPos, CHUNK_SIZE, CHUNK_VOLUME, ChunkPos, LocalPos};
pub use movement::{MoveConfig, PhysicsWorld, PlayerInput, PlayerState, TICK_DT, TICK_HZ};
pub use planet::PlanetShape;
pub use raycast::{RayHit, raycast};
