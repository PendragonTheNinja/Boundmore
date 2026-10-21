# Milestone 12: Skills and Progression

*Spec written 2026-10-21. Nathan, 2026-10-20: "I fully accept it all as long as it is completely following how Boundless did it" (`docs/proposals/skills-and-progression.md`, Decided). Numbers from `docs/research/boundless-tuning.md` section 5 (levels, skills, XP), 2.7 (skills that change mining), 1.6 and 1.7 (movement and energy), and the community recipe data (boundlessinfo `compiled-recipes.json`: craft XP and skill gates per recipe, fetched 2026-10-21), unless marked **guess**.*

## Goal

Everything you do earns XP. Levels give skill points and the per-level currency, and skill points buy Boundless's skill tree: attributes, masteries, crafting levels and epics, on skill pages you can switch between. The skills that touch systems Boundmore already has (mining, movement, reach, energy, chisels, grapples, crafting, light, portals) work now. The others are on the tree but locked until their systems exist. Names are Boundmore's own; meanings and numbers are Boundless's.

## How Boundless did it (the live game, 1.0 to Release 248)

- **Levels**: the XP table in tuning 5.1, from 500 XP to reach level 2 up to 200,000 a level at 50 (3,304,900 in all), then 200,000 a level for ever.
- **Skill points**: 4 at level 2, then 2 a level, 100 by level 50; past 50, 4 a level (for extra pages).
- **The per-level currency** (Boundless's cubits; Boundmore's name: **marks**): 100 a level to 10, then 105, 110 ... up to 300 at 50, and 300 a level after (tuning 5.1 table).
- **Pages** (Boundless's skill sets): a character starts with one page. More are bought for **600 marks** each, up to 5 (Boundless had 3 at launch and 5 by 2020).
  - Each page holds at most 100 points, spent from one pool shared by all pages.
  - Switching pages has a 15-minute cooldown, or is free at home (inside your own claim).
- **Respec**: free full resets until level 20. One **cleanse point** per level up to 50 (each refunds one spent point, or the 5 of an epic). A full reset of a page costs **800 marks**.
- **Categories unlock by level**: attributes 1, mastery 2, crafting 4, epic 1 at 6, endurance 8, epic 2 at 10, exploration 12, epic 3 at 14.
- **XP sources**:
  - Mining a natural block: 2 for soil, sand, gravel, rock and trunks; 8 for the glowing crystal; ore by seam (copper, coal and relic 24; iron 40; wayshard **64, a guess**: Boundmore's own ore).
  - Gathering objects: 20 on tiers 1 and 2, up to 160 on high tiers (tuning 5.5; per object and tier **in the data where found, otherwise guessed**).
  - Placing a block inside your own claim: Boundless's build XP per block (2 for soil, up to 16 for machines).
  - Crafting: each recipe's craft XP from the data (copper bar 6, iron bar 8, glass 5, glue 5, stone tool 4, copper tool 10, iron tool 14, workbench 6, extractor 12, furnace parts 4, sackcloth 2; timber, sticks and wood tools 0).
- **Attributes** (8 ranks each, 1 point a rank; the 5-rank all-attributes bonus adds 2 to each per rank). An attribute's level is (value - 10) / 10, so 0 to 9.
  - **Power**: +5% damage a level.
  - **Dexterity**: +2 action speed a level.
  - **Agility**: run +0.3 and sprint +0.6 m/s a level, and crit chance + 0.2 x level / 9.
  - **Control**: reach +1 a level.
  - **Luck**: every bonus-drop chance multiplied by 1 + level / 9 x 1.8.
  - **Zeal**: max energy; per-level value not published, **guess +150** (a player saw 1,933 at full Zeal, base 600).
  - Vitality (health) and Intelligence (healing, forging) wait for their systems.
- **Masteries** (5 ranks):
  - hammer, shovel, axe: +5% damage and +2 action speed per rank with that tool;
  - grapple: +5 shot speed per rank.
  - Slingbow, bomb and spanner masteries wait.
- **Crafting skills** gate recipes. The data gives each gate:
  - **tool crafting 1** for copper and iron tools;
  - **tech crafting 1** for copper and iron grapples and chisels;
  - **machine crafting 1** for the iron furnace (later);
  - **decoration crafting 1** for the better torch (later);
  - wood, stone and the basic chain need none.
  
  Tool, tech and machine crafting have 2 or 3 direct ranks, and epics add more (tuning 5.4).
- **Epics** (5 points each; some either/or):
  - **Hammer** or **shovel and axe** (+25% damage with those tools; the pair is either/or).
  - **Damage** (epic 3: triples Power's share).
  - **Energy** (+4,000 max energy in the Testing 194 notes, **unconfirmed**) or Health (waits).
  - **Energy regeneration** (+900 a second) or health regeneration (waits).
  - **Durability drain** (about -30% tool wear, **low confidence**).
  - **Light source**: a personal light that switches off in bright places; radius not published, **guess 10 blocks**.
  - **Chisels**: precise cuts.
  - **Double jump** and **high jump** (jump 2 blocks higher).
  - **Portal**: needed to open portals.
  - Gathering, tax, forging, farming, revive, rage and focus and the rest wait for their systems.
- **Death**: an XP debt, when combat exists (not this milestone).

## Acceptance

1. **XP and levels.** Each XP source above adds XP to the account, saved with it. Levelling up shows a notice and adds points, marks and a cleanse point (to 50). The HUD shows level and an XP bar.
2. **The skill screen** (a key, K): the tree by category, each skill's name, ranks, cost, effect in plain words, and the level that unlocks its category. Systems not built yet show their skills greyed with "Comes later" and cannot be bought.
3. **Spending.** Ranks cost 1 point (epics 5) from the pool, at most 100 per page. Either/or epics refuse their partner. Category level gates hold.
4. **Pages.**
   - Buy a page for 600 marks (up to 5).
   - Switch pages: free inside your own claim, otherwise once per 15 minutes ("You can switch pages again in 12 minutes").
5. **Respec.**
   - Before level 20: a free full reset of the active page.
   - After 20: one cleanse point per spent rank, or a full reset of the page for 800 marks.
6. **Effects** (from the active page, the same on client and server, `bm-core` pure functions):
   - **Power, mastery and epics** change swing damage, using tuning 2.3's formula. Mastery and the hammer epic count only for the right tool type and never for the hand stone, as with Boundless's totem.
   - **Dexterity and mastery** add action speed.
   - **Agility** sets run and sprint speed, through the player's own movement settings. A change takes effect at an input both sides agree on, so prediction never corrects.
   - **Agility** also adds crit chance.
   - **Control** sets reach.
   - **Luck** multiplies bonus drops.
   - **Zeal and the energy epics** set max energy and the refill rate.
   - **Grapple mastery** adds shot speed.
   - **The light-source epic** lights around you like a held torch, off in daylight on the surface.
   - **The chisels epic** allows precise cuts with any chisel (the precise chisel items stay usable).
   - **The jump epics** change jumping.
   - **The portal epic** is needed to open a portal's window.
   - **Crafting levels** gate recipes. The gates are data on each recipe; the refusal message names the skill ("Needs Tool Crafting 1").
7. **Saves.** Level, XP, marks, pages, spent points and cleanse points are saved per account in the same batch as everything else. `SAVE_VERSION` 5 with an upgrade: existing characters start at level 1 with no XP. Upgrade check from `main`.
8. **Protocol 16**: the character view (level, XP, marks, pages and their ranks, cleanse points, switch cooldown) on joining and on every change. Messages to spend, cleanse, reset, buy a page and switch pages. Skills are data (`assets/skills.toml`) sent in the handshake.
9. **Tests**:
   - the level and point tables against tuning 5.1;
   - every effect formula against tuning 2.3 (max skills: copper 400 x 2.85 = 1140);
   - gates, either/or, the 100 cap, pages and the cooldown, respec and cleanse;
   - prediction without corrections after an agility change;
   - XP from each source, end to end;
   - the save upgrade.

## Decided (Nathan, 2026-10-21)

1. **Real money: no.** Marks come only from levelling ("Currently only by leveling yes").
2. **Plots for marks: yes**, as in Boundless.
   - 30 marks a plot, no discount for more, 1 to 80 at a time.
   - Bought plots add to the account's plot allowance (`ServerConfig::plot_allowance`, 64) and are saved with the character (`Character::plots_bought`, `SkillAction::BuyPlots`).
3. **Portal epic: required, as in Boundless** ("Yes for now, maybe we change later"). Opening a portal's window needs the Waywise epic (level 10). `ServerConfig::portal_epic_required` defaults to true, so it can be switched off later.

## Non-goals

Death XP debt (combat), Vitality and health, Intelligence, endurance and exploration skills, combat and forging epics, region discovery XP (no named regions yet), feats and coffers, character appearance, the shop's cosmetics.

## Status

*Built 2026-10-21 on branch `claude/amazing-pascal-lwttor`; awaiting merge and Nathan's play-test.*

- [x] Spec, contract and data (protocol 16); Nathan's answers (marks only by levelling, plots for 30 marks, the portal epic required)
- [x] Server (`crates/bm-server/src/game/skills.rs`, `store.rs`):
  - **Characters** are saved per account in the same batch as everything else; `SAVE_VERSION` 5 (version 3 and 4 universes upgrade, every character starting at level 1). Saved characters are cleaned on load: unknown skills dropped, ranks capped, overspent pages cleared.
  - **XP** comes from breaking (gathering objects times the tier's health multiplier), from placing inside your own claim, and from hand crafts at once. Station crafts bank their XP in the station for whoever collects.
  - **Updates**: character updates go out at most 4 a second, and a level-up sends a notice.
  - **Skill actions** run on the input timeline and set the body right after their input.
  - **Effects**: the shared `mining::skilled_tool` (damage, action speed, crit), reach (`cfg.reach` + Control), luck on bonus chances, wear saved with Careful Hands, crafting gates ("Needs Toolmaking 1"), Fine Hands precise cuts, Inner Light (off outdoors in daylight), the portal epic (`portal_epic_required`, default true), plot allowance + plots bought.
- [x] Client (`skills.rs`, `skill_screen.rs`):
  - Predicted character changes are checked by the same rules, the body is applied per input in the predictor, and the snapshot check includes the body.
  - The skill screen (K) shows level, XP, points, cleanses, marks, plot buying, pages, reset, the categories in Boundless's order and skill cards (either/or and "Comes later" greyed).
  - Swing prediction uses the shared `skilled_tool`. Reach adds Control, Fine Hands works, Inner Light glows, and gated recipes are greyed.
  - A hairline XP bar sits under the hotbar; `--demo-ui skills`; screenshot `docs/media/m12-skills.png`.
- [x] Integrated: 429 tests, fmt and clippy clean; upgrade check from `main` (characters start at level 1, players kept); a live run showed a new character's skill screen against a real server. The energy test now waits for a snapshot covering its inputs (it raced on a busy machine).
- [ ] Nathan's play-test. To judge: levelling pace against Boundless; the guessed numbers in `docs/feel.md` (Stamina +150 a rank, Deep Reserves +4000, the hammer epic's +12 action speed, Careful Hands 30%, Inner Light 10, the double jump's height).
- Follow-ups:
  - Inner Light is not switched off in daylight on the client (others see the server's choice).
  - Station XP goes to whoever collects.
