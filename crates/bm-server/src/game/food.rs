//! Eating and buckets (milestone 16, `docs/milestones/16-food.md`).

use super::*;
use bm_core::brew::{self, Brew};

/// How near a creature must be for Soothing and Taunting to reach it, blocks
/// (**guess**).
pub(super) const BREW_THREAT_RANGE: f64 = 32.0;

impl Game {
    /// Eats one of the food in hotbar `slot` right after input `at`, as the
    /// client does (`PlayerState::eat`): a refusal changes nothing, and the
    /// client, which checks the same rules first, predicted nothing.
    pub(super) fn eat(&mut self, id: u32, at: u32, slot: u8) {
        let Some(p) = self.players.get_mut(&id) else { return };
        if slot as usize >= HOTBAR_SLOTS {
            return;
        }
        let Some(food) = p
            .inventory
            .get(slot as usize)
            .and_then(|s| self.items.get(s.item))
            .and_then(|d| d.food())
        else {
            return tracing::debug!("#{id} ate nothing from slot {slot} at {at}");
        };
        if let Err(why) = p.state.eat(&food) {
            return tracing::debug!("#{id} could not eat at {at}: {why:?}");
        }
        let before = p.inventory.clone();
        p.inventory.take_one(slot as usize);
        send_slots(p, &before);
        if matches!(food.brew, Brew::Soothing | Brew::Taunting) {
            self.brew_threat(id, food.brew);
        }
    }

    /// Tells each player whose Lifesaving brew just stood them back up (it
    /// is used inside `PlayerState::take`, wherever the blow came from).
    pub(super) fn lifesaving_notices(&mut self) {
        let mut saved = Vec::new();
        for (id, p) in &mut self.players {
            if p.revive_seen > 1 && p.state.revive == 0 {
                saved.push(*id);
            }
            p.revive_seen = p.state.revive;
        }
        for id in saved {
            self.notice(id, "Your Lifesaving Brew saved you");
        }
    }

    /// Soothing and Taunting brews (milestone 23 part 2): 10000 threat taken
    /// from, or added to, every live creature within [`BREW_THREAT_RANGE`]
    /// that is already fighting (Boundless's calming and aggravating brews
    /// moved the drinker's threat rating). Soothed below nothing, a creature
    /// forgets the drinker.
    pub(super) fn brew_threat(&mut self, id: u32, brew: Brew) {
        let Some(p) = self.players.get(&id) else { return };
        let (pi, at) = (p.planet, p.state.pos);
        let shape = self.planets[pi].world.shape;
        for c in self.herds[pi]
            .list
            .iter_mut()
            .filter(|c| c.alive() && !c.threat.is_empty())
        {
            if shape.delta(at, c.state.pos).length() > BREW_THREAT_RANGE {
                continue;
            }
            if brew == Brew::Taunting {
                c.add_threat(id, brew::THREAT);
            } else if let Some(t) = c.threat.iter_mut().find(|(p, _)| *p == id) {
                t.1 -= brew::THREAT;
                if t.1 <= 0.0 {
                    c.forget(id);
                }
            }
        }
    }

    /// Fills one empty bucket from hotbar `slot` at the water block `pos`.
    /// The water stays (Boundless's did, a **guess**). The full bucket keeps
    /// the empty one's wear (fills used).
    pub(super) fn fill_bucket(&mut self, id: u32, slot: u8, pos: BlockPos) {
        let Some(p) = self.players.get(&id) else { return };
        let Some(stack) = p.inventory.get(slot as usize) else {
            return;
        };
        if !matches!(
            self.items.get(stack.item).map(|d| &d.kind),
            Some(bm_core::items::ItemKind::Bucket { .. })
        ) {
            return;
        }
        let water = self.water_bucket;
        let planet = &self.planets[p.planet];
        let shape = planet.world.shape;
        if !shape.contains_y(pos.y) {
            return;
        }
        let canon = shape.wrap_block(pos);
        if planet.world.block(canon) != Some(planet.world.water) {
            return self.notice(id, "Fill a bucket at water");
        }
        if !in_reach(
            shape,
            &self.movement,
            self.cfg.reach + p.effects.reach_bonus(),
            &p.state,
            canon,
        ) {
            return self.notice(id, "That is out of reach");
        }
        let p = self.players.get_mut(&id).expect("present");
        let before = p.inventory.clone();
        let mut full = ItemStack::new(water, 1);
        full.wear = stack.wear;
        p.inventory.take_one(slot as usize);
        if !place_stack(&self.items, &mut p.inventory, full, slot as usize) {
            p.inventory = before;
            return self.notice(id, "Your inventory is full");
        }
        send_slots(p, &before);
    }
}

/// Puts `stack` into the inventory: into `prefer` if it is empty, else
/// merged with a stack it stacks with, else the first empty slot.
pub(super) fn place_stack(items: &ItemRegistry, inv: &mut Inventory, stack: ItemStack, prefer: usize) -> bool {
    let max = items.get(stack.item).map_or(1, |d| d.stack);
    if inv.slots.get(prefer).is_some_and(|s| s.is_none()) {
        inv.slots[prefer] = Some(stack);
        return true;
    }
    for s in inv.slots.iter_mut().flatten() {
        if s.stacks_with(&stack) && s.count + stack.count <= max {
            s.count += stack.count;
            return true;
        }
    }
    if let Some(slot) = inv.slots.iter_mut().find(|s| s.is_none()) {
        *slot = Some(stack);
        return true;
    }
    false
}
