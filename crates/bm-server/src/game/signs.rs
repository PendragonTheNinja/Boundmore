//! Signs (milestone 22 part 4): writing and unwriting them, and telling the
//! players who hold their chunks. What a sign may say and where its text
//! goes are `bm_core::signs`'s; the written signs are `crate::signs`'s.

use super::*;
use bm_core::forms::{Form, FormKind};
use bm_core::signs::{MAX_PANELS, SignText, SignView};

/// The most panels one write looks over before refusing (a full 10 by 10
/// and a border).
const MAX_FLOOD: usize = 144;

impl Game {
    /// `ClientMsg::WriteSign`: writes (or, blank, unwrites) the sign at
    /// `pos`. A first write on a panel joins every panel of its kind and
    /// facing touching it in its plane, which must make a full rectangle.
    pub(super) fn write_sign(&mut self, id: u32, pos: BlockPos, text: SignText) {
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
        let Some(form) = Form::of(state.shape).filter(|f| matches!(f.kind, FormKind::Sign | FormKind::Panel)) else {
            return;
        };
        let account = p.account.clone();
        let foreign = |b: BlockPos| {
            planet
                .claims
                .at_block(b)
                .filter(|c| self.may_on(c, &account, bm_core::rights::Act::Build).is_err())
        };
        if let Some(c) = foreign(canon) {
            let text = format!("Only {} may write on signs here", c.owner_name);
            return self.notice(id, &text);
        }
        let text = match text.checked() {
            Ok(t) => t,
            Err(e) => return self.notice(id, e),
        };
        let existing = planet.signs.anchor_of(canon);
        if text.is_blank() {
            if let Some(a) = existing {
                self.unwrite_sign(pi, a);
            }
            return;
        }
        let (anchor, members, across, up) = match existing {
            Some(a) => {
                let s = planet.signs.get(a).expect("an anchor's sign");
                (a, s.members.clone(), s.view.across, s.view.up)
            }
            None if form.kind == FormKind::Sign => (canon, vec![canon], 1, 1),
            None => match self.join_panels(pi, canon, state) {
                Ok(j) => j,
                Err(e) => return self.notice(id, e),
            },
        };
        if let Some(c) = members.iter().find_map(|m| foreign(*m)) {
            let text = format!("Only {} may write on signs here", c.owner_name);
            return self.notice(id, &text);
        }
        let view = SignView { text, across, up };
        self.planets[pi].signs.write(anchor, view.clone(), members);
        self.tell_sign(pi, anchor, Some(view));
    }

    /// The panels a first write on the panel at `at` joins: the anchor, all
    /// of them, and the rectangle's size; or why they cannot be one sign.
    fn join_panels(
        &self,
        pi: usize,
        at: BlockPos,
        state: BlockState,
    ) -> Result<(BlockPos, Vec<BlockPos>, u8, u8), &'static str> {
        let planet = &self.planets[pi];
        let shape = planet.world.shape;
        let (rx, _, rz) = bm_core::signs::panel_right(state.shape).ok_or("not a sign panel")?;
        let joins = |b: BlockPos| {
            planet
                .world
                .state(b)
                .is_some_and(|s| s.id == state.id && s.shape == state.shape)
                && planet.signs.anchor_of(b).is_none()
        };
        // Each panel with its place: i across (to the right as read), j up.
        let mut found: Vec<(BlockPos, i64, i64)> = vec![(at, 0, 0)];
        let mut k = 0;
        while k < found.len() {
            let (b, i, j) = found[k];
            k += 1;
            for (di, dj) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let raw = b.offset(rx * di, dj, rz * di);
                if !shape.contains_y(raw.y) {
                    continue;
                }
                let n = shape.wrap_block(raw);
                if found.iter().any(|(f, _, _)| *f == n) || !joins(n) {
                    continue;
                }
                if found.len() >= MAX_FLOOD {
                    return Err("Sign panels join into at most 10 by 10");
                }
                found.push((n, i + di, j + dj));
            }
        }
        let (i0, i1) = (
            found.iter().map(|f| f.1).min().unwrap_or(0),
            found.iter().map(|f| f.1).max().unwrap_or(0),
        );
        let (j0, j1) = (
            found.iter().map(|f| f.2).min().unwrap_or(0),
            found.iter().map(|f| f.2).max().unwrap_or(0),
        );
        let (across, up) = (i1 - i0 + 1, j1 - j0 + 1);
        if across > MAX_PANELS as i64 || up > MAX_PANELS as i64 {
            return Err("Sign panels join into at most 10 by 10");
        }
        if found.len() as i64 != across * up {
            return Err("Sign panels join only into a full rectangle");
        }
        found.sort_by_key(|f| (f.2, f.1));
        let anchor = found[0].0;
        let members = found.into_iter().map(|f| f.0).collect();
        Ok((anchor, members, across as u8, up as u8))
    }

    /// Unwrites the written sign covering `p`, if there is one.
    pub(super) fn unwrite_sign_at(&mut self, pi: usize, p: BlockPos) {
        if let Some(a) = self.planets[pi].signs.anchor_of(p) {
            self.unwrite_sign(pi, a);
        }
    }

    fn unwrite_sign(&mut self, pi: usize, anchor: BlockPos) {
        if self.planets[pi].signs.remove(anchor).is_some() {
            self.tell_sign(pi, anchor, None);
        }
    }

    /// Everyone holding the anchor's chunk hears of the sign.
    fn tell_sign(&self, pi: usize, anchor: BlockPos, sign: Option<SignView>) {
        let chunk = anchor.chunk();
        let msg = ServerMsg::Sign { anchor, sign };
        for o in self
            .players
            .values()
            .filter(|o| o.planet == pi && o.sent.contains(&chunk))
        {
            o.out.send(&msg);
        }
    }
}
