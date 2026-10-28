//! Rights on a claim (milestone 25 part 1, `docs/milestones/25-guilds.md`):
//! what a claim's owner may give a friend there. Boundless's three
//! (Builder, Worker, Engineer) and Farmhand, which its players asked for
//! (Nathan, 2026-10-28). Only the owner adds or gives up plots or removes
//! the claim, whatever rights others hold.

/// Places and breaks ordinary blocks, chisels, writes signs.
pub const BUILDER: u8 = 1;
/// Uses what is there: storage, machines and crafting, locked doors,
/// mending with a spanner.
pub const WORKER: u8 = 2;
/// Places and breaks the working blocks: machines, coils, storage, locks.
pub const ENGINEER: u8 = 4;
/// Tills, sows and harvests crops, and nothing else.
pub const FARMHAND: u8 = 8;
pub const ALL: u8 = BUILDER | WORKER | ENGINEER | FARMHAND;

/// Each right with its name, in the order windows show them.
pub const NAMED: [(u8, &str); 4] = [
    (BUILDER, "Builder"),
    (WORKER, "Worker"),
    (ENGINEER, "Engineer"),
    (FARMHAND, "Farmhand"),
];

/// What an action on a claim needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    Build,
    Work,
    Engineer,
    Farm,
}

impl Act {
    /// The right it needs, by name, for refusals.
    pub fn right_name(self) -> &'static str {
        match self {
            Act::Build => "Builder",
            Act::Work => "Worker",
            Act::Engineer => "Engineer",
            Act::Farm => "Farmhand",
        }
    }
}

/// Whether `rights` allow `act`. A builder may also farm (crops are blocks
/// they could place and break anyway).
pub fn allows(rights: u8, act: Act) -> bool {
    match act {
        Act::Build => rights & BUILDER != 0,
        Act::Work => rights & WORKER != 0,
        Act::Engineer => rights & ENGINEER != 0,
        Act::Farm => rights & (FARMHAND | BUILDER) != 0,
    }
}

/// "Builder and Worker", or "no rights".
pub fn describe(rights: u8) -> String {
    let names: Vec<&str> = NAMED.iter().filter(|(r, _)| rights & r != 0).map(|(_, n)| *n).collect();
    match names.as_slice() {
        [] => "no rights".into(),
        [one] => one.to_string(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_right_allows_its_own_acts() {
        assert!(allows(BUILDER, Act::Build) && allows(BUILDER, Act::Farm));
        assert!(!allows(BUILDER, Act::Work) && !allows(BUILDER, Act::Engineer));
        assert!(allows(FARMHAND, Act::Farm) && !allows(FARMHAND, Act::Build));
        assert!(allows(WORKER, Act::Work) && !allows(WORKER, Act::Build));
        assert!(allows(ENGINEER, Act::Engineer) && !allows(ENGINEER, Act::Work));
        assert!(!allows(0, Act::Farm));
        assert_eq!(describe(0), "no rights");
        assert_eq!(describe(WORKER), "Worker");
        assert_eq!(describe(BUILDER | WORKER | FARMHAND), "Builder, Worker and Farmhand");
    }
}
