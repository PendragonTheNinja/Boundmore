//! Guilds (milestone 25 part 2, `docs/milestones/25-guilds.md`, research
//! `docs/research/guilds-and-permissions.md`): Boundless's ranks and rights
//! (Nathan, 2026-10-28: "as Boundless", without factions). A guild is
//! pure data here; the server keeps them, saves them and tells members.
//!
//! Ranks, lowest first: Operative, Controller, Executive, Director. Every
//! member holds claim rights for the guild's claims (`crate::rights`);
//! Controllers and up may also hold powers over the guild's coin, its
//! membership and its buffs. Executives set the ranks, rights and powers of
//! Operatives and Controllers; the Director sets everyone's and may hand
//! the guild on.

use crate::rights;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};

/// Longest guild name and tag (Boundless's).
pub const NAME_MAX: usize = 20;
pub const TAG_MAX: usize = 5;
/// Guilds one character may be in (Boundless's).
pub const MAX_EACH: usize = 5;
/// Most members a guild may have (**guess**; Boundless's limit is not
/// recorded).
pub const MAX_MEMBERS: usize = 500;
/// Requests waiting at one guild.
pub const MAX_ASKED: usize = 100;
/// Log entries kept.
pub const LOG_KEEP: usize = 200;

/// Powers over the guild itself.
pub const COIN: u8 = 1;
pub const MEMBERSHIP: u8 = 2;
pub const BUFFS: u8 = 4;
pub const ALL_POWERS: u8 = COIN | MEMBERSHIP | BUFFS;
pub const POWERS: [(u8, &str); 3] = [(COIN, "Coin"), (MEMBERSHIP, "Membership"), (BUFFS, "Buffs")];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Rank {
    Operative,
    Controller,
    Executive,
    Director,
}

impl Rank {
    pub const ALL: [Rank; 4] = [Rank::Operative, Rank::Controller, Rank::Executive, Rank::Director];

    pub fn name(self) -> &'static str {
        match self {
            Rank::Operative => "Operative",
            Rank::Controller => "Controller",
            Rank::Executive => "Executive",
            Rank::Director => "Director",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    /// As shown.
    pub name: String,
    pub rank: Rank,
    /// Claim rights on the guild's claims.
    pub rights: u8,
    /// Powers over the guild (Controllers and up only).
    pub powers: u8,
}

impl Member {
    /// Claim rights in effect: the Director and Executives hold them all.
    pub fn claim_rights(&self) -> u8 {
        if self.rank >= Rank::Executive {
            rights::ALL
        } else {
            self.rights
        }
    }

    /// Powers in effect: Executives and the Director hold them all, an
    /// Operative none.
    pub fn powers(&self) -> u8 {
        match self.rank {
            Rank::Operative => 0,
            Rank::Controller => self.powers,
            _ => ALL_POWERS,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// Unix seconds.
    pub time: u64,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Guild {
    pub id: u32,
    pub name: String,
    pub tag: String,
    /// Members by account.
    pub members: BTreeMap<String, Member>,
    /// Requests to join, by account, with each as shown.
    pub asked: BTreeMap<String, String>,
    pub coin: u64,
    pub log: VecDeque<Entry>,
}

/// Why a guild action was refused, worded for the player.
pub type Refused = &'static str;

/// A guild name or tag as typed, trimmed; None if not allowed.
pub fn clean(text: &str, max: usize) -> Option<String> {
    let t = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let ok = !t.is_empty() && t.chars().count() <= max && t.chars().all(|c| c.is_alphanumeric() || " '-".contains(c));
    ok.then_some(t)
}

impl Guild {
    /// A new guild with its founder as Director.
    pub fn found(id: u32, name: &str, tag: &str, founder: &str, founder_name: &str, now: u64) -> Guild {
        let mut g = Guild {
            id,
            name: name.to_string(),
            tag: tag.to_string(),
            members: BTreeMap::new(),
            asked: BTreeMap::new(),
            coin: 0,
            log: VecDeque::new(),
        };
        g.members.insert(
            founder.to_string(),
            Member {
                name: founder_name.to_string(),
                rank: Rank::Director,
                rights: rights::ALL,
                powers: ALL_POWERS,
            },
        );
        g.note(now, format!("{founder_name} founded {name}"));
        g
    }

    pub fn note(&mut self, now: u64, text: String) {
        self.log.push_back(Entry { time: now, text });
        while self.log.len() > LOG_KEEP {
            self.log.pop_front();
        }
    }

    pub fn member(&self, account: &str) -> Option<&Member> {
        self.members.get(account)
    }

    fn name_of(&self, account: &str) -> String {
        self.members
            .get(account)
            .map(|m| m.name.clone())
            .unwrap_or_else(|| account.to_string())
    }

    fn power(&self, account: &str, power: u8) -> Result<&Member, Refused> {
        let m = self.members.get(account).ok_or("You are not in this guild")?;
        if m.powers() & power == 0 {
            return Err("You do not have that power in this guild");
        }
        Ok(m)
    }

    /// `account` asks to join.
    pub fn ask(&mut self, account: &str, name: &str) -> Result<(), Refused> {
        if self.members.contains_key(account) {
            return Err("You are already in this guild");
        }
        if self.asked.contains_key(account) {
            return Err("You have already asked to join");
        }
        if self.asked.len() >= MAX_ASKED {
            return Err("This guild has too many requests waiting");
        }
        self.asked.insert(account.to_string(), name.to_string());
        Ok(())
    }

    /// `by` accepts `who`'s request (`who_guilds`: how many guilds they are
    /// in now).
    pub fn accept(&mut self, by: &str, who: &str, who_guilds: usize, now: u64) -> Result<(), Refused> {
        self.power(by, MEMBERSHIP)?;
        let name = self.asked.get(who).cloned().ok_or("They have not asked to join")?;
        if who_guilds >= MAX_EACH {
            self.asked.remove(who);
            return Err("They are already in 5 guilds");
        }
        if self.members.len() >= MAX_MEMBERS {
            return Err("This guild is full");
        }
        self.asked.remove(who);
        self.members.insert(
            who.to_string(),
            Member {
                name: name.clone(),
                rank: Rank::Operative,
                rights: 0,
                powers: 0,
            },
        );
        let by_name = self.name_of(by);
        self.note(now, format!("{by_name} let {name} join"));
        Ok(())
    }

    pub fn decline(&mut self, by: &str, who: &str, now: u64) -> Result<(), Refused> {
        self.power(by, MEMBERSHIP)?;
        let name = self.asked.remove(who).ok_or("They have not asked to join")?;
        let by_name = self.name_of(by);
        self.note(now, format!("{by_name} turned down {name}"));
        Ok(())
    }

    /// `account` leaves. The Director must hand the guild on first, unless
    /// they are its last member (then the guild ends: true).
    pub fn leave(&mut self, account: &str, now: u64) -> Result<bool, Refused> {
        let m = self.members.get(account).ok_or("You are not in this guild")?;
        if m.rank == Rank::Director && self.members.len() > 1 {
            return Err("Hand the guild to another member before you leave");
        }
        let name = m.name.clone();
        self.members.remove(account);
        self.note(now, format!("{name} left"));
        Ok(self.members.is_empty())
    }

    /// Whether `by` may change `who`: only those below them, and an
    /// Executive only Operatives and Controllers.
    fn over(&self, by: &str, who: &str) -> Result<(), Refused> {
        let a = self.members.get(by).ok_or("You are not in this guild")?;
        let b = self.members.get(who).ok_or("They are not in this guild")?;
        if a.rank < Rank::Executive || b.rank >= a.rank {
            return Err("Only a higher rank can change that");
        }
        Ok(())
    }

    /// `by` removes `who`: an Executive or the Director, over those below.
    pub fn remove(&mut self, by: &str, who: &str, now: u64) -> Result<(), Refused> {
        self.over(by, who)?;
        let name = self.name_of(who);
        let by_name = self.name_of(by);
        self.members.remove(who);
        self.note(now, format!("{by_name} removed {name}"));
        Ok(())
    }

    /// `by` sets `who`'s rank. Making someone Director hands the guild on:
    /// the old Director becomes an Executive.
    pub fn set_rank(&mut self, by: &str, who: &str, rank: Rank, now: u64) -> Result<(), Refused> {
        self.over(by, who)?;
        let by_rank = self.members[by].rank;
        if rank >= by_rank && by_rank != Rank::Director {
            return Err("Only a higher rank can change that");
        }
        if rank == Rank::Director {
            self.members.get_mut(by).expect("checked").rank = Rank::Executive;
        }
        let m = self.members.get_mut(who).expect("checked");
        m.rank = rank;
        let name = m.name.clone();
        let by_name = self.name_of(by);
        self.note(now, format!("{by_name} made {name} {}", rank.name()));
        Ok(())
    }

    /// `by` sets `who`'s claim rights and powers.
    pub fn set_rights(&mut self, by: &str, who: &str, claim_rights: u8, powers: u8, now: u64) -> Result<(), Refused> {
        if by != who {
            self.over(by, who)?;
        } else if self.members.get(by).is_none_or(|m| m.rank != Rank::Director) {
            return Err("Only a higher rank can change that");
        }
        let m = self.members.get_mut(who).expect("checked");
        m.rights = claim_rights & rights::ALL;
        m.powers = powers & ALL_POWERS;
        let (name, r, p) = (m.name.clone(), m.rights, m.powers);
        let by_name = self.name_of(by);
        let powers: Vec<&str> = POWERS.iter().filter(|(b, _)| p & b != 0).map(|(_, n)| *n).collect();
        let powers = if powers.is_empty() {
            String::new()
        } else {
            format!("; powers: {}", powers.join(", "))
        };
        self.note(now, format!("{by_name} gave {name} {}{powers}", rights::describe(r)));
        Ok(())
    }

    /// A member gives coin (already taken from them).
    pub fn donated(&mut self, account: &str, coin: u64, now: u64) {
        self.coin = self.coin.saturating_add(coin);
        let name = self.name_of(account);
        self.note(now, format!("{name} gave {coin} coin"));
    }

    /// `by` pays `coin` to member `to` (the caller credits them).
    pub fn pay(&mut self, by: &str, to: &str, coin: u64, now: u64) -> Result<(), Refused> {
        self.power(by, COIN)?;
        if !self.members.contains_key(to) {
            return Err("They are not in this guild");
        }
        if coin == 0 || coin > self.coin {
            return Err("The guild does not have that much coin");
        }
        self.coin -= coin;
        let (by_name, to_name) = (self.name_of(by), self.name_of(to));
        self.note(now, format!("{by_name} paid {to_name} {coin} coin"));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guild() -> Guild {
        let mut g = Guild::found(1, "Lanternwrights", "LANT", "ada", "Ada", 0);
        for (a, n) in [("bo", "Bo"), ("cy", "Cy"), ("di", "Di")] {
            g.ask(a, n).unwrap();
            g.accept("ada", a, 0, 1).unwrap();
        }
        g
    }

    #[test]
    fn names_are_checked() {
        assert_eq!(
            clean("  The   Lantern Guild ", NAME_MAX),
            Some("The Lantern Guild".into())
        );
        assert_eq!(clean("A name far too long for any guild", NAME_MAX), None);
        assert_eq!(clean("<b>", TAG_MAX), None);
        assert_eq!(clean("", TAG_MAX), None);
    }

    #[test]
    fn joining_needs_the_membership_power_and_room() {
        let mut g = guild();
        assert_eq!(g.members.len(), 4);
        assert_eq!(g.members["bo"].rank, Rank::Operative);
        g.ask("ed", "Ed").unwrap();
        assert_eq!(g.ask("ed", "Ed"), Err("You have already asked to join"));
        assert_eq!(
            g.accept("bo", "ed", 0, 2),
            Err("You do not have that power in this guild")
        );
        // A Controller given Membership may accept.
        g.set_rank("ada", "bo", Rank::Controller, 2).unwrap();
        g.set_rights("ada", "bo", 0, MEMBERSHIP, 2).unwrap();
        assert_eq!(g.accept("bo", "ed", 5, 3), Err("They are already in 5 guilds"));
        assert!(!g.asked.contains_key("ed"));
        g.ask("ed", "Ed").unwrap();
        g.accept("bo", "ed", 1, 3).unwrap();
        assert!(g.member("ed").is_some());
        assert!(g.log.back().unwrap().text.contains("Bo let Ed join"));
    }

    #[test]
    fn ranks_change_only_from_above() {
        let mut g = guild();
        g.set_rank("ada", "bo", Rank::Executive, 1).unwrap();
        // An Executive sets Operatives and Controllers, not peers or up.
        g.set_rank("bo", "cy", Rank::Controller, 1).unwrap();
        assert!(g.set_rank("bo", "cy", Rank::Executive, 1).is_err());
        assert!(
            g.set_rank("cy", "di", Rank::Controller, 1).is_err(),
            "a Controller sets nobody"
        );
        assert!(g.remove("bo", "ada", 1).is_err());
        g.remove("bo", "di", 1).unwrap();
        assert!(g.member("di").is_none());
        // Executives hold every right and power.
        assert_eq!(g.members["bo"].claim_rights(), rights::ALL);
        assert_eq!(g.members["cy"].claim_rights(), 0);
        // Handing the guild on.
        g.set_rank("ada", "bo", Rank::Director, 2).unwrap();
        assert_eq!(
            (g.members["ada"].rank, g.members["bo"].rank),
            (Rank::Executive, Rank::Director)
        );
        assert_eq!(
            g.leave("bo", 3),
            Err("Hand the guild to another member before you leave")
        );
        assert_eq!(g.leave("ada", 3), Ok(false));
    }

    #[test]
    fn coin_comes_in_and_goes_out_by_power() {
        let mut g = guild();
        g.donated("cy", 500, 1);
        assert_eq!(g.coin, 500);
        assert_eq!(
            g.pay("cy", "cy", 100, 1),
            Err("You do not have that power in this guild")
        );
        g.pay("ada", "cy", 100, 1).unwrap();
        assert_eq!(
            g.pay("ada", "cy", 1000, 1),
            Err("The guild does not have that much coin")
        );
        assert_eq!(g.coin, 400);
        // The last member leaving ends it.
        let mut solo = Guild::found(2, "Solo", "S", "z", "Z", 0);
        assert_eq!(solo.leave("z", 1), Ok(true));
        for i in 0..300 {
            g.note(i, "x".into());
        }
        assert_eq!(g.log.len(), LOG_KEEP);
    }
}
