//! Shop stands and request baskets (milestone 24 part 2,
//! `docs/milestones/24-coin-and-trade.md`): the rules both are traded by.
//! Their items are an ordinary storage block's slots; this is what they add:
//! a price per slot and a till of coin.

use serde::{Deserialize, Serialize};

/// The sales fee on every sale through a stand or basket, in tenths of a
/// percent, paid by the seller and taken out of the game: Boundless's 10%
/// (Nathan, 2026-10-28), a quarter off for a seller with the Merchant epic
/// (Boundless's Tax Epic).
pub const SALES_FEE_PERMILLE: u64 = 100;
pub const MERCHANT_FEE_PERMILLE: u64 = 75;

/// The fee rate for a seller, in tenths of a percent.
pub fn fee_permille(merchant: bool) -> u64 {
    if merchant {
        MERCHANT_FEE_PERMILLE
    } else {
        SALES_FEE_PERMILLE
    }
}

/// The fee on a sale of `total` coin, rounded up (any sale pays at least 1).
pub fn fee(total: u64, merchant: bool) -> u64 {
    if total == 0 {
        return 0;
    }
    total.saturating_mul(fee_permille(merchant)).div_ceil(1000).max(1)
}

/// What a stand or basket keeps beside its slots.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShopRecord {
    /// Coin a piece, by slot (0: not for sale, or not wanted).
    pub prices: Vec<u64>,
    /// Coin waiting in it: a stand's takings, or a basket's money to buy
    /// with.
    pub till: u64,
    /// Its owner had the Merchant epic when they last opened it: its sales
    /// pay the lower fee (Boundless applied the Tax Epic at a stand once its
    /// owner revisited it).
    pub merchant: bool,
}

impl ShopRecord {
    pub fn price(&self, slot: usize) -> u64 {
        self.prices.get(slot).copied().unwrap_or(0)
    }

    /// Sets a slot's price, growing the list as needed.
    pub fn set_price(&mut self, slot: usize, price: u64) {
        if self.prices.len() <= slot {
            self.prices.resize(slot + 1, 0);
        }
        self.prices[slot] = price;
        while self.prices.last() == Some(&0) {
            self.prices.pop();
        }
    }

    /// Nothing to keep: no price and no coin.
    pub fn is_empty(&self) -> bool {
        self.till == 0 && !self.merchant && self.prices.iter().all(|&p| p == 0)
    }
}

/// The most a buyer may pay or a basket may spend at once.
pub const MAX_PRICE: u64 = 1_000_000_000;

/// How many of `offered` a basket slot buys at `price` a piece, with `room`
/// left in its stack and `till` coin to pay with.
pub fn basket_takes(price: u64, till: u64, room: u16, offered: u16) -> u16 {
    if price == 0 {
        return 0;
    }
    let affordable = (till / price).min(u16::MAX as u64) as u16;
    offered.min(room).min(affordable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fee_is_ten_percent_and_merchants_pay_three_quarters_of_it() {
        assert_eq!(fee(0, false), 0);
        assert_eq!(fee(1, false), 1);
        assert_eq!(fee(100, false), 10);
        assert_eq!(fee(149, false), 15);
        assert_eq!(fee(10_000, false), 1000);
        assert_eq!(fee(100, true), 8, "7.5 rounded up");
        assert_eq!(fee(10_000, true), 750);
    }

    #[test]
    fn prices_grow_and_trim() {
        let mut r = ShopRecord::default();
        assert!(r.is_empty());
        r.set_price(3, 50);
        assert_eq!(r.prices, vec![0, 0, 0, 50]);
        assert_eq!(r.price(3), 50);
        assert_eq!(r.price(9), 0);
        r.set_price(3, 0);
        assert!(r.prices.is_empty() && r.is_empty());
        r.till = 1;
        assert!(!r.is_empty());
    }

    #[test]
    fn a_basket_buys_what_room_and_till_allow() {
        assert_eq!(basket_takes(10, 1000, 50, 20), 20);
        assert_eq!(basket_takes(10, 1000, 5, 20), 5, "stack room");
        assert_eq!(basket_takes(10, 45, 50, 20), 4, "till");
        assert_eq!(basket_takes(0, 1000, 50, 20), 0, "not wanted");
    }
}
