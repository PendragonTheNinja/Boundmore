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

#[cfg(test)]
mod tests {
    use super::*;
    use bm_core::{BlockPos, BlockRegistry, ColourPalette, Inventory, ItemRegistry, RecipeBook};
    use bm_protocol::ShopKind;

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

    fn view(kind: ShopKind, slots: Vec<Option<ItemStack>>, prices: Vec<u64>) -> ShopView {
        ShopView {
            pos: BlockPos::new(1, 2, 3),
            kind,
            owner: "Alice".into(),
            mine: false,
            slots,
            prices,
            till: 100,
            fee_permille: 100,
            closed: false,
        }
    }

    #[test]
    fn a_stand_offers_only_priced_stock_and_coin_limits_a_purchase() {
        let i = items();
        let stone = i.registry.id("stone").unwrap();
        let v = view(
            ShopKind::Stand,
            vec![Some(ItemStack::new(stone, 10)), Some(ItemStack::new(stone, 5)), None],
            vec![50, 0, 9],
        );
        let o = offers(&v, &i);
        assert_eq!(o.len(), 1, "unpriced and empty slots are not for sale");
        assert_eq!((o[0].slot, o[0].price, o[0].stack.count), (0, 50, 10));
        assert_eq!(affordable(&o[0], 120, 10), 2, "120 coin buys two");
        assert_eq!(affordable(&o[0], 10_000, 99), 10, "no more than there is");
    }

    #[test]
    fn a_basket_wants_what_it_holds_and_finds_it_in_the_pack() {
        let mut i = items();
        let stone = i.registry.id("stone").unwrap();
        let v = view(ShopKind::Basket, vec![Some(ItemStack::new(stone, 1))], vec![10]);
        assert_eq!(wants(&v, &i)[0].have, None, "none carried");
        i.inventory.slots[4] = Some(ItemStack::new(stone, 7));
        let w = wants(&v, &i);
        assert_eq!((w[0].slot, w[0].price, w[0].have), (0, 10, Some((4, 7))));
        assert!(is_shop("shop_stand") && is_shop("request_basket") && !is_shop("storage_chest"));
    }
}
