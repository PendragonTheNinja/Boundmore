# Milestone 10: Energy

*Spec written 2026-10-20. Nathan, 2026-10-20: energy first, then the crafting chain (with a stand-in for glue until creatures exist).*

## Goal

Using things tires you, as in Boundless: every swing, grapple shot and chisel cut costs energy from a bar of 600; it refills after a short pause; at zero you cannot use them until it does. Heavier tools cost more per swing, so damage per energy is the same across tiers, which is the trade-off Boundless built into its tools.

Numbers from `docs/research/boundless-tuning.md` 1.7, 2.1, 1.1 and 4.9 unless marked **guess**.

## How Boundless did it

- **Max energy 600** at base (Zeal raises it; skills come later).
- **Cost per use** ("burst stamina drain", from the game's item data):
  - tools cost damage / 50: hand stone 4, wood 6, stone 8, copper 8, iron 14;
  - grapple shots: copper 60, iron 80;
  - chisel cuts: 10 (basic chisels).
  
  Moving, jumping and sprinting cost nothing (since Testing 187).
- **Refill**: starts only after a pause of about **3.5 s** since the last use (players timed 3 to 4 s). Holding the use button at zero keeps resetting the pause (Release 199). Then it refills quickly: "the bar refills almost at once" with the regeneration skill's 900 a second. The base rate is not documented: **guess 300 a second** (empty to full in 2 s).
- **At zero**: the item does nothing (no swing, no shot, no cut) until energy returns.
- **Hunger** lowers max energy as you use items and move, and food restores it. It waits for food (non-goal).

## Acceptance

1. **The bar.** The HUD shows energy (600) under health's future place, draining with each use and refilling after the pause.
2. **Costs.** Item data gains `energy` (cost per use) for tools, grapples and chisels, set to the values above. Every accepted swing, grapple shot and chisel cut spends it. A use that costs more than what is left is refused, and the pause restarts ("too tired" shows as a brief flash of the bar, not a chat line).
3. **Refill.** `ENERGY_PAUSE` 3.5 s after the last use (or refused use), then `ENERGY_REGEN` 300 a second (**guess**, `docs/feel.md`) up to the max.
4. **Determinism.** Energy lives in `PlayerState` and is stepped in `movement::step` (fixed 60 Hz, integer or exact arithmetic), so client and server agree on every tick:
   - Grapple shots spend in `step` itself.
   - Swings and chisel cuts spend where the server applies them, right after their input `at`. The client applies its own pending swings and cuts at the same inputs when it replays.
   - The title bar's correction counter stays at zero in steady mining.
5. **Saves.** Energy is not saved (you arrive rested). `SAVE_VERSION` does not change. `PROTOCOL_VERSION` 14 (`PlayerState` carries energy).
6. **Tests.**
   - A copper hammer swings 75 times from full before it must rest (600 / 8).
   - An iron grapple fires 7 shots.
   - Refill waits 3.5 s, then fills at the set rate.
   - A swing beyond the budget is refused by the server even if a client sends it.
   - Prediction matches the server through a mining session (end to end).

## Non-goals

Hunger and food (they come with farming and cooking). Zeal and the energy skills (with skills). Energy costs for weapons (with combat). Sprint costing energy (Boundless removed it).

## Status

*Built 2026-10-20 on branch `claude/amazing-pascal-lwttor`; awaiting merge and Nathan's play-test.*

- [x] Spec (this file)
- [x] Core and data:
  - `bm-core/src/energy.rs`: `PlayerState::spent` and `rest` (integers), `try_spend`, refill in `movement::step`.
  - Grapple shots spend in the step.
  - Tool energy defaults to damage / 50; grapple and chisel energy are in `items.toml`.
- [x] Server: swings spend after the cadence and rules checks (too tired is a quiet refusal that restarts the pause); chisel cuts spend before anything else is ruled; energy goes with you through portals; not saved (`SavedBody` unchanged, `SAVE_VERSION` unchanged).
- [x] Client:
  - `Predictor::spend` applies a swing's or cut's cost at its own input and replays the later ones; the spends are kept per input, so a correction re-applies them.
  - The prediction check compares energy too.
  - The energy bar sits over the hotbar (screenshot `docs/media/m10-energy.png`).
- [x] Tests: the costs; 75 copper swings from full; 7 iron grapple shots; the pause and refill; prediction with late-known swings and no correction; end to end, a tired swing refused and the bar back to full after the pause. 371 tests in all. Upgrade check from `main` passes; protocol 14.
- [ ] Nathan's play-test: does running dry feel like Boundless? Is the refill rate (300 a second, a guess) right?
