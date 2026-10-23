//! A forge's contents (milestone 17, `docs/milestones/17-forging.md`): the
//! session running in it and the sealed item in its queue, kept in the
//! forge's station record (`crafting::StationData::forge`) and saved with it.
//!
//! The rules of a session are `bm_core::forge`'s; this holds who owns it and
//! how it is saved. Items are saved by name ([`SavedStack`]), never by
//! catalogue id; traits by their append-only number.

use crate::store::SavedStack;
use bm_core::forge::{Buff, DeckSlot, Dice, Gear, QueueBand, RoundReport, Session, SessionStats, TraitId};
use bm_core::{ItemKind, ItemRegistry, ItemStack, TICK_HZ};
use serde::{Deserialize, Serialize};

/// A session and its forger.
#[derive(Clone, Debug, PartialEq)]
pub struct Forging {
    /// Lowercased account name of the forger: only they go on with it.
    pub account: String,
    /// Their character's name, shown to others at the forge.
    pub name: String,
    pub session: Session,
}

/// A sealed item in the forge's queue.
#[derive(Clone, Debug, PartialEq)]
pub struct Sealed {
    /// The item with its traits.
    pub item: ItemStack,
    /// What its time costs and gives (its rank's band; the seconds may be a
    /// test's shorter ones).
    pub band: QueueBand,
    /// Planet tick its time began; None while it waits for spark or a
    /// repair (nothing paid yet).
    pub started: Option<u64>,
}

impl Sealed {
    pub fn ticks(&self) -> u64 {
        (self.band.seconds as u64 * TICK_HZ as u64).max(1)
    }

    /// How far along (0..1) and seconds left at `tick`.
    pub fn progress(&self, tick: u64) -> (f32, u32) {
        let Some(started) = self.started else {
            return (0.0, self.band.seconds);
        };
        let done = tick.saturating_sub(started).min(self.ticks());
        let left = (self.ticks() - done).div_ceil(TICK_HZ as u64) as u32;
        (done as f32 / self.ticks() as f32, left)
    }
}

/// Everything a forge holds besides finished output.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ForgeData {
    pub session: Option<Forging>,
    pub queued: Option<Sealed>,
}

impl ForgeData {
    pub fn is_empty(&self) -> bool {
        self.session.is_none() && self.queued.is_none()
    }

    /// Everything in it as items (a claim lapsing): the gear unforged and
    /// the deck left, the sealed item. Ingredients already used are spent.
    pub fn items(&self) -> Vec<ItemStack> {
        let mut out = Vec::new();
        if let Some(f) = &self.session {
            out.push(f.session.item);
            out.extend(f.session.leftovers());
        }
        out.extend(self.queued.as_ref().map(|q| q.item));
        out
    }
}

// ---------------------------------------------------------------- saved

/// A forge's contents as saved (save version 11).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SavedForge {
    pub session: Option<SavedSession>,
    pub queued: Option<SavedSealed>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedSession {
    pub account: String,
    pub name: String,
    pub item: SavedStack,
    pub gear: Gear,
    pub stats: SessionStats,
    pub stability: i32,
    pub vigour: i32,
    pub pools: [i32; 3],
    /// Traits by kind, each by its number with its points.
    pub traits: [Vec<(u8, u32)>; 3],
    pub buffs: Vec<Buff>,
    /// Deck slots in order (emptied ones too: slots are addressed by place).
    pub deck: Vec<SavedStack>,
    pub used: Vec<SavedStack>,
    pub dice: u64,
    pub rounds: u32,
    pub last: SavedReport,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SavedReport {
    pub rolls: [Option<(f64, bool)>; 3],
    pub landed: Vec<(u8, u32)>,
    pub vigour_spent: i32,
    pub stability_spent: i32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedSealed {
    pub item: SavedStack,
    pub seconds: u32,
    pub spark: u32,
    pub wear: u32,
    pub xp: u32,
    pub started: Option<u64>,
}

fn save_traits(t: &[(TraitId, u32)]) -> Vec<(u8, u32)> {
    t.iter().map(|&(id, p)| (id as u8, p)).collect()
}

fn load_traits(t: &[(u8, u32)]) -> Vec<(TraitId, u32)> {
    t.iter()
        .filter_map(|&(id, p)| TraitId::from_u8(id).map(|id| (id, p)))
        .collect()
}

pub fn save_forge(f: &ForgeData, items: &ItemRegistry) -> SavedForge {
    let stack = |s: &ItemStack| SavedStack::new(items, s);
    SavedForge {
        session: f.session.as_ref().and_then(|f| {
            let s = &f.session;
            Some(SavedSession {
                account: f.account.clone(),
                name: f.name.clone(),
                item: stack(&s.item)?,
                gear: s.gear,
                stats: s.stats,
                stability: s.stability,
                vigour: s.vigour,
                pools: s.pools,
                traits: [
                    save_traits(&s.traits[0]),
                    save_traits(&s.traits[1]),
                    save_traits(&s.traits[2]),
                ],
                buffs: s.buffs.clone(),
                deck: s.deck.iter().filter_map(|d| stack(&d.stack)).collect(),
                used: s.used.iter().filter_map(stack).collect(),
                dice: s.dice.0,
                rounds: s.rounds,
                last: SavedReport {
                    rolls: s.last.rolls,
                    landed: save_traits(&s.last.landed),
                    vigour_spent: s.last.vigour_spent,
                    stability_spent: s.last.stability_spent,
                },
            })
        }),
        queued: f.queued.as_ref().and_then(|q| {
            Some(SavedSealed {
                item: stack(&q.item)?,
                seconds: q.band.seconds,
                spark: q.band.spark,
                wear: q.band.wear,
                xp: q.band.xp,
                started: q.started,
            })
        }),
    }
}

/// An ingredient stack as a deck slot, if it is one in this catalogue.
pub fn deck_slot(items: &ItemRegistry, stack: ItemStack) -> Option<DeckSlot> {
    match items.get(stack.item)?.kind {
        ItemKind::Ingredient(ingredient) => Some(DeckSlot { stack, ingredient }),
        _ => None,
    }
}

/// Reads a saved forge. Gear whose item no longer exists ends its session
/// (reported); a deck ingredient no longer known becomes an empty slot.
pub fn load_forge(saved: &SavedForge, items: &ItemRegistry, what: &str) -> ForgeData {
    let session = saved.session.as_ref().and_then(|s| {
        let Some(item) = s.item.resolve(items) else {
            tracing::error!(
                "{what}: the gear being forged (`{}`) no longer exists; session lost",
                s.item.item
            );
            return None;
        };
        let deck = s
            .deck
            .iter()
            .map(|d| {
                d.resolve(items).and_then(|st| deck_slot(items, st)).unwrap_or_else(|| {
                    tracing::error!("{what}: deck ingredient `{}` no longer exists; lost", d.item);
                    DeckSlot {
                        stack: ItemStack::new(item.item, 0),
                        ingredient: u8::MAX,
                    }
                })
            })
            .collect();
        let session = Session {
            item,
            gear: s.gear,
            stats: s.stats,
            stability: s.stability,
            vigour: s.vigour,
            pools: s.pools,
            traits: [
                load_traits(&s.traits[0]),
                load_traits(&s.traits[1]),
                load_traits(&s.traits[2]),
            ],
            buffs: s.buffs.clone(),
            deck,
            used: s.used.iter().filter_map(|u| u.resolve(items)).collect(),
            dice: Dice(s.dice),
            rounds: s.rounds,
            last: RoundReport {
                rolls: s.last.rolls,
                landed: load_traits(&s.last.landed),
                vigour_spent: s.last.vigour_spent,
                stability_spent: s.last.stability_spent,
            },
        };
        Some(Forging {
            account: s.account.clone(),
            name: s.name.clone(),
            session,
        })
    });
    let queued = saved.queued.as_ref().and_then(|q| {
        let Some(item) = q.item.resolve(items) else {
            tracing::error!("{what}: the sealed item (`{}`) no longer exists; lost", q.item.item);
            return None;
        };
        Some(Sealed {
            item,
            band: QueueBand {
                seconds: q.seconds,
                spark: q.spark,
                wear: q.wear,
                xp: q.xp,
            },
            started: q.started,
        })
    });
    ForgeData { session, queued }
}
