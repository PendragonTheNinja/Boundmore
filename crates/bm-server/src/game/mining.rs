//! Breaking blocks the Boundless way (milestone 9): swings on the input
//! timeline, damage that heals, areas, bonus yields by tier, and props
//! (torches) that fall with what held them up. The rules themselves are
//! pure functions in `bm_core::mining`; this applies them.

use super::{Game, Refusal, in_reach, send_slots, wear_slot};
use crate::claims::ClaimError;
use crate::world::World;
use bm_core::items::title_case;
use bm_core::mining::{MAX_TIER, aoe_blocks, block_health, bonus_count, is_crit, swing_damage, swing_ticks};
use bm_core::movement::look_dir;
use bm_core::{BlockId, BlockPos, BlockState, Colour, HOTBAR_SLOTS, ItemId, ItemRegistry, Mount, Shape};
use bm_protocol::ServerMsg;

/// Said when a swing is made with no tool in hand.
pub(crate) const NEED_A_TOOL: &str = "You need a tool to break blocks. A hand stone needs nothing to craft";

/// Damaged blocks are swept for healing this often (once a second); reads
/// treat a healed block as whole in between.
pub(super) const HEAL_SWEEP_EVERY: u64 = 60;

/// Whether the block at `pos` can hold a prop: solid and a full cube. An
/// unloaded block cannot (nothing is placed against what is not resident).
pub(super) fn holds_props(world: &World, pos: BlockPos) -> bool {
    world
        .state(pos)
        .is_some_and(|s| world.flags.solid(s.id) && s.shape == Shape::FULL)
}

/// What breaking `state` yields into a pack: its drop, in the block's colour
/// when the block yields itself.
fn drop_of(game: &Game, pi: usize, state: BlockState) -> Option<(ItemId, Colour)> {
    let def = game.planets[pi].world.registry.get(state.id);
    game.items.drop_of(state.id).map(|item| {
        let own = game.items.item_for_block(state.id) == Some(item);
        (
            item,
            if own && def.colourable {
                state.colour
            } else {
                Colour::NATURAL
            },
        )
    })
}

/// splitmix64: the server's dice for bonus yields, a number in 0..1 (only
/// the server rolls them, so they need not match anything).
pub(super) fn roll(state: &mut u64) -> f64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^= z >> 31;
    (z >> 11) as f64 / (1u64 << 53) as f64
}

/// What breaking `block` on a planet of `tier` yields beyond its drop: each
/// bonus that rolled at least one, with its count.
pub(super) fn bonus_yield(items: &ItemRegistry, block: BlockId, tier: u8, rng: &mut u64) -> Vec<(ItemId, u16)> {
    let t = (tier.clamp(1, MAX_TIER) - 1) as usize;
    items
        .bonus_of(block)
        .iter()
        .map(|(item, per_tier)| (*item, bonus_count(per_tier[t], roll(rng))))
        .filter(|&(_, n)| n > 0)
        .collect()
}

impl Game {
    /// Sends one player the true damage of the block at `pos` (after a
    /// refused swing, so their prediction gives way).
    pub(super) fn send_health(&self, id: u32, pos: BlockPos) {
        let Some(p) = self.players.get(&id) else { return };
        let pi = p.planet;
        let world = &self.planets[pi].world;
        if !world.shape.contains_y(pos.y) {
            return;
        }
        let canon = world.shape.wrap_block(pos);
        let damage = world
            .state(canon)
            .map_or(0, |s| self.damage[pi].get(canon, s, self.tick));
        p.out.send(&ServerMsg::BlockHealth { pos: canon, damage });
    }

    fn swing_refused(&mut self, id: u32, seq: u32, pos: BlockPos, why: Refusal) {
        let text = match why {
            Refusal::Quiet(why) => why.to_string(),
            Refusal::Told(text) => {
                self.notice(id, &text);
                text
            }
        };
        self.edit_refused(id, seq, pos, &text);
        self.send_health(id, pos);
    }

    /// Whether this player's swing may damage the block at `canon` (already
    /// wrapped): something breakable, not guarded, not on someone else's
    /// claim, and nothing that must be emptied first.
    fn may_damage(&self, id: u32, pi: usize, canon: BlockPos) -> Result<BlockState, Refusal> {
        let p = self.players.get(&id).ok_or("no such player")?;
        let planet = &self.planets[pi];
        let state = planet.world.state(canon).ok_or("chunk not resident")?;
        if planet.guarded(canon) {
            return Err(Refusal::Told(
                "The ground around a warp shrine cannot be changed".into(),
            ));
        }
        if let Some(c) = planet.claims.at_block(canon)
            && c.owner != p.account
        {
            return Err(ClaimError::OwnedBy(c.owner_name.clone()).into());
        }
        let def = planet.world.registry.get(state.id);
        if state.id.is_air() || !def.breakable {
            return Err("cannot break that".into());
        }
        // A machine's contents belong to all its blocks.
        if let Some(name) = self.machine_contents(pi, canon) {
            return Err(Refusal::Told(format!("Empty the {name} before breaking it")));
        }
        if planet.storage.has_contents(canon) {
            return Err(Refusal::Told(format!(
                "Empty the {} before breaking it",
                title_case(&def.name)
            )));
        }
        if let Some(pid) = self.portals.at(planet.id, canon)
            && self.portals.get(pid).is_some_and(|r| r.fuel > 0)
        {
            return Err(Refusal::Told("Empty the portal's tank before breaking it".into()));
        }
        Ok(state)
    }

    /// One swing of the item in hotbar `slot` at `pos`, following input
    /// `at` (applied right after it, so the look is that input's).
    pub(super) fn swing(&mut self, id: u32, seq: u32, at: u32, pos: BlockPos, slot: u8) {
        let Some(p) = self.players.get(&id) else { return };
        let pi = p.planet;
        // Cadence on the input timeline: no slack, no wall clock.
        if let Some((last, ticks)) = p.last_swing
            && (at as u64) < last as u64 + ticks as u64
        {
            return self.swing_refused(id, seq, pos, "swinging too fast".into());
        }
        if slot as usize >= HOTBAR_SLOTS {
            return self.swing_refused(id, seq, pos, "not a hotbar slot".into());
        }
        let shape = self.planets[pi].world.shape;
        if !shape.contains_y(pos.y) {
            return self.swing_refused(id, seq, pos, "outside the planet".into());
        }
        let canon = shape.wrap_block(pos);
        if !p.sent.contains(&canon.chunk()) {
            return self.swing_refused(id, seq, pos, "chunk not loaded for this player".into());
        }
        if !in_reach(shape, &self.movement, self.cfg.reach, &p.state, canon) {
            return self.swing_refused(id, seq, pos, "out of reach".into());
        }
        if let Err(r) = self.may_damage(id, pi, canon) {
            return self.swing_refused(id, seq, pos, r);
        }
        // The tool is what our copy of the slot holds. A bare hand breaks
        // nothing (Nathan, 2026-10-20: as in Boundless).
        let item = p.inventory.get(slot as usize).map(|s| s.item);
        let Some(tool) = self.items.swing_tool(item) else {
            return self.swing_refused(id, seq, pos, Refusal::Told(NEED_A_TOOL.into()));
        };
        let look = look_dir(p.last_input.yaw, p.last_input.pitch);
        let p = self.players.get_mut(&id).expect("present");
        p.last_swing = Some((at, swing_ticks(tool.action_speed)));
        // Energy (milestone 10): too tired, the swing does nothing but start
        // the pause again (the client predicts the same, `try_spend`).
        if !p.state.try_spend(tool.energy) {
            return self.swing_refused(id, seq, pos, Refusal::Quiet("too tired"));
        }
        let tier = self.planets[pi].tier;
        let area = tool.aoe.map_or(1.0, |a| a.multiplier());
        let mut full = false;
        for (k, b) in aoe_blocks(tool.aoe, canon, look, id, at).into_iter().enumerate() {
            let hit = k == 0;
            if !shape.contains_y(b.y) {
                continue;
            }
            let b = shape.wrap_block(b);
            let state = if hit {
                // Checked above.
                match self.planets[pi].world.state(b) {
                    Some(s) => s,
                    None => continue,
                }
            } else {
                // The rest of the shape skips what it may not touch.
                if !self.players[&id].sent.contains(&b.chunk()) {
                    continue;
                }
                match self.may_damage(id, pi, b) {
                    Ok(s) => s,
                    Err(_) => continue,
                }
            };
            let def = self.planets[pi].world.registry.get(state.id);
            let crit = is_crit(tool.crit_chance, id, at, b);
            let dealt = swing_damage(&tool, def, tier, crit, area);
            let health = block_health(def, tier);
            let total = self.damage[pi].get(b, state, self.tick).saturating_add(dealt);
            let reply = if hit { pos } else { b };
            if total >= health {
                if self.break_block(id, pi, seq, reply, b, state) {
                    continue;
                }
                // No room for what it yields: it stands, one point short.
                full = true;
                self.edit_refused(id, seq, reply, "inventory full");
                self.damage[pi].set(b, state, health - 1, self.tick);
                self.tell_health(pi, b, health - 1);
            } else {
                self.damage[pi].set(b, state, total, self.tick);
                self.tell_health(pi, b, total);
            }
        }
        if full {
            self.notice(id, "Your inventory is full");
        }
        // One durability per swing, whatever it hit.
        let p = self.players.get_mut(&id).expect("present");
        let before = p.inventory.clone();
        let broke = wear_slot(&self.items, &mut p.inventory, slot as usize);
        send_slots(p, &before);
        if let Some(text) = broke {
            p.out.send(&ServerMsg::Notice { text });
        }
    }

    /// Tells everyone who has the chunk how much damage the block at `canon`
    /// has taken.
    fn tell_health(&self, pi: usize, canon: BlockPos, damage: u32) {
        let chunk = canon.chunk();
        for o in self
            .players
            .values()
            .filter(|o| o.planet == pi && o.sent.contains(&chunk))
        {
            o.out.send(&ServerMsg::BlockHealth { pos: canon, damage });
        }
    }

    /// Breaks the block `state` at `canon` for player `id`, whose swing `seq`
    /// named it `reply`: its yield into their pack (false, and nothing
    /// happens, when it does not fit), bonus yields by tier as far as they
    /// fit, and everything a block going does (claims, storage, portals,
    /// props it held up).
    fn break_block(
        &mut self,
        id: u32,
        pi: usize,
        seq: u32,
        reply: BlockPos,
        canon: BlockPos,
        state: BlockState,
    ) -> bool {
        let drop = drop_of(self, pi, state);
        let Some(p) = self.players.get_mut(&id) else {
            return false;
        };
        if let Some((item, colour)) = drop
            && !p.inventory.can_fit_coloured(&self.items, item, colour, 1)
        {
            return false;
        }
        p.out.send(&ServerMsg::EditResult {
            seq,
            pos: reply,
            block: BlockState::AIR,
            accepted: true,
        });
        let before = p.inventory.clone();
        if let Some((item, colour)) = drop {
            // Checked to fit.
            p.inventory.add_coloured(&self.items, item, colour, 1);
        }
        for (item, n) in bonus_yield(&self.items, state.id, self.planets[pi].tier, &mut self.rng) {
            // What does not fit is not given.
            p.inventory.add(&self.items, item, n);
        }
        send_slots(p, &before);
        self.block_gone(pi, canon, state.id, Some(id));
        true
    }

    /// Everything a broken block takes with it, after its yield: the block
    /// becomes air for everyone, a claim stone's claim goes, a storage block
    /// closes, a conduit's portal goes, and props it held fall.
    fn block_gone(&mut self, pi: usize, canon: BlockPos, old: BlockId, by: Option<u32>) {
        let storage_gone = self.planets[pi].world.registry.get(old).storage > 0;
        let claim_removed = if old == self.claim_stone {
            self.planets[pi].claims.remove_by_stone(canon)
        } else {
            None
        };
        self.stats.edits += 1;
        self.change_block(pi, canon, BlockState::AIR, by);
        if storage_gone {
            self.close_storage_at(pi, canon);
        }
        // Checked to have an empty tank.
        if old == self.portal_conduit
            && let Some(pid) = self.portals.at(self.planets[pi].id, canon)
        {
            self.remove_portal(pid);
        }
        if let Some(claim) = claim_removed {
            for o in self.players.values().filter(|o| o.planet == pi) {
                o.out.send(&ServerMsg::ClaimRemoved { id: claim });
            }
        }
    }

    /// Props that hung on `support` fall when it can no longer hold them:
    /// each into `by`'s pack if it fits (else it is lost), and everyone who
    /// has the chunk is told.
    pub(super) fn drop_unsupported_props(&mut self, pi: usize, support: BlockPos, by: Option<u32>) {
        let world = &self.planets[pi].world;
        if holds_props(world, support) {
            return;
        }
        let shape = world.shape;
        let mut fallen = Vec::new();
        for m in [Mount::Floor, Mount::North, Mount::East, Mount::South, Mount::West] {
            let (dx, dy, dz) = m.support();
            let at = support.offset(-dx, -dy, -dz);
            if !shape.contains_y(at.y) {
                continue;
            }
            let at = shape.wrap_block(at);
            if let Some(s) = world.state(at)
                && world.registry.get(s.id).mount == Some(m)
            {
                fallen.push((at, s));
            }
        }
        for (at, s) in fallen {
            // A gathering object (a plant on the floor) gives what gathering
            // it would: its bonus yield (a guess; Boundless's rule for a
            // plant whose ground goes is not recorded).
            let bonus = bonus_yield(&self.items, s.id, self.planets[pi].tier, &mut self.rng);
            let drop = drop_of(self, pi, s);
            if let Some(id) = by
                && let Some(p) = self.players.get_mut(&id)
            {
                let before = p.inventory.clone();
                if let Some((item, colour)) = drop {
                    p.inventory.add_coloured(&self.items, item, colour, 1);
                }
                for (item, n) in bonus {
                    p.inventory.add(&self.items, item, n);
                }
                send_slots(p, &before);
            }
            // Everyone hears of it, the breaker too: they did not ask for it.
            self.change_block(pi, at, BlockState::AIR, None);
        }
    }

    /// Forgets damage that has healed.
    pub(super) fn heal_blocks(&mut self) {
        for d in &mut self.damage {
            d.heal(self.tick);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bm_core::BlockRegistry;

    /// Mean count of `item` from `n` breaks of `block` on `tier`.
    fn mean(items: &ItemRegistry, block: BlockId, tier: u8, item: ItemId, n: u32) -> f64 {
        let mut rng = 12345;
        let mut total = 0u32;
        for _ in 0..n {
            for (i, c) in bonus_yield(items, block, tier, &mut rng) {
                assert_eq!(i, item);
                total += c as u32;
            }
        }
        total as f64 / n as f64
    }

    #[test]
    fn seams_give_more_ore_on_harder_worlds_and_rock_gives_flint() {
        let b = BlockRegistry::builtin();
        let items = ItemRegistry::builtin(&b);
        let vein = b.id("copper_vein").unwrap();
        let ore = items.id("copper_ore").unwrap();
        let flint = items.id("flint").unwrap();
        // Tuning 4.4: +0.5 on tiers 1 and 2, +0.7 on 3 and 4, +1.05 above.
        let m1 = mean(&items, vein, 1, ore, 4000);
        assert!((0.46..0.54).contains(&m1), "{m1}");
        let m3 = mean(&items, vein, 3, ore, 4000);
        assert!((0.66..0.74).contains(&m3), "{m3}");
        let m5 = mean(&items, vein, 5, ore, 4000);
        assert!((1.03..1.07).contains(&m5), "{m5}");
        // One or two extra on tier 5, never none.
        let mut rng = 9;
        for _ in 0..200 {
            let y = bonus_yield(&items, vein, 5, &mut rng);
            assert!(matches!(y.as_slice(), [(i, 1 | 2)] if *i == ore), "{y:?}");
        }
        let gravel = mean(&items, b.id("gravel").unwrap(), 1, flint, 20_000);
        assert!((0.04..0.06).contains(&gravel), "{gravel}");
        let rock = mean(&items, b.id("stone").unwrap(), 1, flint, 20_000);
        assert!((0.01..0.02).contains(&rock), "{rock}");
        assert_eq!(
            mean(&items, b.id("stone").unwrap(), 5, flint, 1000),
            0.0,
            "no flint above tier 4"
        );
        assert!(bonus_yield(&items, b.id("soil").unwrap(), 8, &mut rng).is_empty());
    }

    #[test]
    fn rolls_spread_over_zero_to_one() {
        let mut rng = 1;
        let r: Vec<f64> = (0..10_000).map(|_| roll(&mut rng)).collect();
        assert!(r.iter().all(|x| (0.0..1.0).contains(x)));
        let low = r.iter().filter(|&&x| x < 0.25).count();
        assert!((2300..2700).contains(&low), "{low}");
    }
}
