# Milestone 28: Two Hands

*Spec drafted 2026-10-28; Nathan picked it the same day ("Two hands" over sound, character looks and rented worlds) and answered its decisions (below).*

Boundmore today has one hand: a 10-slot hotbar, one selected item, left click uses it (swing, shoot, fire a grapple, eat) and right click places a block or fills a chisel cut. Boundless had two hands, and players built their whole way of playing on them: a hammer in one hand and blocks in the other while bridging, a grapple in each hand to swing like Spider-Man, a sling and a bomb bag for a fight.

## What Boundless did (forum; the wiki has no page on it)

- **Each hand has its own bar of 8 slots**, drawn as a ring (filled from 12 o'clock clockwise). One slot in each hand is selected (topic 20299, 2018; 26429, 2019).
- **The left mouse button uses the left hand's item, the right button the right hand's**, whatever it is: a tool swings, a block places, a grapple fires, food is eaten (topics 7756, 26429).
- **Only one hand acts at a time**: two hammers swing left, right, left, right, never both at once ("we want to encourage more intelligent use of the system", a developer, topic 3195, 2015).
- **Dual grapples** (Release 152, 2017): a grapple in each hand, both hooks out at once; letting go releases the oldest first (topic 16292). Before hands existed players had eight hooks out; the hands limited it to two.
- **Choosing items:** the mouse wheel cycles the right hand, Alt and the wheel the left (topic 4741); a setting chose whether one press switched both hands ("Item Selection Radial Switching": both hands or only the hand used), and players disliked switching both by accident while hanging from a grapple (topics 25070, 23059, 37008). Shift-click puts an item in the left hand's next slot, Ctrl-click (or a right-click) in the right's (topics 13635, 26429).
- **A hand slot links to an inventory stack** rather than holding its own copy: the item stays in the inventory and the hand points at it (topic 20299, "link one of each to whichever hand slot"). A stack used up empties the link.
- What players asked for: action sets (swap all hand items at once, topic 48193); not having to give up inventory slots to fill hands (20299).

## Decided (Nathan, 2026-10-28)

1. **Hand bars: "Really just a normal 1-10 hotbar. I never really liked Boundless' radial dials. Unless there's a better way to do it."** Built as **one 1 to 0 hotbar with two markers**, Claude's proposal for that better way: the number keys and the wheel choose the left hand's slot (the left button, as before); Alt with a number or the wheel chooses the right hand's slot (the right button). No radial, no second bar; items stay in the hotbar as now (no save change). Cheap to change if it does not feel right.
2. **Each button uses its own hand's item, whatever it is** (Boundless's): a block in either hand places with that hand's button, a tool swings with its button, a grapple fires with its button. "Right click always places" goes. A chisel's button cuts; Shift and its button fills a cut back.
3. **One hand at a time** (Boundless's): the hands alternate on one swing timer; held both, the button pressed last acts.
4. **Dual grapples as part 2** (Boundless's: both hooks out, released oldest first). In part 1 a hook belongs to the hand that fired it, either hand's grapple fires with its button, and one hook is out at a time.
5. Choosing items: as 1 (not asked separately).

## Decisions for Nathan (as asked)

These shape how the game is played every second and how inventories work (`CLAUDE.md`, "Stop and ask first"):

1. **Hand bars.** (a) **Boundless's: two rings of 8 slots, each a link to an inventory stack** (the item stays in the inventory; recommended), or (b) the current 10-slot hotbar split in two (left 1 to 5, right 6 to 0), items held in those slots as now.
2. **Buttons.** **Left button uses the left hand, right button the right hand, whatever each holds** (recommended, Boundless's). Placing a block and filling a chisel cut then belong to whichever hand holds the block or chisel. Today's "right click places" goes away.
3. **One hand at a time** (Boundless's, recommended) or both hands may act together (faster, but Boundless's developers ruled it out on purpose).
4. **Dual grapples:** both hooks out at once, released oldest first (Boundless's, recommended). This changes movement (`movement::step` swings from two ropes), so it is the costliest part; it could come as part 2.
5. **Choosing items:** wheel for the right hand, Alt and wheel for the left, number keys 1 to 8 for the right and Alt and 1 to 8 for the left; switching only ever changes the hand chosen (players' wish). Recommended as written; or Boundless's radial (hold Q) as well.

## The plan once decided

- **Part 1:** the right hand's slot and button in `PlayerInput` (`off`, `fire_off`), a hook owned by the hand that fired it (`Hook::hand`), `movement::step` taking both hands' grapples, the server reading both hands (grapples, quirks, held light, how loud you are), each button using its hand's item on the client, the hotbar's two markers, Alt to choose the right hand. No save change (the hotbar is unchanged).
- **Part 2:** dual grapples in `movement::step` (two hooks in `PlayerState`, both deterministic), the oldest released first.

## Non-goals

Action sets (later), dual wielding beyond Boundless's rules, left-handed mouse settings (the operating system's swap already works).

## Status

- [x] Research and spec (2026-10-28)
- [x] Nathan's answers (2026-10-28)
- [x] Part 1 (2026-10-29): `PlayerInput::{off, fire_off}` (the right hand's slot and button), `Hook::hand`, `movement::step_hands` taking both hands' grapples (`step` is the left-only shorthand); a hook belongs to the hand that fired it and goes when that hand stops holding a grapple; one hook at a time. The server reads both hands for grapples and their wear, body quirks (`Quirks::of_hands`), held light and how far creatures notice you. Client: Alt with a number or the wheel picks the right hand's slot; each button uses its hand's item (a block places, a chisel cuts or with Shift fills, a grapple fires, anything else is used); held both, the last pressed acts; prediction steps both hands' grapples; the hotbar marks the left hand gold (L) and the right blue (R) (`docs/media/m28-hands.jpg`); a spanner or light in either hand counts. Protocol 56; no save change. Tests: core `each_hand_fires_its_own_grapple`; the end-to-end gnawing test now holds the hammer in the right hand.
- [x] Part 2, dual grapples (2026-10-29): `PlayerState::hooks`, one hook a hand, both out at once; each hand's button fires and lets go of its own hook (Boundless on PC; the "oldest first" rule in topic 16292 was the console's separate let-go button, so it has no use here); a hand that puts its grapple away lets go of its rope; R and F reel both ropes; two taut ropes are satisfied in turn, four passes a tick, so neither stretches (deterministic as before). The same hotbar slot in both hands is the left hand's only (one grapple never fires two hooks). Our ropes are drawn from the left and right of the view; other players' two ropes are sent (`RemotePlayer::hooks`), so a snapshot carries 16 players (was 23) and 36 creatures (was 40). Protocol 57. Tests: core `each_hand_fires_its_own_grapple` (both out, each let go alone) and `two_ropes_hold_the_body_between_them`; the end-to-end grapple test fires from the right hand beside the left. Load test (50 bots): about 5 ms a tick, as before.
- [ ] Nathan's play-test: does Alt for the right hand feel right; do two ropes swing well
