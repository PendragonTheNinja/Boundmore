//! Coin and feats (milestone 24, `docs/milestones/24-coin-and-trade.md`,
//! research `docs/research/coin-and-trade.md`).
//!
//! A feat is one counter and five thresholds, as Boundless's: reaching a
//! tier pays that tier's coin and XP once. Counters are named by keys the
//! server counts ("placed", "placed:rock", "kill:grazer", "craft:mixer",
//! "sprinted" in blocks, ...); feats name the key they watch, so a new feat
//! is data (`assets/feats.toml`). The ledger (coin, counters, tiers paid) is
//! the server's, saved per account.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Coin each feat tier pays (Boundless's, wiki Coin page).
pub const TIER_COIN: [u64; 5] = [100, 280, 860, 2_560, 7_700];
/// XP each tier pays: the tier's coin again (**guess**: Boundless's coffer
/// XP is not on the wiki).
pub const TIER_XP: [u64; 5] = TIER_COIN;
/// Coin a new character starts with (Nathan, 2026-10-27).
pub const STARTING_COIN: u64 = 500;

/// One feat.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatDef {
    pub id: String,
    /// What players see.
    pub name: String,
    /// What it asks, in words with `{n}` for the next threshold
    /// ("Place {n} blocks").
    pub text: String,
    /// "builder", "combat", "crafter", "explorer", "gatherer", "miner",
    /// "core", "trader".
    pub group: String,
    /// The counter it watches.
    pub counts: String,
    /// Tiers 1 to 5.
    pub tiers: [u64; 5],
}

impl FeatDef {
    /// Tiers reached by `count` (0 to 5).
    pub fn reached(&self, count: u64) -> u8 {
        self.tiers.iter().take_while(|&&t| count >= t).count() as u8
    }

    /// The text for the next tier ("Place 2500 blocks"), or the last one's.
    pub fn task(&self, done: u8) -> String {
        let n = self.tiers[(done as usize).min(4)];
        self.text.replace("{n}", &n.to_string())
    }
}

#[derive(Debug, Deserialize)]
struct FeatFile {
    feat: Vec<FeatDef>,
}

/// Every feat, in the file's order.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FeatBook {
    pub feats: Vec<FeatDef>,
}

impl FeatBook {
    /// Reads `assets/feats.toml`: ids unique, thresholds rising.
    pub fn parse(text: &str) -> Result<FeatBook, String> {
        let f: FeatFile = toml::from_str(text).map_err(|e| format!("feats: {e}"))?;
        let mut seen = std::collections::HashSet::new();
        for d in &f.feat {
            if !seen.insert(d.id.clone()) {
                return Err(format!("feats: {} twice", d.id));
            }
            if d.tiers.windows(2).any(|w| w[1] <= w[0]) || d.tiers[0] == 0 {
                return Err(format!("feats: {}'s tiers must rise from above 0", d.id));
            }
            if !d.text.contains("{n}") {
                return Err(format!("feats: {}'s text has no {{n}}", d.id));
            }
        }
        Ok(FeatBook { feats: f.feat })
    }

    pub fn get(&self, id: &str) -> Option<&FeatDef> {
        self.feats.iter().find(|f| f.id == id)
    }
}

/// A tier just reached: what it pays.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reached {
    pub feat: String,
    /// 1 to 5.
    pub tier: u8,
    pub coin: u64,
    pub xp: u64,
}

/// A character's coin and feats, saved per account.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ledger {
    pub coin: u64,
    /// Counter key -> count.
    pub counts: BTreeMap<String, u64>,
    /// Feat id -> tiers paid (1 to 5).
    pub paid: BTreeMap<String, u8>,
}

impl Default for Ledger {
    /// A new character's: the starting coin.
    fn default() -> Self {
        Ledger {
            coin: STARTING_COIN,
            counts: BTreeMap::new(),
            paid: BTreeMap::new(),
        }
    }
}

impl Ledger {
    pub fn count(&self, key: &str) -> u64 {
        self.counts.get(key).copied().unwrap_or(0)
    }

    /// Adds `n` to counter `key` and pays every feat tier it reaches (coin
    /// here; the caller adds the XP). Feats counting feats done are checked
    /// again until nothing more is reached.
    pub fn add(&mut self, book: &FeatBook, key: &str, n: u64) -> Vec<Reached> {
        if n == 0 {
            return Vec::new();
        }
        let c = self.counts.entry(key.to_string()).or_insert(0);
        *c = c.saturating_add(n);
        self.settle(book)
    }

    /// Sets counter `key` to at least `value` (a level reached, worlds
    /// known) and pays what that, or anything counted unpaid, reaches.
    pub fn raise(&mut self, book: &FeatBook, key: &str, value: u64) -> Vec<Reached> {
        let c = self.counts.entry(key.to_string()).or_insert(0);
        *c = (*c).max(value);
        self.settle(book)
    }

    /// Adds `n` to counter `key` without paying (its owner is away: what it
    /// reaches is paid by the next `raise` or `add`, on their return).
    pub fn note(&mut self, key: &str, n: u64) {
        let c = self.counts.entry(key.to_string()).or_insert(0);
        *c = c.saturating_add(n);
    }

    fn settle(&mut self, book: &FeatBook) -> Vec<Reached> {
        let mut out = Vec::new();
        loop {
            let before = out.len();
            for f in &book.feats {
                let reached = f.reached(self.count(&f.counts));
                let paid = self.paid.get(&f.id).copied().unwrap_or(0);
                for tier in paid + 1..=reached {
                    let i = (tier - 1) as usize;
                    self.coin = self.coin.saturating_add(TIER_COIN[i]);
                    out.push(Reached {
                        feat: f.id.clone(),
                        tier,
                        coin: TIER_COIN[i],
                        xp: TIER_XP[i],
                    });
                }
                if reached > paid {
                    self.paid.insert(f.id.clone(), reached);
                }
            }
            if out.len() == before {
                return out;
            }
            // Feat tiers count toward the feats-done feat.
            let done = (out.len() - before) as u64;
            *self.counts.entry(FEATS_DONE.to_string()).or_insert(0) += done;
        }
    }

    /// Takes `amount` coin if there is that much.
    pub fn spend(&mut self, amount: u64) -> bool {
        if self.coin < amount {
            return false;
        }
        self.coin -= amount;
        true
    }
}

/// The counter a broken block of `def` adds to, if any (miner and gatherer
/// feats): by its health class, material and name.
pub fn mined_key(def: &crate::block::BlockDef) -> Option<&'static str> {
    use crate::block::{Health, Material};
    const GEMS: [&str; 9] = [
        "amethyst", "emerald", "sapphire", "topaz", "diamond", "ruby", "aurorite", "pyrelith", "gloamite",
    ];
    const ROCKS: [&str; 5] = ["stone", "deep_stone", "sandstone", "basalt", "limestone"];
    let n = def.name.as_str();
    Some(match (def.health, def.material) {
        (Health::Seam, _) if n.contains("coal") => "mine:coal",
        (Health::Seam, _) if n.contains("relic") => "mine:relic",
        (Health::Seam, _) if n.contains("fossil") => "mine:fossil",
        (Health::Seam, _) if GEMS.iter().any(|g| n.starts_with(g)) => "mine:gem",
        (Health::Seam, _)
            if ["copper", "iron", "silver", "gold", "titanium"]
                .iter()
                .any(|m| n.starts_with(m)) =>
        {
            "mine:ore"
        }
        (Health::Plant, Material::Rock) => "gather:boulder",
        (Health::Plant, _) if ["_cap", "glowcap", "_bracket", "_coral"].iter().any(|f| n.ends_with(f)) => {
            "gather:fungus"
        }
        (Health::Plant, _) => "gather:plant",
        (Health::Soft, Material::Plant) => "gather:foliage",
        (Health::Soft, Material::Soil) if !n.starts_with("tilled") => "gather:soil",
        (_, Material::Wood) if n.ends_with("trunk") => "gather:trunk",
        (_, Material::Rock) if ROCKS.contains(&n) => "mine:rock",
        _ if n == "glow_crystal" => "mine:crystal",
        _ => return None,
    })
}

/// The material counter a placed block of `def` adds to, beside "placed".
pub fn placed_key(def: &crate::block::BlockDef) -> Option<&'static str> {
    use crate::block::Material;
    match def.material {
        Material::Wood => Some("placed:wood"),
        Material::Rock => Some("placed:rock"),
        Material::Soil => Some("placed:soil"),
        _ => None,
    }
}

/// The counter of feat tiers reached.
pub const FEATS_DONE: &str = "feats_done";

#[cfg(test)]
mod tests {
    use super::*;

    fn book() -> FeatBook {
        FeatBook::parse(
            r#"
[[feat]]
id = "builder"
name = "Builder"
text = "Place {n} blocks"
group = "builder"
counts = "placed"
tiers = [10, 20, 30, 40, 50]

[[feat]]
id = "achiever"
name = "Achiever"
text = "Reach {n} feat tiers"
group = "core"
counts = "feats_done"
tiers = [2, 4, 6, 8, 10]
"#,
        )
        .unwrap()
    }

    #[test]
    fn the_shipped_feats_parse() {
        let b = FeatBook::parse(include_str!("../../../assets/feats.toml")).unwrap();
        assert!(b.feats.len() >= 40, "{}", b.feats.len());
    }

    #[test]
    fn tiers_pay_once_and_feats_done_counts_them() {
        let b = book();
        let mut l = Ledger::default();
        assert_eq!(l.coin, STARTING_COIN);
        assert!(l.add(&b, "placed", 9).is_empty());
        let r = l.add(&b, "placed", 1);
        assert_eq!(r.len(), 1);
        assert_eq!((r[0].feat.as_str(), r[0].tier, r[0].coin), ("builder", 1, 100));
        // A jump past two thresholds pays both, then the feats-done feat:
        // three tiers reach its first, and that tier, a fourth, its second.
        let r = l.add(&b, "placed", 20);
        let got: Vec<(&str, u8)> = r.iter().map(|x| (x.feat.as_str(), x.tier)).collect();
        assert_eq!(
            got,
            vec![("builder", 2), ("builder", 3), ("achiever", 1), ("achiever", 2)]
        );
        assert_eq!(l.count(FEATS_DONE), 5);
        assert_eq!(l.coin, STARTING_COIN + 100 + 280 + 860 + 100 + 280);
        // Counted while away: paid on the next settling.
        l.note("placed", 10);
        assert_eq!(l.paid["builder"], 3, "not yet");
        let r = l.raise(&b, "level", 1);
        assert_eq!((r[0].feat.as_str(), r[0].tier), ("builder", 4), "paid on return");
        // Never twice.
        assert!(l.add(&b, "placed", 0).is_empty());
        assert!(l.raise(&b, "placed", 5).is_empty());
        assert_eq!(l.paid["builder"], 4);
        assert!(l.spend(100));
        assert!(!l.spend(u64::MAX));
    }

    #[test]
    fn bad_books_are_refused() {
        let bad = r#"
[[feat]]
id = "a"
name = "A"
text = "Do {n}"
group = "core"
counts = "x"
tiers = [1, 1, 2, 3, 4]
"#;
        assert!(FeatBook::parse(bad).is_err());
        assert!(FeatBook::parse(&bad.replace("{n}", "it")).is_err());
    }

    #[test]
    fn blocks_count_toward_their_feats() {
        let reg = crate::block::BlockRegistry::builtin();
        let key = |n: &str| mined_key(reg.get(reg.id(n).unwrap()));
        for (n, k) in [
            ("stone", Some("mine:rock")),
            ("basalt", Some("mine:rock")),
            ("coal_seam", Some("mine:coal")),
            ("hard_coal_seam", Some("mine:coal")),
            ("copper_vein", Some("mine:ore")),
            ("titanium_seam", Some("mine:ore")),
            ("relic_seam", Some("mine:relic")),
            ("small_fossil_seam", Some("mine:fossil")),
            ("ruby_seam", Some("mine:gem")),
            ("glow_crystal", Some("mine:crystal")),
            ("trunk", Some("gather:trunk")),
            ("foliage", Some("gather:foliage")),
            ("spikeleaf", Some("gather:plant")),
            ("glowcap", Some("gather:fungus")),
            ("tarspot_cap", Some("gather:fungus")),
            ("boulder", Some("gather:boulder")),
            ("soil", Some("gather:soil")),
            ("refined_stone", None),
            ("timber", None),
            ("workbench", None),
        ] {
            assert_eq!(key(n), k, "{n}");
        }
        assert_eq!(placed_key(reg.get(reg.id("timber").unwrap())), Some("placed:wood"));
    }

    #[test]
    fn task_text_names_the_next_threshold() {
        let b = book();
        let f = b.get("builder").unwrap();
        assert_eq!(f.task(0), "Place 10 blocks");
        assert_eq!(f.task(3), "Place 40 blocks");
        assert_eq!(f.task(5), "Place 50 blocks");
    }
}
