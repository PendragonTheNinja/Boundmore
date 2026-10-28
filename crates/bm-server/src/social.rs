//! Friends and the rights friends hold on claims (milestone 25 part 1,
//! `docs/milestones/25-guilds.md`). Friendship is mutual: one asks, the
//! other accepts. A claim's owner gives friends rights there
//! (`bm_core::rights`); ending a friendship takes away the rights each held
//! on the other's claims. Everything is kept in memory and saved by account
//! and by claim (`player_friends`, `claim_rights`).

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Most friends one account may have (**guess**: Boundless's limit is not
/// recorded).
pub const MAX_FRIENDS: usize = 200;
/// Most requests waiting for one account.
pub const MAX_ASKED: usize = 50;
/// Most friends holding rights on one claim (**guess**; it bounds what each
/// claim adds to the welcome).
pub const MAX_HOLDERS: usize = 50;

/// What changed since the last save: friends by account, rights by claim
/// (None deletes).
pub type SocialDirty = (Vec<(String, Option<SavedFriends>)>, Vec<(u32, Option<SavedRights>)>);

/// One account's friends and the requests waiting for it, by account, with
/// each as shown.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedFriends {
    pub friends: BTreeMap<String, String>,
    pub asked_by: BTreeMap<String, String>,
}

impl SavedFriends {
    pub fn is_empty(&self) -> bool {
        self.friends.is_empty() && self.asked_by.is_empty()
    }
}

/// Who holds rights on one claim: account -> (name as shown, rights).
pub type SavedRights = BTreeMap<String, (String, u8)>;

/// What asking someone to be a friend did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Asked {
    /// Waiting for them.
    Sent,
    /// They had asked us: now friends.
    Friends,
    AlreadyFriends,
    AlreadyAsked,
    /// One of the two lists is full.
    Full,
}

#[derive(Default)]
pub struct Social {
    friends: HashMap<String, SavedFriends>,
    rights: HashMap<u32, SavedRights>,
    friends_dirty: BTreeSet<String>,
    rights_dirty: BTreeSet<u32>,
}

impl Social {
    pub fn load(friends: Vec<(String, SavedFriends)>, rights: Vec<(u32, SavedRights)>) -> Self {
        Social {
            friends: friends.into_iter().collect(),
            rights: rights.into_iter().collect(),
            ..Default::default()
        }
    }

    pub fn of(&self, account: &str) -> Option<&SavedFriends> {
        self.friends.get(account)
    }

    pub fn are_friends(&self, a: &str, b: &str) -> bool {
        self.friends.get(a).is_some_and(|f| f.friends.contains_key(b))
    }

    /// Accounts that have `account` among their friends (it is mutual).
    pub fn friends_of(&self, account: &str) -> impl Iterator<Item = &String> {
        self.friends.get(account).into_iter().flat_map(|f| f.friends.keys())
    }

    fn entry(&mut self, account: &str) -> &mut SavedFriends {
        self.friends_dirty.insert(account.to_string());
        self.friends.entry(account.to_string()).or_default()
    }

    /// `from` (shown as `from_name`) asks `to` (shown as `to_name`).
    pub fn ask(&mut self, from: &str, from_name: &str, to: &str, to_name: &str) -> Asked {
        if self.are_friends(from, to) {
            return Asked::AlreadyFriends;
        }
        // They asked first: this answers it.
        if self.friends.get(from).is_some_and(|f| f.asked_by.contains_key(to)) {
            return if self.befriend(from, from_name, to, to_name) {
                Asked::Friends
            } else {
                Asked::Full
            };
        }
        if self.friends.get(to).is_some_and(|f| f.asked_by.contains_key(from)) {
            return Asked::AlreadyAsked;
        }
        if self.friends.get(to).is_some_and(|f| f.asked_by.len() >= MAX_ASKED)
            || self.friends.get(from).is_some_and(|f| f.friends.len() >= MAX_FRIENDS)
        {
            return Asked::Full;
        }
        self.entry(to).asked_by.insert(from.to_string(), from_name.to_string());
        Asked::Sent
    }

    /// `by` accepts `from`'s request. False if there was none, or a list is
    /// full.
    pub fn accept(&mut self, by: &str, by_name: &str, from: &str) -> bool {
        let Some(from_name) = self.friends.get(by).and_then(|f| f.asked_by.get(from)).cloned() else {
            return false;
        };
        self.befriend(by, by_name, from, &from_name)
    }

    fn befriend(&mut self, a: &str, a_name: &str, b: &str, b_name: &str) -> bool {
        let full = |s: &Self, k: &str| s.friends.get(k).is_some_and(|f| f.friends.len() >= MAX_FRIENDS);
        if full(self, a) || full(self, b) {
            return false;
        }
        let ea = self.entry(a);
        ea.asked_by.remove(b);
        ea.friends.insert(b.to_string(), b_name.to_string());
        let eb = self.entry(b);
        eb.asked_by.remove(a);
        eb.friends.insert(a.to_string(), a_name.to_string());
        true
    }

    /// `by` turns down `from`'s request.
    pub fn decline(&mut self, by: &str, from: &str) -> bool {
        let had = self.friends.get(by).is_some_and(|f| f.asked_by.contains_key(from));
        if had {
            self.entry(by).asked_by.remove(from);
        }
        had
    }

    /// Ends a friendship, and the rights each held on the other's claims
    /// (`owner_of` names a claim's owner). The claims whose rights changed.
    pub fn unfriend(&mut self, a: &str, b: &str, owner_of: impl Fn(u32) -> Option<String>) -> Vec<u32> {
        if !self.are_friends(a, b) {
            return Vec::new();
        }
        self.entry(a).friends.remove(b);
        self.entry(b).friends.remove(a);
        let mut changed = Vec::new();
        for (&claim, holders) in self.rights.iter_mut() {
            let owner = owner_of(claim);
            let other = match owner.as_deref() {
                Some(o) if o == a => b,
                Some(o) if o == b => a,
                _ => continue,
            };
            if holders.remove(other).is_some() {
                changed.push(claim);
            }
        }
        self.rights.retain(|_, h| !h.is_empty());
        self.rights_dirty.extend(changed.iter().copied());
        changed
    }

    /// The rights `account` holds on `claim`.
    pub fn rights(&self, claim: u32, account: &str) -> u8 {
        self.rights
            .get(&claim)
            .and_then(|h| h.get(account))
            .map_or(0, |&(_, r)| r)
    }

    /// Everyone holding rights on `claim`, by account.
    pub fn holders(&self, claim: u32) -> Option<&SavedRights> {
        self.rights.get(&claim)
    }

    /// Sets what `account` (shown as `name`) holds on `claim`; 0 takes all.
    /// False if the claim already has [`MAX_HOLDERS`] others.
    pub fn set_rights(&mut self, claim: u32, account: &str, name: &str, rights: u8) -> bool {
        if rights != 0
            && self
                .rights
                .get(&claim)
                .is_some_and(|h| h.len() >= MAX_HOLDERS && !h.contains_key(account))
        {
            return false;
        }
        let h = self.rights.entry(claim).or_default();
        if rights == 0 {
            h.remove(account);
        } else {
            h.insert(account.to_string(), (name.to_string(), rights & bm_core::rights::ALL));
        }
        if h.is_empty() {
            self.rights.remove(&claim);
        }
        self.rights_dirty.insert(claim);
        true
    }

    /// A claim is gone, and its rights with it.
    pub fn forget_claim(&mut self, claim: u32) {
        if self.rights.remove(&claim).is_some() {
            self.rights_dirty.insert(claim);
        }
    }

    pub fn take_dirty(&mut self) -> SocialDirty {
        let friends = std::mem::take(&mut self.friends_dirty)
            .into_iter()
            .map(|a| {
                let f = self.friends.get(&a).filter(|f| !f.is_empty()).cloned();
                (a, f)
            })
            .collect();
        let rights = std::mem::take(&mut self.rights_dirty)
            .into_iter()
            .map(|c| (c, self.rights.get(&c).cloned()))
            .collect();
        (friends, rights)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bm_core::rights::{BUILDER, WORKER};

    #[test]
    fn friends_are_asked_then_accepted_and_both_ways() {
        let mut s = Social::default();
        assert_eq!(s.ask("alice", "Alice", "bob", "Bob"), Asked::Sent);
        assert_eq!(s.ask("alice", "Alice", "bob", "Bob"), Asked::AlreadyAsked);
        assert!(!s.are_friends("alice", "bob"));
        assert!(s.accept("bob", "Bob", "alice"));
        assert!(s.are_friends("alice", "bob") && s.are_friends("bob", "alice"));
        assert_eq!(s.of("alice").unwrap().friends["bob"], "Bob");
        assert!(s.of("bob").unwrap().asked_by.is_empty());
        assert_eq!(s.ask("bob", "Bob", "alice", "Alice"), Asked::AlreadyFriends);
        // Asking someone who asked you is accepting.
        assert_eq!(s.ask("carol", "Carol", "alice", "Alice"), Asked::Sent);
        assert_eq!(s.ask("alice", "Alice", "carol", "Carol"), Asked::Friends);
        assert!(!s.decline("bob", "nobody"));
        let (f, _) = s.take_dirty();
        assert_eq!(f.len(), 3);
    }

    #[test]
    fn ending_a_friendship_takes_rights_both_ways() {
        let mut s = Social::default();
        s.ask("alice", "Alice", "bob", "Bob");
        s.accept("bob", "Bob", "alice");
        // Claim 1 is Alice's, 2 Bob's, 3 Carol's.
        s.set_rights(1, "bob", "Bob", BUILDER | WORKER);
        s.set_rights(2, "alice", "Alice", WORKER);
        s.set_rights(3, "bob", "Bob", BUILDER);
        assert_eq!(s.rights(1, "bob"), BUILDER | WORKER);
        let owner = |c: u32| Some(["", "alice", "bob", "carol"][c as usize].to_string());
        let mut changed = s.unfriend("alice", "bob", owner);
        changed.sort();
        assert_eq!(changed, vec![1, 2]);
        assert_eq!((s.rights(1, "bob"), s.rights(2, "alice")), (0, 0));
        assert_eq!(s.rights(3, "bob"), BUILDER, "Carol's claim untouched");
        s.set_rights(3, "bob", "Bob", 0);
        assert!(s.holders(3).is_none());
    }
}
