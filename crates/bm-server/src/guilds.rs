//! Every guild in the universe (milestone 25 part 2): the guilds by id,
//! each account's primary guild, which claims are aligned to or controlled
//! by a guild, and what changed since the last save. The rules of one guild
//! are `bm_core::guilds`.

use bm_core::guilds::{Guild, MAX_EACH};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};

/// A claim given to a guild: aligned (still run by its owner, open to the
/// guild's members by their rights, its prestige counted to the guild) or
/// controlled (run by the guild's leaders too).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Alignment {
    pub guild: u32,
    pub controlled: bool,
}

#[derive(Default)]
pub struct Guilds {
    by_id: HashMap<u32, Guild>,
    /// Lowercased names in use, for uniqueness.
    names: HashMap<String, u32>,
    next_id: u32,
    primary: HashMap<String, u32>,
    claims: HashMap<u32, Alignment>,
    dirty: BTreeSet<u32>,
    primary_dirty: BTreeSet<String>,
    claims_dirty: BTreeSet<u32>,
}

/// What changed since the last save (None deletes).
pub struct GuildsDirty {
    pub guilds: Vec<(u32, Option<Guild>)>,
    pub primary: Vec<(String, Option<u32>)>,
    pub claims: Vec<(u32, Option<Alignment>)>,
    pub next_id: u32,
}

impl Guilds {
    pub fn load(guilds: Vec<Guild>, primary: Vec<(String, u32)>, claims: Vec<(u32, Alignment)>, next_id: u32) -> Self {
        let mut g = Guilds {
            next_id: next_id.max(1),
            ..Default::default()
        };
        for guild in guilds {
            g.next_id = g.next_id.max(guild.id + 1);
            g.names.insert(guild.name.to_lowercase(), guild.id);
            g.by_id.insert(guild.id, guild);
        }
        g.primary = primary.into_iter().filter(|(_, id)| g.by_id.contains_key(id)).collect();
        g.claims = claims
            .into_iter()
            .filter(|(_, a)| g.by_id.contains_key(&a.guild))
            .collect();
        g
    }

    pub fn get(&self, id: u32) -> Option<&Guild> {
        self.by_id.get(&id)
    }

    /// Changes a guild (marked to be saved).
    pub fn change<T>(&mut self, id: u32, f: impl FnOnce(&mut Guild) -> T) -> Option<T> {
        let g = self.by_id.get_mut(&id)?;
        self.dirty.insert(id);
        Some(f(g))
    }

    pub fn name_taken(&self, name: &str) -> bool {
        self.names.contains_key(&name.to_lowercase())
    }

    /// Founds a guild; the caller has checked the name and the founder's
    /// count.
    pub fn found(&mut self, name: &str, tag: &str, founder: &str, founder_name: &str, now: u64) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        self.names.insert(name.to_lowercase(), id);
        self.by_id
            .insert(id, Guild::found(id, name, tag, founder, founder_name, now));
        self.dirty.insert(id);
        if !self.primary.contains_key(founder) {
            self.set_primary(founder, Some(id));
        }
        id
    }

    /// The guilds `account` is in, lowest id first.
    pub fn of(&self, account: &str) -> Vec<u32> {
        let mut ids: Vec<u32> = self
            .by_id
            .values()
            .filter(|g| g.members.contains_key(account))
            .map(|g| g.id)
            .collect();
        ids.sort_unstable();
        ids
    }

    pub fn count_of(&self, account: &str) -> usize {
        self.by_id.values().filter(|g| g.members.contains_key(account)).count()
    }

    pub fn can_join_more(&self, account: &str) -> bool {
        self.count_of(account) < MAX_EACH
    }

    pub fn primary(&self, account: &str) -> Option<u32> {
        self.primary.get(account).copied()
    }

    pub fn set_primary(&mut self, account: &str, id: Option<u32>) {
        match id {
            Some(id) => self.primary.insert(account.to_string(), id),
            None => self.primary.remove(account),
        };
        self.primary_dirty.insert(account.to_string());
    }

    /// After `account` left or was removed from `id`: their primary moves
    /// to another of their guilds (the lowest), as in Boundless.
    pub fn after_leaving(&mut self, account: &str, id: u32) {
        if self.primary(account) == Some(id) {
            let next = self.of(account).first().copied();
            self.set_primary(account, next);
        }
    }

    /// A guild with no members left is gone, and its claims with it.
    pub fn end(&mut self, id: u32) -> Vec<u32> {
        if let Some(g) = self.by_id.remove(&id) {
            self.names.remove(&g.name.to_lowercase());
        }
        self.dirty.insert(id);
        let claims: Vec<u32> = self
            .claims
            .iter()
            .filter(|(_, a)| a.guild == id)
            .map(|(c, _)| *c)
            .collect();
        for c in &claims {
            self.align(*c, None);
        }
        claims
    }

    /// When `account` joins `id`: their first guild becomes their primary.
    pub fn joined(&mut self, account: &str, id: u32) {
        if self.primary(account).is_none() {
            self.set_primary(account, Some(id));
        }
    }

    pub fn alignment(&self, claim: u32) -> Option<Alignment> {
        self.claims.get(&claim).copied()
    }

    pub fn align(&mut self, claim: u32, a: Option<Alignment>) {
        match a {
            Some(a) => self.claims.insert(claim, a),
            None => self.claims.remove(&claim),
        };
        self.claims_dirty.insert(claim);
    }

    /// The claims given to a guild.
    pub fn claims_of(&self, id: u32) -> impl Iterator<Item = u32> + '_ {
        self.claims.iter().filter(move |(_, a)| a.guild == id).map(|(c, _)| *c)
    }

    pub fn take_dirty(&mut self) -> GuildsDirty {
        GuildsDirty {
            guilds: std::mem::take(&mut self.dirty)
                .into_iter()
                .map(|id| (id, self.by_id.get(&id).cloned()))
                .collect(),
            primary: std::mem::take(&mut self.primary_dirty)
                .into_iter()
                .map(|a| {
                    let p = self.primary.get(&a).copied();
                    (a, p)
                })
                .collect(),
            claims: std::mem::take(&mut self.claims_dirty)
                .into_iter()
                .map(|c| (c, self.claims.get(&c).copied()))
                .collect(),
            next_id: self.next_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn founding_joining_and_ending_keep_primaries_and_claims_right() {
        let mut gs = Guilds::load(Vec::new(), Vec::new(), Vec::new(), 1);
        let a = gs.found("Lanternwrights", "LANT", "ada", "Ada", 0);
        let b = gs.found("Tidecallers", "TIDE", "ada", "Ada", 0);
        assert!(gs.name_taken("lanternwrights"));
        assert_eq!(gs.primary("ada"), Some(a), "the first guild is primary");
        assert_eq!(gs.of("ada"), vec![a, b]);
        gs.align(
            7,
            Some(Alignment {
                guild: b,
                controlled: false,
            }),
        );
        // Leaving the primary moves it to another.
        gs.change(a, |g| g.members.remove("ada"));
        gs.after_leaving("ada", a);
        assert_eq!(gs.primary("ada"), Some(b));
        assert_eq!(gs.end(b), vec![7]);
        assert_eq!(gs.alignment(7), None);
        assert!(!gs.name_taken("Tidecallers"));
        let d = gs.take_dirty();
        assert_eq!(d.next_id, 3);
        assert!(d.guilds.iter().any(|(id, g)| *id == b && g.is_none()));
    }
}
