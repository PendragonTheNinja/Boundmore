//! Boundmore core: voxel data, coordinates, planets and the movement shared by
//! server and client. Headless by rule: nothing here may depend on graphics,
//! windowing, sockets or the filesystem.

pub mod block;
pub mod chisel;
pub mod chunk;
pub mod colours;
pub mod combat;
pub mod creatures;
pub mod coords;
pub mod energy;
pub mod inventory;
pub mod items;
pub mod light;
pub mod look;
pub mod mining;
pub mod movement;
pub mod planet;
pub mod plots;
pub mod portal;
pub mod raycast;
pub mod recipes;
pub mod shape;
pub mod skills;
pub mod time;

pub use block::{
    BlockDef, BlockFlags, BlockId, BlockRegistry, BlockState, Bonus, Chiselling, Colour, Health, Material, Model,
    Mount, RegistryError, Shape, Station,
};
pub use chunk::{CHUNK_FORMAT_VERSION, Chunk, ChunkDecodeError};
pub use colours::{ColourDef, ColourError, ColourPalette};
pub use coords::{BlockPos, CHUNK_SIZE, CHUNK_VOLUME, ChunkPos, LocalPos};
pub use inventory::{HOTBAR_SLOTS, INVENTORY_SLOTS, Inventory};
pub use items::{Aoe, ItemDef, ItemError, ItemId, ItemKind, ItemRegistry, ItemStack, SlingStats, ToolClass, ToolStats};
pub use light::{BlockSource, ChunkLight, Light, LightMap, MAX_LIGHT};
pub use look::{NO_TINT, PlanetLook};
pub use movement::{
    Body, GrappleSpec, Hook, MoveConfig, PhysicsWorld, PlayerInput, PlayerState, StepEvents, TICK_DT, TICK_HZ,
};
pub use planet::PlanetShape;
pub use plots::{PLOT_SIZE, PlotPos};
pub use portal::{FrameError, PortalAxis, PortalFrame, find_frame};
pub use raycast::{RayHit, raycast, raycast_shaped};
pub use recipes::{Recipe, RecipeBook, RecipeError};
pub use shape::{ShapeFamily, edge_pairs, octant};
pub use time::DayCycle;
