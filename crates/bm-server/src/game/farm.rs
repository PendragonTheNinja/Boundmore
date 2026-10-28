//! Farming in the game loop (milestone 16 part 2, ADR-0010): sowing,
//! growth, catching up as chunks load, harvesting with replanting, tilling,
//! and telling a player about the crop they look at.

use super::*;
use bm_core::TICK_HZ;
use bm_core::crops::{self, Crop, STAGES};

/// Stage changes are looked at once a second: a crop's next change is
/// minutes away at the soonest.
pub(super) const FARM_EVERY: u64 = TICK_HZ as u64;

impl Game {
    /// Whether a seed of crop `kind` may be sown at `canon` by a character
    /// with `farming` level (`check_edit`): on its ground, with the level.
    pub(super) fn sowing_allowed(
        crops: &bm_core::crops::CropKinds,
        kind: u16,
        world: &crate::world::World,
        canon: BlockPos,
        farming: u8,
    ) -> Result<(), Refusal> {
        let def = crops.get(kind).ok_or("no such crop")?;
        if farming < def.farming {
            return Err(Refusal::Told(format!(
                "Sowing {} needs Farming {}",
                def.display, def.farming
            )));
        }
        let ground = world.block(canon.offset(0, -1, 0));
        if !ground.is_some_and(|g| def.ground.contains(&g)) {
            return Err(Refusal::Told("Sow seeds on soil, turf or tilled soil".into()));
        }
        Ok(())
    }

    /// A block changed at `canon` (`change_block`): a crop whose block went
    /// loses its record; a seed a player just sowed gets one, with the
    /// conditions it was sown in (turf under it becomes soil, as in
    /// Boundless).
    pub(super) fn crop_block_changed(&mut self, pi: usize, canon: BlockPos, new: BlockId, by: Option<u32>) {
        let new_of = self.crops.of_block(new);
        if let Some(rec) = self.planets[pi].farm.get(canon) {
            if new_of.map(|(k, _)| k) != Some(rec.kind) {
                self.planets[pi].farm.remove(canon);
            }
            return;
        }
        let (Some(kind), Some(id)) = (self.crops.sown_as(new), by) else {
            return;
        };
        let below = canon.offset(0, -1, 0);
        let world = &self.planets[pi].world;
        let ground = world.block(below);
        let tilled = ground == Some(self.tilled_soil);
        let water = world.water;
        let wet = [(1, 0, 0), (-1, 0, 0), (0, 0, 1), (0, 0, -1)]
            .iter()
            .any(|&(dx, dy, dz)| {
                world.block(canon.offset(dx, dy, dz)) == Some(water)
                    || world.block(below.offset(dx, dy, dz)) == Some(water)
            });
        let epic = self.players.get(&id).is_some_and(|p| p.effects.farming_epic);
        if let Some(&(_, soil)) = self.grass_soils.iter().find(|(g, _)| Some(*g) == ground) {
            let soil = BlockState::plain(soil);
            self.change_block(pi, self.planets[pi].world.shape.wrap_block(below), soil, None);
        }
        let crop = Crop {
            kind,
            sown_at: self.tick,
            fertilised_at: None,
            tilled,
            wet,
            epic,
        };
        self.planets[pi].farm.put(canon, crop, self.tick, &self.crops);
        self.add_xp(id, crops::SOW_XP);
    }

    /// Chunks that just loaded: their crops catch up to now (ADR-0010).
    /// Nobody holds a chunk that has just arrived, so its blocks change
    /// quietly; a record whose block is gone is dropped.
    pub(super) fn catch_up_crops(&mut self) {
        let now = self.tick;
        for p in &mut self.planets {
            for chunk in p.world.take_arrived() {
                for (pos, crop) in p.farm.in_chunk(chunk) {
                    let Some(def) = self.crops.get(crop.kind) else { continue };
                    let Some(state) = p.world.state(pos) else { continue };
                    if self.crops.of_block(state.id).map(|(k, _)| k) != Some(crop.kind) {
                        p.farm.remove(pos);
                        continue;
                    }
                    let want = def.stages[crop.stage(def, now) as usize - 1];
                    if state.id != want {
                        p.world.set_state(pos, BlockState::plain(want));
                    }
                    p.farm.schedule(pos, crop, now, &self.crops);
                }
            }
        }
    }

    /// Crops whose stage changes now, in chunks in memory: their blocks
    /// change for everyone who has them.
    pub(super) fn step_farms(&mut self) {
        let now = self.tick;
        for pi in 0..self.planets.len() {
            for pos in self.planets[pi].farm.take_due(now) {
                let Some(crop) = self.planets[pi].farm.get(pos) else {
                    continue;
                };
                let Some(def) = self.crops.get(crop.kind) else { continue };
                // Not in memory: it catches up when its chunk loads.
                let Some(state) = self.planets[pi].world.state(pos) else {
                    continue;
                };
                if self.crops.of_block(state.id).map(|(k, _)| k) != Some(crop.kind) {
                    continue;
                }
                let want = def.stages[crop.stage(def, now) as usize - 1];
                if state.id != want {
                    self.change_block(pi, pos, BlockState::plain(want), None);
                }
                self.planets[pi].farm.schedule(pos, crop, now, &self.crops);
            }
        }
    }

    /// A ripe crop with a record broken by player `id`: its seeds (one kept
    /// to replant it) and yield into their pack (as far as they fit), XP, and
    /// it starts again from seed with the conditions it had. Returns false
    /// when it is not such a crop (the caller breaks it as a block).
    pub(super) fn harvest(
        &mut self,
        id: u32,
        pi: usize,
        seq: u32,
        reply: BlockPos,
        canon: BlockPos,
        block: BlockId,
    ) -> bool {
        let Some((kind, stage)) = self.crops.of_block(block) else {
            return false;
        };
        let Some(rec) = self.planets[pi]
            .farm
            .get(canon)
            .filter(|r| r.kind == kind && stage == STAGES)
        else {
            return false;
        };
        let Some(def) = self.crops.get(kind).cloned() else {
            return false;
        };
        let Some(p) = self.players.get_mut(&id) else {
            return false;
        };
        let rng = &mut self.rng;
        let (seeds, yields) = crops::harvest(&def, rec.tilled, p.effects.farming_epic, || mining::roll(rng));
        let seed = BlockState::plain(def.stages[0]);
        p.out.send(&ServerMsg::EditResult {
            seq,
            pos: reply,
            block: seed,
            accepted: true,
        });
        let before = p.inventory.clone();
        // One seed goes back into the ground; what does not fit is lost.
        p.inventory.add(&self.items, def.seed, seeds - 1);
        p.inventory.add(&self.items, def.yields, yields);
        send_slots(p, &before);
        let replanted = Crop {
            sown_at: self.tick,
            fertilised_at: None,
            ..rec
        };
        self.planets[pi].farm.put(canon, replanted, self.tick, &self.crops);
        self.stats.edits += 1;
        self.change_block(pi, canon, seed, Some(id));
        self.add_xp(id, def.harvest_xp());
        true
    }

    /// A tiller swung at `canon`: any soil or grass with room above becomes tilled
    /// soil, for good. Anything else is refused (crops are harvested by the
    /// ordinary swing, before this is asked). Returns whether it tilled.
    pub(super) fn till(&mut self, id: u32, pi: usize, seq: u32, pos: BlockPos, canon: BlockPos) -> Result<(), Refusal> {
        let world = &self.planets[pi].world;
        let block = world.block(canon).ok_or("chunk not resident")?;
        if !self.grass_soils.iter().any(|&(g, s)| block == g || block == s) {
            return Err(Refusal::Told("A tiller tills soil and turf".into()));
        }
        let above = world.block(canon.offset(0, 1, 0));
        if !above.is_some_and(|b| b.is_air()) {
            return Err(Refusal::Told("Clear the block above first".into()));
        }
        let tilled = BlockState::plain(self.tilled_soil);
        if let Some(p) = self.players.get(&id) {
            p.out.send(&ServerMsg::EditResult {
                seq,
                pos,
                block: tilled,
                accepted: true,
            });
        }
        self.stats.edits += 1;
        self.change_block(pi, canon, tilled, Some(id));
        Ok(())
    }

    /// A tiller swung at `canon` (not a crop): the claim, the cadence and
    /// the energy, then tilling (`till`), and one use of the tiller.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn tiller_swing(
        &mut self,
        id: u32,
        seq: u32,
        at: u32,
        pos: BlockPos,
        canon: BlockPos,
        slot: u8,
        tool: bm_core::items::ToolStats,
    ) {
        let Some(p) = self.players.get(&id) else { return };
        let pi = p.planet;
        let effects = p.effects.brewed(&p.state);
        if let Err(r) = self.may(pi, canon, &p.account, bm_core::rights::Act::Farm) {
            return self.swing_refused(id, seq, pos, r);
        }
        let p = self.players.get_mut(&id).expect("present");
        p.last_swing = Some((at, bm_core::mining::swing_ticks(tool.action_speed)));
        if !p.state.try_spend(tool.energy, bm_core::food::Drain::Tool) {
            return self.swing_refused(id, seq, pos, Refusal::Quiet("too tired"));
        }
        if let Err(r) = self.till(id, pi, seq, pos, canon) {
            return self.swing_refused(id, seq, pos, r);
        }
        self.wear_swung_tool(id, slot, &effects);
    }

    /// What player `id` is told about the crop at `pos`: its stage and time
    /// to ripe, or that it is wild.
    pub(super) fn look_crop(&self, id: u32, pos: BlockPos) {
        let Some(p) = self.players.get(&id) else { return };
        let planet = &self.planets[p.planet];
        let shape = planet.world.shape;
        if !shape.contains_y(pos.y) {
            return;
        }
        let canon = shape.wrap_block(pos);
        let Some(block) = planet.world.block(canon) else { return };
        let Some((kind, stage)) = self.crops.of_block(block) else {
            return;
        };
        let Some(def) = self.crops.get(kind) else { return };
        let rec = planet.farm.get(canon);
        let ripe_in = rec.map_or(0, |c| c.ticks_to_ripe(def, self.tick) / TICK_HZ as u64) as u32;
        p.out.send(&ServerMsg::CropInfo {
            pos,
            name: def.display.clone(),
            stage,
            stages: STAGES,
            ripe_in,
            wild: rec.is_none(),
            tilled: rec.is_some_and(|c| c.tilled),
            dry: rec.is_some_and(|c| def.water && !c.wet),
        });
    }
}
