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
use std::collections::{BTreeMap, BTreeSet, VecDeque};

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
    /// Endeavour to spend on buffs (part 3).
    pub endeavour: u64,
    /// The week (`week_of`) `xp_week` counts.
    pub week: u64,
    /// XP each member earned this week with this guild their primary.
    pub xp_week: BTreeMap<String, u64>,
    /// Buffs bought and not yet over.
    pub buffs: Vec<ActiveBuff>,
}

// ------------------------------------------------------------- buffs

/// XP a member earns for one endeavour, and the most endeavour one member
/// gives in a week (Boundless's).
pub const XP_PER_ENDEAVOUR: u64 = 100;
pub const ENDEAVOUR_WEEK_CAP: u64 = 500;
/// A buff lasts a week (Boundless's), in seconds.
pub const BUFF_SECONDS: u64 = 7 * 86_400;
/// Buffs a guild may have at once (Boundless's).
pub const MAX_BUFFS: usize = 3;
/// Coin and endeavour to start a buff, by tier 1 (Minor) to 5 (Mega)
/// (Boundless's GuildBuff table).
pub const TIER_COIN: [u64; 5] = [5_500, 15_000, 40_000, 80_000, 160_000];
pub const TIER_ENDEAVOUR: [u64; 5] = [400, 700, 1_400, 2_300, 3_600];
pub const TIER_NAMES: [&str; 5] = ["Minor", "Improved", "Greater", "Super", "Mega"];

/// The week a unix time falls in.
pub fn week_of(now: u64) -> u64 {
    now / (7 * 86_400)
}

/// What a helix machine gives (the first four of Boundless's Release 211).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum BuffKind {
    /// Chisel durability and block placing speed.
    Builders,
    /// Run speed and grapple length.
    GrappleRun,
    /// Less damage from lava.
    LavaProtection,
    /// A smaller defeat penalty.
    DefeatInsurance,
}

impl BuffKind {
    pub const ALL: [BuffKind; 4] = [
        BuffKind::Builders,
        BuffKind::GrappleRun,
        BuffKind::LavaProtection,
        BuffKind::DefeatInsurance,
    ];

    pub fn name(self) -> &'static str {
        match self {
            BuffKind::Builders => "Builder's Buff",
            BuffKind::GrappleRun => "Grapple and Run",
            BuffKind::LavaProtection => "Lava Protection",
            BuffKind::DefeatInsurance => "Defeat Insurance",
        }
    }

    /// Its helix machine's block name.
    pub fn helix(self) -> &'static str {
        match self {
            BuffKind::Builders => "builders_helix",
            BuffKind::GrappleRun => "grapple_run_helix",
            BuffKind::LavaProtection => "lava_protection_helix",
            BuffKind::DefeatInsurance => "defeat_insurance_helix",
        }
    }

    pub fn of_helix(block: &str) -> Option<BuffKind> {
        BuffKind::ALL.into_iter().find(|k| k.helix() == block)
    }

    /// The three tiers it comes at (Boundless's).
    pub fn tiers(self) -> [u8; 3] {
        match self {
            BuffKind::Builders | BuffKind::LavaProtection => [1, 2, 4],
            BuffKind::GrappleRun => [1, 3, 5],
            BuffKind::DefeatInsurance => [2, 4, 5],
        }
    }

    /// Its strength at `tier`: a percent (Builder's: chisel durability and
    /// placing speed; Lava: less damage; Defeat: less penalty), or for
    /// Grapple and Run tenths of a block a second of run speed (and
    /// [`grapple_bonus`] for the grapple). Builder's and Grapple and Run are
    /// Boundless's; Lava and Defeat are **guesses** (Measure M1).
    pub fn strength(self, tier: u8) -> u16 {
        let i = self.tiers().iter().position(|&t| t == tier).unwrap_or(0);
        match self {
            BuffKind::Builders => [15, 30, 50][i],
            BuffKind::GrappleRun => [2, 4, 6][i],
            BuffKind::LavaProtection => [25, 50, 75][i],
            BuffKind::DefeatInsurance => [25, 50, 75][i],
        }
    }

    /// What it does at `tier`, for its window.
    pub fn describe(self, tier: u8) -> String {
        let s = self.strength(tier);
        match self {
            BuffKind::Builders => format!("{s}% more chisel durability and {s}% faster block placing"),
            BuffKind::GrappleRun => format!(
                "{:.1} blocks a second faster running and {} blocks more grapple length",
                s as f64 / 10.0,
                grapple_bonus(tier)
            ),
            BuffKind::LavaProtection => format!("{s}% less damage from lava"),
            BuffKind::DefeatInsurance => format!("{s}% less XP lost when defeated"),
        }
    }
}

/// Grapple and Run's extra grapple length at `tier`, blocks (Boundless's).
pub fn grapple_bonus(tier: u8) -> u16 {
    match tier {
        1 => 5,
        3 => 15,
        5 => 25,
        _ => 0,
    }
}

/// A buff a guild bought: for the members whose primary guild it was then.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveBuff {
    pub kind: BuffKind,
    pub tier: u8,
    /// Unix seconds it ends.
    pub until: u64,
    pub members: BTreeSet<String>,
}

/// What a member's guild buffs add up to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Boost {
    /// Run speed, tenths of a block a second, and grapple length, blocks.
    pub run_tenths: u16,
    pub grapple: u16,
    pub chisel_percent: u16,
    pub place_percent: u16,
    pub lava_percent: u16,
    pub defeat_percent: u16,
}

/// The boost `account` has from these guilds' buffs at `now`.
pub fn boost<'a>(guilds: impl IntoIterator<Item = &'a Guild>, account: &str, now: u64) -> Boost {
    let mut b = Boost::default();
    for g in guilds {
        for buff in g.buffs.iter().filter(|x| x.until > now && x.members.contains(account)) {
            let s = buff.kind.strength(buff.tier);
            match buff.kind {
                BuffKind::Builders => {
                    b.chisel_percent = b.chisel_percent.max(s);
                    b.place_percent = b.place_percent.max(s);
                }
                BuffKind::GrappleRun => {
                    b.run_tenths = b.run_tenths.max(s);
                    b.grapple = b.grapple.max(grapple_bonus(buff.tier));
                }
                BuffKind::LavaProtection => b.lava_percent = b.lava_percent.max(s),
                BuffKind::DefeatInsurance => b.defeat_percent = b.defeat_percent.max(s),
            }
        }
    }
    b
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
            endeavour: 0,
            week: 0,
            xp_week: BTreeMap::new(),
            buffs: Vec::new(),
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

    /// `account`, whose primary guild this is, earned `xp`: endeavour for
    /// every 100, up to 500 a week from each member. Returns what it gave.
    pub fn earn(&mut self, account: &str, xp: u64, now: u64) -> u64 {
        if !self.members.contains_key(account) || xp == 0 {
            return 0;
        }
        let week = week_of(now);
        if week != self.week {
            self.week = week;
            self.xp_week.clear();
        }
        let had = self.xp_week.entry(account.to_string()).or_insert(0);
        let given = |x: u64| (x / XP_PER_ENDEAVOUR).min(ENDEAVOUR_WEEK_CAP);
        let before = given(*had);
        *had = had.saturating_add(xp);
        let gained = given(*had) - before;
        self.endeavour = self.endeavour.saturating_add(gained);
        gained
    }

    /// Buffs not yet over at `now`.
    pub fn active(&self, now: u64) -> impl Iterator<Item = &ActiveBuff> {
        self.buffs.iter().filter(move |b| b.until > now)
    }

    /// `by` starts `kind` at `tier` for `members` (those whose primary
    /// guild this is now), paying its coin and endeavour.
    pub fn start_buff(
        &mut self,
        by: &str,
        kind: BuffKind,
        tier: u8,
        members: BTreeSet<String>,
        now: u64,
    ) -> Result<(), Refused> {
        self.power(by, BUFFS)?;
        if !kind.tiers().contains(&tier) {
            return Err("That buff does not come at that strength");
        }
        self.buffs.retain(|b| b.until > now);
        if self.buffs.iter().any(|b| b.kind == kind) {
            return Err("That buff is already running");
        }
        if self.buffs.len() >= MAX_BUFFS {
            return Err("A guild can run 3 buffs at once");
        }
        let (coin, endeavour) = (TIER_COIN[tier as usize - 1], TIER_ENDEAVOUR[tier as usize - 1]);
        if self.coin < coin {
            return Err("The guild does not have the coin for that");
        }
        if self.endeavour < endeavour {
            return Err("The guild does not have the endeavour for that");
        }
        self.coin -= coin;
        self.endeavour -= endeavour;
        self.buffs.push(ActiveBuff {
            kind,
            tier,
            until: now + BUFF_SECONDS,
            members,
        });
        let by_name = self.name_of(by);
        self.note(
            now,
            format!("{by_name} started {} {}", TIER_NAMES[tier as usize - 1], kind.name()),
        );
        Ok(())
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

    #[test]
    fn endeavour_comes_from_xp_up_to_a_weekly_cap() {
        let mut g = guild();
        let now = 10 * 7 * 86_400;
        assert_eq!(g.earn("bo", 250, now), 2);
        assert_eq!(g.earn("bo", 50, now), 1, "the remainder carries");
        assert_eq!(g.earn("bo", 1_000_000, now), 497, "500 a week at most");
        assert_eq!(g.earn("bo", 1_000, now), 0);
        assert_eq!(g.earn("stranger", 1_000, now), 0);
        assert_eq!(g.earn("bo", 1_000, now + 7 * 86_400), 10, "a new week");
        assert_eq!(g.endeavour, 510);
    }

    #[test]
    fn buffs_cost_coin_and_endeavour_and_reach_their_members() {
        let mut g = guild();
        g.coin = 100_000;
        g.endeavour = 2_000;
        let members: BTreeSet<String> = ["ada".to_string(), "bo".to_string()].into();
        assert_eq!(
            g.start_buff("bo", BuffKind::GrappleRun, 1, members.clone(), 0),
            Err("You do not have that power in this guild")
        );
        assert!(
            g.start_buff("ada", BuffKind::GrappleRun, 2, members.clone(), 0)
                .is_err(),
            "not a tier it has"
        );
        g.start_buff("ada", BuffKind::GrappleRun, 3, members.clone(), 0)
            .unwrap();
        assert_eq!((g.coin, g.endeavour), (60_000, 600));
        assert_eq!(
            g.start_buff("ada", BuffKind::GrappleRun, 1, members.clone(), 1),
            Err("That buff is already running")
        );
        assert_eq!(
            g.start_buff("ada", BuffKind::Builders, 2, members.clone(), 1),
            Err("The guild does not have the endeavour for that")
        );
        g.start_buff("ada", BuffKind::Builders, 1, members.clone(), 1).unwrap();
        let b = boost([&g], "bo", 100);
        assert_eq!((b.run_tenths, b.grapple, b.place_percent), (4, 15, 15));
        assert_eq!(boost([&g], "cy", 100), Boost::default(), "not primary when bought");
        assert_eq!(boost([&g], "bo", BUFF_SECONDS + 1), Boost::default(), "a week later");
        assert_eq!(BuffKind::of_helix("grapple_run_helix"), Some(BuffKind::GrappleRun));
    }
}
