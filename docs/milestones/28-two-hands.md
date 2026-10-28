# Milestone 28: Two Hands

*Spec drafted 2026-10-28; Nathan picked it the same day ("Two hands" over sound, character looks and rented worlds). Waiting on his answers to the decisions below before it is built.*

Boundmore today has one hand: a 10-slot hotbar, one selected item, left click uses it (swing, shoot, fire a grapple, eat) and right click places a block or fills a chisel cut. Boundless had two hands, and players built their whole way of playing on them: a hammer in one hand and blocks in the other while bridging, a grapple in each hand to swing like Spider-Man, a sling and a bomb bag for a fight.

## What Boundless did (forum; the wiki has no page on it)

- **Each hand has its own bar of 8 slots**, drawn as a ring (filled from 12 o'clock clockwise). One slot in each hand is selected (topic 20299, 2018; 26429, 2019).
- **The left mouse button uses the left hand's item, the right button the right hand's**, whatever it is: a tool swings, a block places, a grapple fires, food is eaten (topics 7756, 26429).
- **Only one hand acts at a time**: two hammers swing left, right, left, right, never both at once ("we want to encourage more intelligent use of the system", a developer, topic 3195, 2015).
- **Dual grapples** (Release 152, 2017): a grapple in each hand, both hooks out at once; letting go releases the oldest first (topic 16292). Before hands existed players had eight hooks out; the hands limited it to two.
- **Choosing items:** the mouse wheel cycles the right hand, Alt and the wheel the left (topic 4741); a setting chose whether one press switched both hands ("Item Selection Radial Switching": both hands or only the hand used), and players disliked switching both by accident while hanging from a grapple (topics 25070, 23059, 37008). Shift-click puts an item in the left hand's next slot, Ctrl-click (or a right-click) in the right's (topics 13635, 26429).
- **A hand slot links to an inventory stack** rather than holding its own copy: the item stays in the inventory and the hand points at it (topic 20299, "link one of each to whichever hand slot"). A stack used up empties the link.
- What players asked for: action sets (swap all hand items at once, topic 48193); not having to give up inventory slots to fill hands (20299).

## Decisions for Nathan

These shape how the game is played every second and how inventories work (`CLAUDE.md`, "Stop and ask first"):

1. **Hand bars.** (a) **Boundless's: two rings of 8 slots, each a link to an inventory stack** (the item stays in the inventory; recommended), or (b) the current 10-slot hotbar split in two (left 1 to 5, right 6 to 0), items held in those slots as now.
2. **Buttons.** **Left button uses the left hand, right button the right hand, whatever each holds** (recommended, Boundless's). Placing a block and filling a chisel cut then belong to whichever hand holds the block or chisel. Today's "right click places" goes away.
3. **One hand at a time** (Boundless's, recommended) or both hands may act together (faster, but Boundless's developers ruled it out on purpose).
4. **Dual grapples:** both hooks out at once, released oldest first (Boundless's, recommended). This changes movement (`movement::step` swings from two ropes), so it is the costliest part; it could come as part 2.
5. **Choosing items:** wheel for the right hand, Alt and wheel for the left, number keys 1 to 8 for the right and Alt and 1 to 8 for the left; switching only ever changes the hand chosen (players' wish). Recommended as written; or Boundless's radial (hold Q) as well.

## The plan once decided

- **Part 1:** two hand bars (saved with the character: a save format change, so `SAVE_VERSION` and an upgrade that moves today's hotbar into the hands), per-hand selection in `PlayerInput`, each button using its hand's item, one hand at a time on the server (`check_edit`, swings, shots and eating already share one cooldown), the HUD's two bars, the inventory window's link-to-hand clicks.
- **Part 2:** dual grapples in `movement::step` (two hooks in `PlayerState`, both deterministic), the oldest released first.

## Non-goals

Action sets (later), dual wielding beyond Boundless's rules, left-handed mouse settings (the operating system's swap already works).

## Status

- [x] Research and spec (2026-10-28)
- [ ] Nathan's answers to Decisions 1 to 5
