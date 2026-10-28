//! Friends and rights on claims in play (milestone 25 part 1,
//! `docs/milestones/25-guilds.md`). Every rule that once asked "is this the
//! claim's owner?" asks [`Game::may`] instead: the owner may do anything on
//! their claim, a friend what the owner gave them, anyone else nothing.

use super::*;
use bm_core::rights::{self, Act};
use bm_protocol::{ClaimView, FriendAction, FriendView, FriendsView};

impl Game {
    /// Whether `account` may do `act` at `canon` on planet `pi`: anywhere
    /// unclaimed, anywhere on their own claims, and on a friend's claim
    /// where that friend gave them the right.
    pub(super) fn may(&self, pi: usize, canon: BlockPos, account: &str, act: Act) -> Result<(), Refusal> {
        let Some(c) = self.planets[pi].claims.at_block(canon) else {
            return Ok(());
        };
        self.may_on(c, account, act)
    }

    pub(super) fn may_on(&self, c: &crate::claims::Claim, account: &str, act: Act) -> Result<(), Refusal> {
        if c.owner == account {
            return Ok(());
        }
        let in_guild = self.guild_rights_on(c.id, account) != 0;
        if !in_guild && !self.social.are_friends(&c.owner, account) {
            return Err(ClaimError::OwnedBy(c.owner_name.clone()).into());
        }
        if rights::allows(self.guild_rights_on(c.id, account), act) {
            return Ok(());
        }
        if !self.social.are_friends(&c.owner, account) {
            return Err(ClaimError::OwnedBy(c.owner_name.clone()).into());
        }
        if rights::allows(self.social.rights(c.id, account), act) {
            Ok(())
        } else {
            Err(Refusal::Told(format!(
                "{} has not given you {} rights here",
                c.owner_name,
                act.right_name()
            )))
        }
    }

    /// What placing or breaking `block` needs: the working blocks (storage,
    /// machines, coils, shops, locks, conduits) Engineer, crops Farmhand,
    /// everything else Builder.
    pub(super) fn act_for_block(&self, pi: usize, block: BlockId) -> Act {
        let def = self.planets[pi].world.registry.get(block);
        let working = def.storage > 0
            || def.station.is_some()
            || def.assembly.is_some()
            || def.powers.is_some()
            || block == self.lock
            || block == self.portal_conduit;
        if working {
            Act::Engineer
        } else if self.crops.of_block(block).is_some() || self.crops.sown_as(block).is_some() {
            Act::Farm
        } else {
            Act::Build
        }
    }

    /// Whether `account` holds any right on `claim` (a visitor who does
    /// pays no footfall there).
    pub(super) fn holds_rights(&self, claim: u32, account: &str) -> bool {
        self.social.rights(claim, account) != 0 || self.guild_rights_on(claim, account) != 0
    }

    /// A claim as players see it, with who holds rights there.
    pub(super) fn claim_view(&self, c: &crate::claims::Claim) -> ClaimView {
        let mut v = c.view();
        v.rights = self
            .social
            .holders(c.id)
            .map(|h| h.values().map(|(name, r)| (name.clone(), *r)).collect())
            .unwrap_or_default();
        v.guild = self
            .guilds
            .alignment(c.id)
            .and_then(|a| self.guilds.get(a.guild).map(|g| (g.id, g.tag.clone(), a.controlled)));
        v
    }

    pub(super) fn claim_views(&self, pi: usize) -> Vec<ClaimView> {
        self.planets[pi].claims.all().map(|c| self.claim_view(c)).collect()
    }

    /// The friends window's view for player `id`.
    fn friends_view(&self, account: &str) -> FriendsView {
        let Some(f) = self.social.of(account) else {
            return FriendsView::default();
        };
        FriendsView {
            friends: f
                .friends
                .iter()
                .map(|(acct, name)| FriendView {
                    name: name.clone(),
                    planet: self
                        .player_by_account(acct)
                        .map(|oid| self.planets[self.players[&oid].planet].name.clone()),
                })
                .collect(),
            asked_by: f.asked_by.values().cloned().collect(),
        }
    }

    pub(super) fn send_friends(&self, id: u32) {
        if let Some(p) = self.players.get(&id) {
            p.out.send(&ServerMsg::Friends(self.friends_view(&p.account)));
        }
    }

    /// `account`'s friends who are playing see them come, go or travel.
    pub(super) fn tell_friends_of(&self, account: &str) {
        for friend in self.social.friends_of(account) {
            if let Some(oid) = self.player_by_account(friend) {
                self.send_friends(oid);
            }
        }
    }

    /// The account and shown name for a typed name: someone playing, or
    /// any account in the store.
    fn find_account(&self, name: &str) -> Option<(String, String)> {
        let key = crate::accounts::key(name.trim());
        if let Some(oid) = self.player_by_account(&key) {
            return Some((key, self.players[&oid].name.clone()));
        }
        match self.store.account(&key) {
            Ok(Some(r)) => Some((key, r.name)),
            Ok(None) => None,
            Err(e) => {
                tracing::error!("looking up `{key}`: {e:#}");
                None
            }
        }
    }

    pub(super) fn friend(&mut self, id: u32, name: &str, action: FriendAction) {
        let Some((me, my_name)) = self.players.get(&id).map(|p| (p.account.clone(), p.name.clone())) else {
            return;
        };
        let Some((them, their_name)) = self.find_account(name) else {
            return self.notice(id, &format!("There is no player called {}", name.trim()));
        };
        if them == me {
            return self.notice(id, "You cannot be your own friend");
        }
        use crate::social::Asked;
        match action {
            FriendAction::Ask => match self.social.ask(&me, &my_name, &them, &their_name) {
                Asked::Sent => {
                    self.notice(id, &format!("Asked {their_name} to be your friend"));
                    if let Some(oid) = self.player_by_account(&them) {
                        self.notice(oid, &format!("{my_name} asks to be your friend (O for friends)"));
                        self.send_friends(oid);
                    }
                }
                Asked::Friends => self.became_friends(id, &them, &their_name, &my_name),
                Asked::AlreadyFriends => self.notice(id, &format!("{their_name} is already your friend")),
                Asked::AlreadyAsked => self.notice(id, &format!("You have already asked {their_name}")),
                Asked::Full => self.notice(id, "A friends list is full"),
            },
            FriendAction::Accept => {
                if self.social.accept(&me, &my_name, &them) {
                    self.became_friends(id, &them, &their_name, &my_name);
                } else {
                    self.notice(id, &format!("{their_name} has not asked to be your friend"));
                }
            }
            FriendAction::Decline => {
                self.social.decline(&me, &them);
            }
            FriendAction::Remove => {
                let planets = &self.planets;
                let changed = self.social.unfriend(&me, &them, |claim| {
                    planets
                        .iter()
                        .find_map(|p| p.claims.get(claim))
                        .map(|c| c.owner.clone())
                });
                if let Some(oid) = self.player_by_account(&them) {
                    self.notice(oid, &format!("{my_name} is no longer your friend"));
                    self.send_friends(oid);
                }
                self.notice(id, &format!("{their_name} is no longer your friend"));
                for claim in changed {
                    self.claim_rights_changed(claim);
                }
            }
        }
        self.send_friends(id);
    }

    fn became_friends(&mut self, id: u32, them: &str, their_name: &str, my_name: &str) {
        self.notice(id, &format!("You and {their_name} are friends"));
        if let Some(oid) = self.player_by_account(them) {
            self.notice(oid, &format!("You and {my_name} are friends"));
            self.send_friends(oid);
        }
    }

    /// Everyone on the claim's planet sees who now holds rights there.
    pub(super) fn claim_rights_changed(&mut self, claim: u32) {
        if let Some(pi) = self.planets.iter().position(|p| p.claims.get(claim).is_some()) {
            self.broadcast_claim(pi, claim);
        }
    }

    pub(super) fn set_rights(&mut self, id: u32, claim: u32, friend: &str, bits: u8) {
        let Some(me) = self.players.get(&id).map(|p| p.account.clone()) else {
            return;
        };
        let Some(c) = self.planets.iter().find_map(|p| p.claims.get(claim)) else {
            return;
        };
        if c.owner != me {
            return self.notice(id, "Only a claim's owner gives rights on it");
        }
        let Some((them, their_name)) = self.find_account(friend) else {
            return self.notice(id, &format!("There is no player called {}", friend.trim()));
        };
        if bits != 0 && !self.social.are_friends(&me, &them) {
            return self.notice(id, &format!("Only friends get rights: {their_name} is not your friend"));
        }
        if !self.social.set_rights(claim, &them, &their_name, bits) {
            return self.notice(
                id,
                &format!(
                    "A claim can give rights to {} friends at most",
                    crate::social::MAX_HOLDERS
                ),
            );
        }
        self.claim_rights_changed(claim);
        if let Some(oid) = self.player_by_account(&them) {
            let owner = self.players[&id].name.clone();
            self.notice(
                oid,
                &format!("{owner} gave you {} on a claim", rights::describe(bits & rights::ALL)),
            );
        }
    }
}
