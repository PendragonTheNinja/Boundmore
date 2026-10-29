//! The portal window, as the client sees it (ADR-0009).
//!
//! The server is the authority: it finds the portal, keeps its link and
//! tank, and answers every button. These functions only turn the last
//! `PortalView` and the inventory into what the window says and which of its
//! buttons can do anything, so an action that cannot apply is shown disabled
//! rather than sent to be refused.

use crate::items::Items;
use bm_core::{BlockPos, ItemKind};
use bm_protocol::{ClientMsg, PortalAction, PortalView};

/// "2 by 3": width, then height.
pub fn size_text(width: u8, height: u8) -> String {
    format!("{width} by {height}")
}

/// Why a portal is dormant, when the window can say: None when it is open
/// (or closed for a reason the view does not show).
pub fn dormant_because(v: &PortalView) -> Option<&'static str> {
    if v.open {
        None
    } else if v.link.is_none() {
        Some("Link it with a token from another portal of the same size")
    } else if v.fuel < v.trip_cost {
        Some("Add wayshards to open it")
    } else {
        None
    }
}

fn plural(n: u32, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

/// "1 hour", "3 hours", "30 minutes".
pub fn hours_text(hours: f32) -> String {
    if hours >= 1.0 && (hours - hours.round()).abs() < 0.01 {
        plural(hours.round() as u32, "hour", "hours")
    } else {
        plural((hours * 60.0).round().max(1.0) as u32, "minute", "minutes")
    }
}

/// "Each traveller pays 1 wayshard, at most once every 1 hour".
pub fn trip_text(v: &PortalView) -> String {
    format!(
        "Each traveller pays {}, at most once every {}",
        plural(v.trip_cost, "wayshard", "wayshards"),
        hours_text(v.cooldown_hours)
    )
}

/// The size at the start of a token's name ("2 by 3, Verdance, near 812,
/// 140"), if it has one.
fn size_in_name(name: &str) -> Option<(u8, u8)> {
    let first = name.split(',').next()?;
    let (w, h) = first.trim().split_once(" by ")?;
    Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
}

/// A portal token in the pack, as the Link list shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct TokenChoice {
    pub slot: u16,
    pub name: String,
    /// None when it can be used here; otherwise why not.
    pub refused: Option<&'static str>,
}

/// Every portal token carried, by slot, with what the server said it links
/// to. A token whose name gives another size is shown but cannot be chosen.
pub fn tokens(items: &Items, v: &PortalView) -> Vec<TokenChoice> {
    items
        .inventory
        .slots
        .iter()
        .enumerate()
        .filter_map(|(i, s)| {
            let s = (*s)?;
            if !matches!(items.def(&s)?.kind, ItemKind::PortalToken) {
                return None;
            }
            let name = items.token_name(s.tag).map(str::to_string);
            let refused = if s.tag == v.id {
                Some(OWN_TOKEN)
            } else {
                match name.as_deref().and_then(size_in_name) {
                    Some(size) if size != (v.width, v.height) => Some("Another size"),
                    _ => None,
                }
            };
            Some(TokenChoice {
                slot: i as u16,
                name: name.unwrap_or_else(|| "A portal token".into()),
                refused,
            })
        })
        .collect()
}

/// Why a token of this very portal cannot link it.
pub const OWN_TOKEN: &str = "This portal's own token";

/// Wayshards in the pack.
pub fn wayshards_carried(items: &Items) -> u32 {
    items.registry.id("wayshard").map_or(0, |w| items.inventory.count(w))
}

/// How many wayshards the pack has room for.
pub fn wayshard_room(items: &Items) -> u32 {
    let Ok(w) = items.registry.id("wayshard") else { return 0 };
    let limit = items.registry.get(w).map_or(1, |d| d.stack) as u32;
    items
        .inventory
        .slots
        .iter()
        .map(|s| match s {
            None => limit,
            Some(s) if s.item == w && s.tag == 0 && s.wear == 0 && s.colour == bm_core::Colour::NATURAL => {
                limit.saturating_sub(s.count as u32)
            }
            Some(_) => 0,
        })
        .sum()
}

/// What each of the window's buttons would send, None where it cannot
/// apply. Every list may be sent as it is.
#[derive(Clone, Debug, PartialEq)]
pub struct Buttons {
    pub make_token: Option<ClientMsg>,
    pub unlink: Option<ClientMsg>,
    /// One message per wayshard stack, filling the tank as far as it goes.
    pub add_fuel: Vec<ClientMsg>,
    pub take_all: Option<ClientMsg>,
    pub take_stack: Option<ClientMsg>,
}

pub fn buttons(items: &Items, v: &PortalView, pos: BlockPos) -> Buttons {
    let act = |action| ClientMsg::Portal { pos, action };
    let has_room = items.inventory.slots.iter().any(Option::is_none);
    let mut add_fuel = Vec::new();
    if let Ok(w) = items.registry.id("wayshard") {
        let mut space = v.capacity.saturating_sub(v.fuel);
        for (i, s) in items.inventory.slots.iter().enumerate() {
            let Some(s) = s.filter(|s| s.item == w) else { continue };
            if space == 0 {
                break;
            }
            let count = (s.count as u32).min(space);
            space -= count;
            add_fuel.push(act(PortalAction::AddFuel {
                slot: i as u16,
                count: count as u16,
            }));
        }
    }
    let room = wayshard_room(items).min(v.fuel);
    let stack = items
        .registry
        .id("wayshard")
        .ok()
        .and_then(|w| items.registry.get(w))
        .map_or(1, |d| d.stack as u32);
    let take = |n: u32| {
        (n > 0).then(|| {
            act(PortalAction::TakeFuel {
                count: n.min(u16::MAX as u32) as u16,
            })
        })
    };
    Buttons {
        make_token: has_room.then(|| act(PortalAction::MakeToken)),
        unlink: v.link.is_some().then(|| act(PortalAction::Unlink)),
        add_fuel,
        take_all: take(room),
        // Only offered when it differs from taking everything.
        take_stack: if room > stack { take(stack) } else { None },
    }
}

/// The message choosing the token in `slot`.
pub fn link_request(pos: BlockPos, slot: u16) -> ClientMsg {
    ClientMsg::Portal {
        pos,
        action: PortalAction::Link { slot },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bm_core::{BlockRegistry, ColourPalette, Inventory, ItemRegistry, ItemStack, RecipeBook};

    fn items() -> Items {
        let blocks = BlockRegistry::builtin();
        let reg = ItemRegistry::builtin(&blocks);
        let book = RecipeBook::builtin(&reg);
        Items::new(
            reg.defs().to_vec(),
            book.recipes,
            Inventory::default(),
            ColourPalette::builtin(),
            &blocks,
        )
        .unwrap()
    }

    fn view() -> PortalView {
        PortalView {
            id: 99,
            corner: BlockPos::new(10, 44, 10),
            width: 2,
            height: 3,
            link: Some("Verdance, near 812, 140".into()),
            open: true,
            fuel: 495,
            capacity: 500,
            trip_cost: 1,
            cooldown_hours: 1.0,
        }
    }

    const POS: BlockPos = BlockPos { x: 10, y: 45, z: 10 };

    fn act(action: PortalAction) -> ClientMsg {
        ClientMsg::Portal { pos: POS, action }
    }

    #[test]
    fn the_window_says_what_the_portal_is() {
        let v = view();
        assert_eq!(size_text(v.width, v.height), "2 by 3");
        assert_eq!(dormant_because(&v), None);
        assert_eq!(
            trip_text(&v),
            "Each traveller pays 1 wayshard, at most once every 1 hour"
        );
        let empty = PortalView {
            open: false,
            fuel: 0,
            ..view()
        };
        assert_eq!(dormant_because(&empty), Some("Add wayshards to open it"));
        let unlinked = PortalView {
            open: false,
            link: None,
            ..view()
        };
        assert!(dormant_because(&unlinked).unwrap().starts_with("Link it"));
        let far = PortalView {
            trip_cost: 2,
            cooldown_hours: 0.5,
            ..view()
        };
        assert_eq!(
            trip_text(&far),
            "Each traveller pays 2 wayshards, at most once every 30 minutes"
        );
        assert_eq!(hours_text(3.0), "3 hours");
        for t in [trip_text(&far), dormant_because(&unlinked).unwrap().into()] {
            assert!(!t.contains('\u{2014}'), "no em dashes in player text");
        }
    }

    /// M8 follow-up: a token of this very portal is shown but cannot link it.
    #[test]
    fn this_portals_own_token_is_refused() {
        let mut i = items();
        let t = i.registry.id("portal_token").unwrap();
        i.inventory.slots[0] = Some(ItemStack::tagged(t, 99));
        i.inventory.slots[1] = Some(ItemStack::tagged(t, 8));
        let got = tokens(&i, &view());
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].refused, Some(OWN_TOKEN));
        assert_eq!(got[1].refused, None);
    }

    #[test]
    fn buttons_are_enabled_only_where_they_can_act() {
        let mut i = items();
        let w = i.registry.id("wayshard").unwrap();
        // Nothing carried: no fuel to add; the tank's 495 fit in the pack.
        let b = buttons(&i, &view(), POS);
        assert!(b.add_fuel.is_empty());
        assert_eq!(b.make_token, Some(act(PortalAction::MakeToken)));
        assert_eq!(b.unlink, Some(act(PortalAction::Unlink)));
        assert_eq!(b.take_all, Some(act(PortalAction::TakeFuel { count: 495 })));
        assert_eq!(b.take_stack, Some(act(PortalAction::TakeFuel { count: 100 })));
        // Two stacks carried: the tank has room for 5, taken from the first.
        i.inventory.slots[3] = Some(ItemStack::new(w, 4));
        i.inventory.slots[12] = Some(ItemStack::new(w, 30));
        let b = buttons(&i, &view(), POS);
        assert_eq!(
            b.add_fuel,
            vec![
                act(PortalAction::AddFuel { slot: 3, count: 4 }),
                act(PortalAction::AddFuel { slot: 12, count: 1 })
            ]
        );
        // An empty tank: nothing to take; roomy, so both stacks go in whole.
        let empty = PortalView { fuel: 0, ..view() };
        let b = buttons(&i, &empty, POS);
        assert_eq!(b.add_fuel.len(), 2);
        assert_eq!((b.take_all, b.take_stack), (None, None));
        // A full tank takes nothing more.
        let full = PortalView { fuel: 500, ..view() };
        assert!(buttons(&i, &full, POS).add_fuel.is_empty());
        // Unlinked: nothing to unlink. A small tank: one take button.
        let small = PortalView {
            link: None,
            fuel: 7,
            ..view()
        };
        let b = buttons(&i, &small, POS);
        assert_eq!(b.unlink, None);
        assert_eq!(b.take_all, Some(act(PortalAction::TakeFuel { count: 7 })));
        assert_eq!(b.take_stack, None);
        // A full pack: no token can be made, and only the room left on the
        // wayshard stacks can be taken.
        let stone = i.registry.id("stone").unwrap();
        for s in i.inventory.slots.iter_mut().filter(|s| s.is_none()) {
            *s = Some(ItemStack::new(stone, 1));
        }
        let b = buttons(&i, &view(), POS);
        assert_eq!(b.make_token, None);
        assert_eq!(b.take_all, Some(act(PortalAction::TakeFuel { count: 166 })));
    }

    #[test]
    fn tokens_are_listed_by_name_and_another_size_cannot_be_chosen() {
        let mut i = items();
        let token = i.registry.id("portal_token").unwrap();
        i.inventory.slots[2] = Some(ItemStack::tagged(token, 7));
        i.inventory.slots[20] = Some(ItemStack::tagged(token, 9));
        i.inventory.slots[21] = Some(ItemStack::tagged(token, 11));
        i.set_token_names(vec![
            (7, "2 by 3, Saffra, near 40, 900".into()),
            (9, "1 by 2, Hearthmoor, near 3, 4".into()),
        ]);
        let list = tokens(&i, &view());
        assert_eq!(
            list,
            vec![
                TokenChoice {
                    slot: 2,
                    name: "2 by 3, Saffra, near 40, 900".into(),
                    refused: None
                },
                TokenChoice {
                    slot: 20,
                    name: "1 by 2, Hearthmoor, near 3, 4".into(),
                    refused: Some("Another size")
                },
                TokenChoice {
                    slot: 21,
                    name: "A portal token".into(),
                    refused: None
                },
            ]
        );
        assert_eq!(link_request(POS, 2), act(PortalAction::Link { slot: 2 }));
        assert_eq!(size_in_name("12 by 3"), Some((12, 3)));
        assert_eq!(size_in_name("Somewhere, near 1, 2"), None);
    }
}
