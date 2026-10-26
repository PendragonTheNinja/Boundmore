//! Doors and trapdoors (milestone 22 part 2): E swings a door and every door
//! block joined to it; a lock beside a door on a claim keeps it shut to all
//! but the owner. What the blocks look like and stop is their shape
//! (`bm_core::doors`).

use super::*;
use bm_core::doors::{Side, swung};

/// The most door blocks one swing moves.
const MAX_DOOR: usize = 64;

impl Game {
    /// `ClientMsg::Swing`: opens or shuts the door at `pos`.
    pub(super) fn swing_door(&mut self, id: u32, pos: BlockPos) {
        let Some(p) = self.players.get(&id) else { return };
        let pi = p.planet;
        let planet = &self.planets[pi];
        let shape = planet.world.shape;
        if !shape.contains_y(pos.y) {
            return;
        }
        let canon = shape.wrap_block(pos);
        let reach = self.cfg.reach + p.effects.reach_bonus();
        if !p.sent.contains(&canon.chunk()) || !in_reach(shape, &self.movement, reach, &p.state, canon) {
            return;
        }
        let Some(state) = planet.world.state(canon) else { return };
        let reg = &planet.world.registry;
        let def = reg.get(state.id);
        let (Some(swing), Some(toggle)) = (def.swing, def.toggle.as_deref()) else {
            return;
        };
        let Ok(toggle) = reg.id(toggle) else { return };
        let opening = def.solid;
        let claim = planet.claims.at_block(canon);
        let claim_id = claim.map(|c| c.id);
        // The joined door: same block, same shape, touching by faces, on
        // the same claim (Boundless never swung across a claim border).
        let mut door = vec![canon];
        let mut i = 0;
        while i < door.len() && door.len() < MAX_DOOR {
            let at = door[i];
            i += 1;
            for (dx, dy, dz) in [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)] {
                let n = at.offset(dx, dy, dz);
                if !shape.contains_y(n.y) {
                    continue;
                }
                let n = shape.wrap_block(n);
                if door.contains(&n) || door.len() >= MAX_DOOR {
                    continue;
                }
                if planet.world.state(n) == Some(state) && planet.claims.at_block(n).map(|c| c.id) == claim_id {
                    door.push(n);
                }
            }
        }
        // A lock beside it on someone else's claim.
        if let Some(c) = claim
            && c.owner != p.account
        {
            let locked = door.iter().any(|b| {
                [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)]
                    .iter()
                    .any(|&(dx, dy, dz)| planet.world.block(shape.wrap_block(b.offset(dx, dy, dz))) == Some(self.lock))
            });
            if locked {
                return self.notice(id, "This door is locked");
            }
        }
        // Never shut on anyone standing in it.
        if !opening {
            let m = self.movement;
            let blocked = self
                .players
                .values()
                .filter(|o| o.planet == pi)
                .any(|o| door.iter().any(|b| o.state.overlaps_block(&m, *b)));
            if blocked {
                return self.notice(id, "Someone is in the way");
            }
        }
        let centre = DVec3::new(canon.x as f64 + 0.5, canon.y as f64 + 0.5, canon.z as f64 + 0.5);
        let d = shape.delta(centre, p.state.pos);
        let opener = Side::towards(d.x, d.z);
        let new = BlockState {
            id: toggle,
            colour: state.colour,
            shape: swung(swing, state.shape, opening, opener),
        };
        for b in door {
            self.change_block(pi, b, new, None);
        }
    }
}
