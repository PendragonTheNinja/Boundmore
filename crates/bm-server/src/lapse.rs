//! Claim lapse (milestone 6): a claim whose owner has not visited for
//! `claim_keep_days` crumbles to ash, a plot at a time.
//!
//! Every block the owner built (anything that is neither the terrain the
//! planet's generator made there, nor what the server cut at the shrine, nor
//! air or water) becomes ash in the block's colour: its own if it has one,
//! else the palette colour nearest its painted colour (Nathan, 2026-10-19).
//! What the owner dug stays dug. Contents of stations and storage blocks go to
//! the owner's reclaim (`reclaim.rs`); the claim stone's plot goes last, and
//! then the claim is removed.
//!
//! The work follows the save rules: a plot's chunk is loaded through the
//! planet's normal loader and pinned until the save that carries its ash has
//! taken it, so the ash, the emptied storage records and the reclaimed items
//! always reach the disk in one transaction. The generated terrain to compare
//! against is made on the loader pool, never on the game thread. A lapse that
//! a restart interrupts starts again and finds the ashed plots already done
//! (ash is never turned again, and the emptied blocks hold nothing).

use bm_core::{BlockDef, BlockId, BlockRegistry, BlockState, Chunk, ChunkPos, Colour, ColourPalette, PlotPos};
use bm_worldgen::Generator;
use crossbeam_channel::Receiver;
use std::collections::VecDeque;
use std::sync::Arc;

/// What a block of a lapsing claim becomes, or None to leave it.
/// `generated` is what the generator makes there, `plaza` what the server
/// cut there for the shrine (if anything).
pub fn ash_for(
    current: BlockState,
    generated: BlockState,
    plaza: Option<BlockState>,
    registry: &BlockRegistry,
    palette: &ColourPalette,
    ash: BlockId,
    water: BlockId,
) -> Option<BlockState> {
    if current.id.is_air() || current.id == water || current.id == ash {
        return None;
    }
    if current == generated || plaza == Some(current) {
        return None;
    }
    let def = registry.get(current.id);
    // The server's own blocks (shrines) are not anyone's build.
    if !def.breakable {
        return None;
    }
    // A prop (a torch) is no block to crumble: it goes out.
    if def.is_prop() {
        return Some(BlockState::AIR);
    }
    // A chiselled block crumbles in its shape: ash takes every family.
    Some(BlockState {
        shape: current.shape,
        ..BlockState::coloured(ash, ash_colour(current.colour, def, palette))
    })
}

/// The ash colour for a block: its own colour if it has one, else the
/// palette colour nearest the colour its sides are painted.
pub fn ash_colour(colour: Colour, def: &BlockDef, palette: &ColourPalette) -> Colour {
    if colour != Colour::NATURAL {
        colour
    } else {
        palette.nearest(def.color)
    }
}

/// One lapsing claim.
pub struct Lapse {
    /// Index into the game's planets.
    pub planet: usize,
    pub claim: u32,
    /// Lowercased account name.
    pub owner: String,
    /// Seconds since the owner was last seen, when it began.
    pub away: u64,
    /// Plots still to turn, a chunk's together, the stone's plot last.
    pub plots: VecDeque<PlotPos>,
    /// The chunk being worked on (pinned in the world).
    pub chunk: Option<ChunkPos>,
    /// Its generated terrain, once made.
    pub reference: Option<Chunk>,
    reference_rx: Option<Receiver<Chunk>>,
    /// Whether any block has turned or any item come back yet.
    pub begun: bool,
    /// Whether any item has come back.
    pub reclaimed: bool,
}

impl Lapse {
    pub fn new(planet: usize, claim: u32, owner: String, away: u64, mut plots: Vec<PlotPos>, stone: PlotPos) -> Self {
        let chunk = |p: &PlotPos| p.origin().chunk();
        plots.sort_by_key(|p| (*p == stone, chunk(p), *p));
        Self {
            planet,
            claim,
            owner,
            away,
            plots: plots.into(),
            chunk: None,
            reference: None,
            reference_rx: None,
            begun: false,
            reclaimed: false,
        }
    }

    /// The chunk the next plot is in, if any plots are left.
    pub fn next_chunk(&self) -> Option<ChunkPos> {
        self.plots.front().map(|p| p.origin().chunk())
    }

    /// Starts making the generated terrain of `c` on the loader pool.
    pub fn make_reference(&mut self, generator: &Arc<Generator>, c: ChunkPos) {
        let (tx, rx) = crossbeam_channel::bounded(1);
        let g = generator.clone();
        rayon::spawn(move || {
            let _ = tx.send(g.generate(c));
        });
        self.reference = None;
        self.reference_rx = Some(rx);
    }

    /// Takes the generated terrain in if it is ready.
    pub fn receive_reference(&mut self) {
        if let Some(rx) = &self.reference_rx
            && let Ok(chunk) = rx.try_recv()
        {
            self.reference = Some(chunk);
            self.reference_rx = None;
        }
    }
}

/// How long someone was away, in words for a notice.
pub fn months_away(seconds: u64) -> String {
    const MONTH: u64 = 30 * 86_400;
    match seconds / MONTH {
        0 => "less than a month".into(),
        1 => "a month".into(),
        n => format!("{n} months"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Setup {
        reg: BlockRegistry,
        palette: ColourPalette,
        ash: BlockId,
        water: BlockId,
    }

    impl Setup {
        fn new() -> Self {
            let reg = BlockRegistry::builtin();
            Self {
                ash: reg.id("ash").unwrap(),
                water: reg.id("water").unwrap(),
                palette: ColourPalette::builtin(),
                reg,
            }
        }

        fn id(&self, name: &str) -> BlockId {
            self.reg.id(name).unwrap()
        }

        fn turn(&self, current: BlockState, generated: BlockState, plaza: Option<BlockState>) -> Option<BlockState> {
            ash_for(
                current,
                generated,
                plaza,
                &self.reg,
                &self.palette,
                self.ash,
                self.water,
            )
        }
    }

    #[test]
    fn a_torch_goes_out_rather_than_turning_to_ash() {
        let s = Setup::new();
        let torch = BlockState::plain(s.id("torch_east"));
        assert_eq!(s.turn(torch, BlockState::AIR, None), Some(BlockState::AIR));
        assert_eq!(s.turn(BlockState::AIR, BlockState::AIR, None), None, "and stays out");
    }

    #[test]
    fn a_chiselled_block_crumbles_in_its_shape() {
        let s = Setup::new();
        let timber = s.id("timber");
        let slab = bm_core::Shape::new(bm_core::ShapeFamily::Bevel, 0xf0).unwrap();
        let cut = BlockState {
            shape: slab,
            ..BlockState::coloured(timber, Colour(12))
        };
        let ash = s.turn(cut, BlockState::AIR, None).unwrap();
        assert_eq!((ash.id, ash.colour, ash.shape), (s.ash, Colour(12), slab));
        // Natural ground cut with a slope chisel is a change too.
        let stone = BlockState::plain(s.id("stone"));
        let sloped = BlockState {
            shape: bm_core::Shape::new(bm_core::ShapeFamily::Slope, 0x30).unwrap(),
            ..stone
        };
        assert_eq!(s.turn(sloped, stone, None).unwrap().shape, sloped.shape);
    }

    #[test]
    fn only_what_differs_from_the_generated_terrain_turns() {
        let s = Setup::new();
        let stone = BlockState::plain(s.id("stone"));
        let dirt = BlockState::plain(s.id("soil"));
        let water = BlockState::plain(s.water);
        // Natural terrain stays, whatever its colour.
        assert_eq!(s.turn(stone, stone, None), None);
        let tinted = BlockState::coloured(s.id("stone"), Colour(40));
        assert_eq!(s.turn(tinted, tinted, None), None);
        // Dug stays dug; water stays water.
        assert_eq!(s.turn(BlockState::AIR, stone, None), None);
        assert_eq!(s.turn(water, BlockState::AIR, None), None);
        // A block placed where something else (or nothing) was turns.
        assert!(s.turn(stone, dirt, None).is_some());
        assert!(s.turn(stone, BlockState::AIR, None).is_some());
        // The same type in another colour is a build too.
        assert!(s.turn(tinted, stone, None).is_some());
        // What the server cut at the shrine is not.
        assert_eq!(s.turn(dirt, stone, Some(dirt)), None);
        assert!(s.turn(stone, BlockState::AIR, Some(BlockState::AIR)).is_some());
        // Nor its unbreakable shrine, nor ash already.
        let shrine = BlockState::plain(s.id("warp_shrine"));
        assert_eq!(s.turn(shrine, BlockState::AIR, None), None);
        let ash = BlockState::coloured(s.ash, Colour(5));
        assert_eq!(s.turn(ash, BlockState::AIR, None), None);
    }

    #[test]
    fn ash_keeps_the_blocks_colour_or_the_nearest_to_its_paint() {
        let s = Setup::new();
        let stone = s.id("stone");
        // Coloured: exactly its colour.
        assert_eq!(
            s.turn(BlockState::coloured(stone, Colour(77)), BlockState::AIR, None),
            Some(BlockState::coloured(s.ash, Colour(77)))
        );
        // Natural: the palette colour nearest the painted side colour, never
        // natural ash. Machines and claim stones too.
        for name in ["stone", "sand", "claim_stone", "storage_chest"] {
            let id = s.id(name);
            let def = s.reg.get(id);
            let want = s.palette.nearest(def.color);
            assert_ne!(want, Colour::NATURAL);
            assert_eq!(
                s.turn(BlockState::plain(id), BlockState::AIR, None),
                Some(BlockState::coloured(s.ash, want)),
                "{name}"
            );
        }
        // Painted near a palette colour, the ash is that colour.
        let mut def = s.reg.get(stone).clone();
        def.color = s.palette.get(Colour(120)).unwrap().rgb;
        assert_eq!(ash_colour(Colour::NATURAL, &def, &s.palette), Colour(120));
    }

    #[test]
    fn the_stone_plot_goes_last_and_a_chunks_plots_together() {
        let stone = PlotPos::new(0, 0, 0);
        let plots = vec![
            PlotPos::new(4, 0, 0),
            stone,
            PlotPos::new(1, 0, 0),
            PlotPos::new(5, 0, 0),
            PlotPos::new(2, 0, 0),
        ];
        let l = Lapse::new(0, 1, "a".into(), 0, plots, stone);
        let order: Vec<_> = l.plots.iter().map(|p| p.x).collect();
        assert_eq!(order, vec![1, 2, 4, 5, 0]);
        assert_eq!(l.next_chunk(), Some(ChunkPos::new(0, 0, 0)));
    }

    #[test]
    fn time_away_reads_as_months() {
        assert_eq!(months_away(5), "less than a month");
        assert_eq!(months_away(31 * 86_400), "a month");
        assert_eq!(months_away(183 * 86_400), "6 months");
    }
}
