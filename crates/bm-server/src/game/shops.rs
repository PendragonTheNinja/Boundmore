//! Shop stands and request baskets (milestone 24 part 2,
//! `docs/milestones/24-coin-and-trade.md`). Their items are the block's
//! storage slots (the owner stocks them as storage); prices and the till are
//! `Planet::shops`. Every trade changes the pack, the ledger and the stand
//! in one tick, so the next save carries all three together.

use super::*;
use bm_core::trade::{self, MAX_PRICE, ShopRecord};
use bm_protocol::{ShopAction, ShopKind, ShopView};

/// Where a shop is and whose.
struct ShopAt {
    pi: usize,
    canon: BlockPos,
    kind: ShopKind,
    size: usize,
    owner: String,
    owner_name: String,
}

impl Game {
    pub(super) fn shop_kind(&self, block: BlockId) -> Option<ShopKind> {
        if block == self.shop_stand {
            Some(ShopKind::Stand)
        } else if block == self.request_basket {
            Some(ShopKind::Basket)
        } else {
            None
        }
    }

    /// A shop stand or basket this player can reach (anyone's).
    fn shop_access(&self, id: u32, pos: BlockPos) -> Result<ShopAt, Refusal> {
        let (canon, block) = self.block_in_reach(id, pos)?;
        let pi = self.planet_of(id).ok_or("no such player")?;
        let kind = self.shop_kind(block).ok_or("not a shop")?;
        let planet = &self.planets[pi];
        let claim = planet
            .claims
            .at_block(canon)
            .ok_or_else(|| Refusal::Told("This stand stands on no claim, so it trades with nobody".into()))?;
        Ok(ShopAt {
            pi,
            canon,
            kind,
            size: planet.world.registry.get(block).storage as usize,
            owner: claim.owner.clone(),
            owner_name: claim.owner_name.clone(),
        })
    }

    fn shop_view(&self, at: &ShopAt, viewer: &str) -> ShopView {
        let rec = self.planets[at.pi].shops.get(at.canon);
        let mine = viewer == at.owner;
        // Who pays the fee: a stand's owner (at the rate it was last opened
        // with), or a basket's seller (at their own).
        let merchant = match at.kind {
            ShopKind::Stand => rec.merchant,
            ShopKind::Basket => self
                .player_by_account(viewer)
                .is_some_and(|id| self.players[&id].effects.merchant),
        };
        let slots = self.planets[at.pi].storage.slots(at.canon, at.size);
        ShopView {
            pos: at.canon,
            kind: at.kind,
            owner: at.owner_name.clone(),
            mine,
            prices: (0..slots.len()).map(|i| rec.price(i)).collect(),
            slots,
            till: if mine || at.kind == ShopKind::Basket {
                rec.till
            } else {
                0
            },
            fee_permille: trade::fee_permille(merchant),
            closed: false,
        }
    }

    fn closed_shop(pos: BlockPos) -> ServerMsg {
        ServerMsg::Shop(ShopView {
            pos,
            kind: ShopKind::Stand,
            owner: String::new(),
            mine: false,
            slots: Vec::new(),
            prices: Vec::new(),
            till: 0,
            fee_permille: trade::SALES_FEE_PERMILLE,
            closed: true,
        })
    }

    pub(super) fn open_shop(&mut self, id: u32, pos: Option<BlockPos>) {
        let Some(pos) = pos else {
            if let Some(p) = self.players.get_mut(&id)
                && let Some(at) = p.shop.take()
            {
                p.out.send(&Self::closed_shop(at));
            }
            return;
        };
        match self.shop_access(id, pos) {
            Ok(at) => {
                let account = self.players[&id].account.clone();
                // Its owner's visit applies their Merchant epic (or its loss).
                if account == at.owner {
                    let merchant = self.players[&id].effects.merchant;
                    let shops = &mut self.planets[at.pi].shops;
                    let mut rec = shops.get(at.canon);
                    if rec.merchant != merchant {
                        rec.merchant = merchant;
                        shops.set(at.canon, rec);
                    }
                }
                let view = self.shop_view(&at, &account);
                let mine = view.mine;
                if let Some(p) = self.players.get_mut(&id) {
                    p.shop = Some(at.canon);
                    p.out.send(&ServerMsg::Shop(view));
                }
                // Its owner stocks it as storage.
                if mine {
                    self.open_storage(id, Some(at.canon));
                }
            }
            Err(r) => {
                self.tell(id, r);
                if let Some(p) = self.players.get_mut(&id) {
                    p.shop = None;
                    p.out.send(&Self::closed_shop(pos));
                }
            }
        }
    }

    /// Sends a shop to everyone who has it open (each sees it as owner or
    /// customer).
    pub(super) fn push_shop(&self, pi: usize, canon: BlockPos) {
        let viewers: Vec<u32> = self
            .players
            .iter()
            .filter(|(_, p)| p.planet == pi && p.shop == Some(canon))
            .map(|(id, _)| *id)
            .collect();
        for id in viewers {
            match self.shop_access(id, canon) {
                Ok(at) => {
                    let p = &self.players[&id];
                    p.out.send(&ServerMsg::Shop(self.shop_view(&at, &p.account)));
                }
                Err(_) => self.players[&id].out.send(&Self::closed_shop(canon)),
            }
        }
    }

    pub(super) fn shop_action(&mut self, id: u32, pos: BlockPos, action: ShopAction) {
        let at = match self.shop_access(id, pos) {
            Ok(at) => at,
            Err(r) => return self.tell(id, r),
        };
        let Some(p) = self.players.get(&id) else { return };
        if p.shop != Some(at.canon) {
            return tracing::debug!("#{id} acted at a shop it has not open");
        }
        let mine = p.account == at.owner;
        let result = match action {
            ShopAction::SetPrice { slot, price } if mine => self.set_price(&at, slot, price),
            ShopAction::TakeTill if mine => self.take_till(id, &at),
            ShopAction::Charge { amount } if mine && at.kind == ShopKind::Basket => self.charge(id, &at, amount),
            ShopAction::Buy {
                slot,
                count,
                price,
                expect,
            } if !mine && at.kind == ShopKind::Stand => self.buy(id, &at, slot, count, price, expect),
            ShopAction::Sell {
                slot,
                from,
                count,
                price,
            } if !mine && at.kind == ShopKind::Basket => self.sell(id, &at, slot, from, count, price),
            _ => Err(Refusal::Quiet("that is not for this shop and viewer")),
        };
        if let Err(r) = result {
            self.tell(id, r);
        }
        self.push_shop(at.pi, at.canon);
        self.push_storage(at.pi, at.canon, at.size);
    }

    fn set_price(&mut self, at: &ShopAt, slot: u8, price: u64) -> Result<(), Refusal> {
        if slot as usize >= at.size {
            return Err("no such slot".into());
        }
        let shops = &mut self.planets[at.pi].shops;
        let mut rec = shops.get(at.canon);
        rec.set_price(slot as usize, price.min(MAX_PRICE));
        shops.set(at.canon, rec);
        Ok(())
    }

    fn take_till(&mut self, id: u32, at: &ShopAt) -> Result<(), Refusal> {
        let coin = self.planets[at.pi].shops.till(at.canon);
        if coin == 0 {
            return Ok(());
        }
        let account = self.players[&id].account.clone();
        let ledger = self.ledgers.get_mut(&account).ok_or("no ledger")?;
        ledger.coin = ledger.coin.saturating_add(coin);
        let shops = &mut self.planets[at.pi].shops;
        let mut rec = shops.get(at.canon);
        rec.till = 0;
        shops.set(at.canon, rec);
        self.ledgers_dirty.insert(account);
        self.ledgers_unsent.insert(id);
        Ok(())
    }

    fn charge(&mut self, id: u32, at: &ShopAt, amount: u64) -> Result<(), Refusal> {
        let account = self.players[&id].account.clone();
        let ledger = self.ledgers.get_mut(&account).ok_or("no ledger")?;
        let amount = amount.min(MAX_PRICE);
        if amount == 0 || !ledger.spend(amount) {
            return Err(Refusal::Told("You do not have that much coin".into()));
        }
        let shops = &mut self.planets[at.pi].shops;
        let mut rec = shops.get(at.canon);
        rec.till = rec.till.saturating_add(amount);
        shops.set(at.canon, rec);
        self.ledgers_dirty.insert(account);
        self.ledgers_unsent.insert(id);
        Ok(())
    }

    fn buy(
        &mut self,
        id: u32,
        at: &ShopAt,
        slot: u8,
        count: u16,
        shown: u64,
        expect: ItemStack,
    ) -> Result<(), Refusal> {
        let slot = slot as usize;
        let price = self.planets[at.pi].shops.get(at.canon).price(slot);
        if price == 0 {
            return Err(Refusal::Told("That is not for sale".into()));
        }
        let mut slots = self.planets[at.pi].storage.slots(at.canon, at.size);
        let Some(stack) = slots.get(slot).copied().flatten() else {
            return Err(Refusal::Told("Sold out".into()));
        };
        // What the buyer saw is what they get: the owner may have changed
        // the price or the stock since (no bait and switch).
        if price != shown || !stack.stacks_with(&expect) {
            return Err(Refusal::Told("The price or the stock changed: look again".into()));
        }
        let n = count.min(stack.count);
        if n == 0 {
            return Ok(());
        }
        let cost = price.saturating_mul(n as u64);
        let account = self.players[&id].account.clone();
        if self.ledgers.get(&account).is_none_or(|l| l.coin < cost) {
            return Err(Refusal::Told(format!("That costs {cost} coin")));
        }
        let p = self.players.get_mut(&id).expect("present");
        let before = p.inventory.clone();
        let mut pack = p.inventory.clone();
        if pack.add_stack(&self.items, ItemStack { count: n, ..stack }) > 0 {
            return Err(Refusal::Told("No room in your pack".into()));
        }
        p.inventory = pack;
        send_slots(p, &before);
        slots[slot] = (stack.count > n).then_some(ItemStack {
            count: stack.count - n,
            ..stack
        });
        self.planets[at.pi].storage.set(at.canon, slots);
        let fee = trade::fee(cost, self.planets[at.pi].shops.get(at.canon).merchant);
        self.ledgers.get_mut(&account).expect("checked").spend(cost);
        let shops = &mut self.planets[at.pi].shops;
        let mut rec: ShopRecord = shops.get(at.canon);
        rec.till = rec.till.saturating_add(cost - fee);
        shops.set(at.canon, rec);
        self.ledgers_dirty.insert(account);
        self.ledgers_unsent.insert(id);
        tracing::info!("#{id} bought {n} for {cost} coin at {:?} (fee {fee})", at.canon);
        self.count(id, "spent", cost);
        match self.player_by_account(&at.owner) {
            Some(owner) => {
                self.count(owner, "sold", 1);
                self.count(owner, "tax", fee);
            }
            // Away: counted now, paid when they next join.
            None => self.note_away(&at.owner, &[("sold", 1), ("tax", fee)]),
        }
        Ok(())
    }

    fn sell(&mut self, id: u32, at: &ShopAt, slot: u8, from: u8, count: u16, shown: u64) -> Result<(), Refusal> {
        let slot = slot as usize;
        let rec = self.planets[at.pi].shops.get(at.canon);
        let price = rec.price(slot);
        if price != shown {
            return Err(Refusal::Told("The price changed: look again".into()));
        }
        let mut slots = self.planets[at.pi].storage.slots(at.canon, at.size);
        let Some(wanted) = slots.get(slot).copied().flatten().filter(|_| price > 0) else {
            return Err(Refusal::Told("That is not wanted here".into()));
        };
        let p = self.players.get(&id).expect("present");
        let Some(offered) = p.inventory.get(from as usize).filter(|s| s.stacks_with(&wanted)) else {
            return Err(Refusal::Told("That is not what this basket wants".into()));
        };
        let max = self.items.get(wanted.item).map_or(1, |d| d.stack);
        let room = max.saturating_sub(wanted.count);
        let n = trade::basket_takes(price, rec.till, room, count.min(offered.count));
        if n == 0 {
            return Err(Refusal::Told(if room == 0 {
                "This basket is full".into()
            } else {
                "This basket has no coin left to buy with".into()
            }));
        }
        let pay = price * n as u64;
        let fee = trade::fee(pay, p.effects.merchant);
        let account = p.account.clone();
        let p = self.players.get_mut(&id).expect("present");
        let before = p.inventory.clone();
        p.inventory.slots[from as usize] = (offered.count > n).then_some(ItemStack {
            count: offered.count - n,
            ..offered
        });
        send_slots(p, &before);
        slots[slot] = Some(ItemStack {
            count: wanted.count + n,
            ..wanted
        });
        self.planets[at.pi].storage.set(at.canon, slots);
        let shops = &mut self.planets[at.pi].shops;
        let mut rec = shops.get(at.canon);
        rec.till -= pay;
        shops.set(at.canon, rec);
        let ledger = self.ledgers.get_mut(&account).ok_or("no ledger")?;
        ledger.coin = ledger.coin.saturating_add(pay - fee);
        self.ledgers_dirty.insert(account);
        self.ledgers_unsent.insert(id);
        tracing::info!("#{id} sold {n} for {pay} coin at {:?} (fee {fee})", at.canon);
        self.count(id, "basket_sold", 1);
        self.count(id, "tax", fee);
        Ok(())
    }

    /// The player logged in as `account`, if any.
    pub(super) fn player_by_account(&self, account: &str) -> Option<u32> {
        self.players
            .iter()
            .find(|(_, p)| p.account == account)
            .map(|(id, _)| *id)
    }

    /// Closes every open shop its viewer can no longer reach.
    pub(super) fn check_open_shops(&mut self) {
        let open: Vec<(u32, BlockPos)> = self
            .players
            .iter()
            .filter_map(|(id, p)| p.shop.map(|s| (*id, s)))
            .collect();
        for (id, pos) in open {
            if self.shop_access(id, pos).is_err()
                && let Some(p) = self.players.get_mut(&id)
            {
                p.shop = None;
                p.out.send(&Self::closed_shop(pos));
            }
        }
    }

    /// Counts toward an absent account's feats (loading its ledger); the
    /// tiers reached are paid when they next join.
    pub(super) fn note_away(&mut self, account: &str, counts: &[(&str, u64)]) {
        if !self.ledgers.contains_key(account) {
            match self.store.ledger(account) {
                Ok(l) => {
                    self.ledgers.insert(account.to_string(), l);
                }
                Err(e) => return tracing::error!("counting feats for {account}: {e:#}"),
            }
        }
        let l = self.ledgers.get_mut(account).expect("loaded");
        for (key, n) in counts {
            l.note(key, *n);
        }
        self.ledgers_dirty.insert(account.to_string());
    }

    /// Coin to an account's ledger, loading it if they are away (a lapsed
    /// claim's tills). A ledger that cannot be read is reported; the coin is
    /// kept in the log so it can be given back by hand.
    pub(super) fn pay_account(&mut self, account: &str, coin: u64) {
        if coin == 0 {
            return;
        }
        if !self.ledgers.contains_key(account) {
            match self.store.ledger(account) {
                Ok(l) => {
                    self.ledgers.insert(account.to_string(), l);
                }
                Err(e) => {
                    // Kept and tried again at the next report, never lost.
                    tracing::error!("{coin} coin owed to {account} could not be paid yet: {e:#}");
                    *self.owed.entry(account.to_string()).or_default() += coin;
                    return;
                }
            }
        }
        let l = self.ledgers.get_mut(account).expect("loaded");
        l.coin = l.coin.saturating_add(coin);
        self.ledgers_dirty.insert(account.to_string());
        if let Some(id) = self.player_by_account(account) {
            self.ledgers_unsent.insert(id);
        }
    }
}
