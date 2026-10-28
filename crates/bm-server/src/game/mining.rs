//! Breaking blocks the Boundless way (milestone 9): swings on the input
//! timeline, damage that heals, areas, bonus yields by tier, and props
//! (torches) that fall with what held them up. The rules themselves are
//! pure functions in `bm_core::mining`; this applies them.

use super::{Game, Refusal, in_reach, send_slots, swing_tool_of, wear_slot};
use crate::world::World;
use bm_core::forge::Held;
use bm_core::items::ToolClass;
use bm_core::mining::{
    MAX_TIER, aoe_blocks, block_health, bonus_count, is_crit, skilled_tool, skilled_tool_keen, swing_damage,
    swing_ticks,
};
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
/// bonus that rolled at least one, with its count. `luck` (the breaker's
/// `Effects::luck_multiplier`, 1 untrained) multiplies the chance part of
/// each bonus, capped at 1 [tuning 2.8]; the whole part is always given.
pub(super) fn bonus_yield(
    items: &ItemRegistry,
    block: BlockId,
    tier: u8,
    luck: f64,
    rng: &mut u64,
) -> Vec<(ItemId, u16)> {
    let t = (tier.clamp(1, MAX_TIER) - 1) as usize;
    items
        .bonus_of(block)
        .iter()
        .map(|(item, per_tier)| (*item, bonus_count(lucky(per_tier[t], luck), roll(rng))))
        .filter(|&(_, n)| n > 0)
        .collect()
}

/// An expected bonus count with its chance part multiplied by `luck`,
/// capped at a certainty.
fn lucky(expected: f32, luck: f64) -> f32 {
    let e = expected.max(0.0) as f64;
    let whole = e.floor();
    (whole + ((e - whole) * luck).min(1.0)) as f32
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

    pub(super) fn swing_refused(&mut self, id: u32, seq: u32, pos: BlockPos, why: Refusal) {
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
        if let Some(r) = self.meteor_guards(pi, canon) {
            return Err(r);
        }
        // Rights there (milestone 25): the block's right, and only its
        // owner breaks a claim stone.
        if let Some(c) = planet.claims.at_block(canon) {
            self.may_on(c, &p.account, self.act_for_block(pi, state.id))?;
            if state.id == self.claim_stone && c.owner != p.account {
                return Err(Refusal::Told(format!("Only {} can remove this claim", c.owner_name)));
            }
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
                def.display_name()
            )));
        }
        if planet.shops.till(canon) > 0 {
            return Err(Refusal::Told(format!(
                "Take the coin from the {} before breaking it",
                def.display_name()
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
        if p.state.down {
            return self.swing_refused(id, seq, pos, "defeated".into());
        }
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
        if !in_reach(
            shape,
            &self.movement,
            self.cfg.reach + p.effects.reach_bonus(),
            &p.state,
            canon,
        ) {
            return self.swing_refused(id, seq, pos, "out of reach".into());
        }
        let effects = p.effects.brewed(&p.state);
        // The tool is what our copy of the slot holds. A bare hand breaks
        // nothing (Nathan, 2026-10-20: as in Boundless).
        let stack = p.inventory.get(slot as usize);
        // Through its traits (milestone 17), as the holder stands now.
        let held = Held::of(&p.state, &self.planets[pi].day, self.tick as f64);
        // A tiller tills soil and turf; at a crop it harvests like any tool.
        if let Some(tool) = swing_tool_of(&self.items, stack, &held).filter(|t| t.class == Some(ToolClass::Tiller))
            && !self.planets[pi]
                .world
                .block(canon)
                .is_some_and(|b| self.crops.of_block(b).is_some())
        {
            return self.tiller_swing(id, seq, at, pos, canon, slot, tool);
        }
        let spanner = swing_tool_of(&self.items, stack, &held).filter(|t| t.class == Some(ToolClass::Spanner));
        if let Some(tool) = spanner {
            return self.spanner_swing(id, seq, at, pos, canon, slot, tool);
        }
        if let Err(r) = self.may_damage(id, pi, canon) {
            return self.swing_refused(id, seq, pos, r);
        }
        // Creative mode (a testing aid): anything breakable breaks in one
        // swing of anything, at no cost to tool or energy.
        if p.state.body.creative {
            let Some(state) = self.planets[pi].world.state(canon) else {
                return self.swing_refused(id, seq, pos, "chunk not resident".into());
            };
            if !self.break_block(id, pi, seq, pos, canon, state) {
                self.edit_refused(id, seq, pos, "inventory full");
                self.notice(id, "Your inventory is full");
            }
            return;
        }
        let Some(tool) = swing_tool_of(&self.items, stack, &held) else {
            return self.swing_refused(id, seq, pos, Refusal::Told(NEED_A_TOOL.into()));
        };
        // The tool as this character swings it: damage, cadence and crits
        // (Keen food multiplies the crits skills add).
        let tool = skilled_tool_keen(&tool, &effects, p.state.crit_multiplier());
        let look = look_dir(p.last_input.yaw, p.last_input.pitch);
        let p = self.players.get_mut(&id).expect("present");
        p.last_swing = Some((at, swing_ticks(tool.action_speed)));
        // Energy (milestone 10): too tired, the swing does nothing but start
        // the pause again (the client predicts the same, `try_spend`).
        if !p.state.try_spend(tool.energy, bm_core::food::Drain::Tool) {
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
        self.wear_swung_tool(id, slot, &effects);
    }

    /// One durability per swing, whatever it hit, unless the durability epic
    /// spares it.
    pub(super) fn wear_swung_tool(&mut self, id: u32, slot: u8, effects: &bm_core::skills::Effects) {
        let Some(p) = self.players.get_mut(&id) else { return };
        // The durability epic and Gentle food spare some wear.
        if roll(&mut self.rng) < p.state.wear_spared(effects.wear_saved()) {
            return;
        }
        let before = p.inventory.clone();
        let broke = wear_slot(&self.items, &mut p.inventory, slot as usize);
        send_slots(p, &before);
        if let Some(text) = broke {
            p.out.send(&ServerMsg::Notice { text });
        }
    }

    /// A spanner swung at `canon` (milestone 15): on a machine that wears it
    /// removes the spanner's wear per hit (more with Spanner Mastery), at
    /// the swing's cadence, with its energy and durability; it never damages
    /// a block. Anything else is refused.
    #[allow(clippy::too_many_arguments)]
    fn spanner_swing(
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
        if let Err(r) = self.may(pi, canon, &p.account, bm_core::rights::Act::Work) {
            return self.swing_refused(id, seq, pos, r);
        }
        let is_machine = self
            .machine(pi, canon)
            .is_some_and(|m| bm_core::spark::wears(m.kind) && m.idle.is_none());
        let is_coil = {
            let w = &self.planets[pi].world;
            w.block(canon).is_some_and(|b| w.registry.get(b).powers.is_some())
        };
        if !is_machine && !is_coil {
            return self.swing_refused(
                id,
                seq,
                pos,
                Refusal::Told("A spanner repairs machines and power coils".into()),
            );
        }
        let tool = skilled_tool(&tool, &effects);
        let p = self.players.get_mut(&id).expect("present");
        p.last_swing = Some((at, swing_ticks(tool.action_speed)));
        if !p.state.try_spend(tool.energy, bm_core::food::Drain::Tool) {
            return self.swing_refused(id, seq, pos, Refusal::Quiet("too tired"));
        }
        let amount = bm_core::spark::repair_per_hit(tool.repair, effects.spanner_mastery);
        let mended = if is_coil {
            self.repair_coil(pi, canon, amount)
        } else {
            self.repair_machine(pi, canon, amount)
        };
        if let Some((pos, wear, max)) = mended
            && let Some(p) = self.players.get(&id)
        {
            p.out.send(&ServerMsg::MachineWear { pos, wear, max });
        }
        self.wear_swung_tool(id, slot, &effects);
    }

    /// Tells everyone who has the chunk how much damage the block at `canon`
    /// has taken.
    pub(super) fn tell_health(&self, pi: usize, canon: BlockPos, damage: u32) {
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
        // A ripe sown crop is harvested and replants itself (milestone 16).
        if self.harvest(id, pi, seq, reply, canon, state.id) {
            return true;
        }
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
        let tier = self.planets[pi].tier;
        let luck = p.effects.luck_multiplier();
        for (item, n) in bonus_yield(&self.items, state.id, tier, luck, &mut self.rng) {
            // What does not fit is not given.
            p.inventory.add(&self.items, item, n);
        }
        send_slots(p, &before);
        let xp = super::skills::break_xp(self.planets[pi].world.registry.get(state.id), tier);
        self.block_gone(pi, canon, state.id, Some(id));
        if state.id == self.meteor_shell_id {
            self.meteor_shell_broken(id, pi, canon);
        }
        self.add_xp(id, xp);
        // Feats (milestone 24): what was mined or gathered, and each kind
        // the first time.
        let def = self.planets[pi].world.registry.get(state.id);
        if let Some(k) = bm_core::feats::mined_key(def) {
            let first = format!("mined:{}", def.name);
            self.count(id, k, 1);
            let new_kind = self
                .players
                .get(&id)
                .and_then(|p| self.ledgers.get(&p.account))
                .is_some_and(|l| l.count(&first) == 0);
            if new_kind {
                self.count(id, &first, 1);
                self.count(id, "mined_kinds", 1);
            }
        }
        true
    }

    /// Everything a broken block takes with it, after its yield: the block
    /// becomes air for everyone, a claim stone's claim goes, a storage block
    /// closes, a conduit's portal goes, and props it held fall.
    pub(super) fn block_gone(&mut self, pi: usize, canon: BlockPos, old: BlockId, by: Option<u32>) {
        let storage_gone = self.planets[pi].world.registry.get(old).storage > 0;
        let claim_removed = if old == self.claim_stone {
            self.planets[pi].claims.remove_by_stone(canon)
        } else {
            self.prestige_removed(pi, canon, old);
            None
        };
        if let Some(claim) = claim_removed {
            self.claim_forgotten(claim);
        }
        // A machine block going: its machine as it stood.
        let def = self.planets[pi].world.registry.get(old);
        let machine = if def.station.is_some() || def.assembly.is_some() {
            self.machine(pi, canon)
        } else {
            None
        };
        self.stats.edits += 1;
        self.change_block(pi, canon, BlockState::AIR, by);
        if let Some(m) = machine {
            self.machine_broken(pi, m, canon, by);
        }
        if storage_gone {
            self.close_storage_at(pi, canon);
        }
        if old == self.guild_control {
            self.control_broken(pi, canon);
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
            let luck = by
                .and_then(|id| self.players.get(&id))
                .map_or(1.0, |p| p.effects.luck_multiplier());
            let bonus = bonus_yield(&self.items, s.id, self.planets[pi].tier, luck, &mut self.rng);
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

    /// Mean count of `item` from `n` breaks of `block` on `tier` (other bonus
    /// drops ignored).
    fn mean(items: &ItemRegistry, block: BlockId, tier: u8, item: ItemId, n: u32) -> f64 {
        let mut rng = 12345;
        let mut total = 0u32;
        for _ in 0..n {
            for (i, c) in bonus_yield(items, block, tier, 1.0, &mut rng) {
                if i == item {
                    total += c as u32;
                }
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
            let y = bonus_yield(&items, vein, 5, 1.0, &mut rng);
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
        assert!(bonus_yield(&items, b.id("soil").unwrap(), 8, 1.0, &mut rng).is_empty());
    }

    #[test]
    fn luck_multiplies_bonus_chances_up_to_a_certainty() {
        // Tuning 2.8: a gem seam's +0.5 at max luck (x2.8) is one more, never
        // more than certain; the whole part is untouched.
        assert_eq!(lucky(0.5, 2.8), 1.0);
        assert!((lucky(0.05, 2.8) - 0.14).abs() < 1e-6);
        assert!((lucky(1.05, 2.8) - 1.14).abs() < 1e-6);
        assert_eq!(lucky(2.0, 2.8), 2.0);
        assert_eq!(lucky(0.3, 1.0), 0.3);
        let b = BlockRegistry::builtin();
        let items = ItemRegistry::builtin(&b);
        let flint = items.id("flint").unwrap();
        let gravel = b.id("gravel").unwrap();
        let mut rng = 7;
        let n = 20_000;
        let found: u32 = (0..n)
            .flat_map(|_| bonus_yield(&items, gravel, 1, 2.8, &mut rng))
            .map(|(i, c)| {
                assert_eq!(i, flint);
                c as u32
            })
            .sum();
        let mean = found as f64 / n as f64;
        assert!((0.125..0.155).contains(&mean), "5% x 2.8: {mean}");
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
