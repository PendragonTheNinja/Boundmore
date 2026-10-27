//! Shop stands and request baskets as the client sees them (milestone 24
//! part 2). The server rules every trade; these turn the last `ShopView` and
//! the pack into what the window lists and offers.

use crate::items::Items;
use bm_core::ItemStack;
use bm_protocol::ShopView;

/// Whether a block of this name trades.
pub fn is_shop(name: &str) -> bool {
    matches!(name, "shop_stand" | "request_basket")
}

/// A stand's slot for sale.
#[derive(Clone, Debug, PartialEq)]
pub struct Offer {
    pub slot: u8,
    pub stack: ItemStack,
    pub name: String,
    pub price: u64,
}

/// What a stand sells: every stocked slot with a price.
pub fn offers(view: &ShopView, items: &Items) -> Vec<Offer> {
    view.slots
        .iter()
        .enumerate()
        .filter_map(|(i, s)| {
            let s = (*s)?;
            let price = view.prices.get(i).copied().unwrap_or(0);
            (price > 0).then(|| Offer {
                slot: i as u8,
                stack: s,
                name: items.name(&s),
                price,
            })
        })
        .collect()
}

/// A basket's slot asking for more.
#[derive(Clone, Debug, PartialEq)]
pub struct Want {
    pub slot: u8,
    pub stack: ItemStack,
    pub name: String,
    pub price: u64,
    /// The first pack slot holding what it wants, and how many are there.
    pub have: Option<(u8, u16)>,
}

/// What a basket buys, and where in the pack each is.
pub fn wants(view: &ShopView, items: &Items) -> Vec<Want> {
    view.slots
        .iter()
        .enumerate()
        .filter_map(|(i, s)| {
            let s = (*s)?;
            let price = view.prices.get(i).copied().unwrap_or(0);
            if price == 0 {
                return None;
            }
            let have = items
                .inventory
                .slots
                .iter()
                .enumerate()
                .find_map(|(j, p)| p.filter(|p| p.stacks_with(&s)).map(|p| (j as u8, p.count)));
            Some(Want {
                slot: i as u8,
                stack: s,
                name: items.name(&s),
                price,
                have,
            })
        })
        .collect()
}

/// How many a buyer with `coin` can afford of an offer, up to `want`.
pub fn affordable(offer: &Offer, coin: u64, want: u16) -> u16 {
    let can = (coin / offer.price).min(u16::MAX as u64) as u16;
    want.min(offer.stack.count).min(can)
}
