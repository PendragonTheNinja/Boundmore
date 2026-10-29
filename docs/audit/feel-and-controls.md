# Audit: feel and controls

*2026-10-29, branch `ccr-b3c63c22-hfkvc9` (M28 built). Read-only audit: code read, nothing built or run. Boundless evidence is from the official forum (Discourse API) and the fandom wiki; forum topic ids are given as `t/<id>`.*

Scope: second-to-second play. Movement, grapples, mining and the hand stone, energy and hunger in the moment, combat and creatures, camera, controls, HUD and feedback, first-person hands, prediction, and a new player's first minutes.

## Summary

1. **The game is silent and the hands are empty.** There is no audio crate at all (`crates/bm-client/Cargo.toml`), and only a sling or a bag of bombs is drawn in hand (`app.rs` around line 3161). Nothing is drawn for a tool, block, grapple, torch or food, and nothing moves when you swing. Every swing, hit, break, pickup and landing is felt only through numbers and cracks. This is the biggest single gap in feel against Boundless.
2. **A new player is not taught anything.** The hand stone goes in slot 10 (`game.rs:2164`), but the selected hands start on slots 1 and 2, which are empty. So the first click says "You need a tool to break blocks. A hand stone needs nothing to craft" (`app.rs:97`) to someone who already has one. There is no tutorial, no objectives and no key list in the game. Boundless had a tutorial that players praised (t/10446, t/26637), and poor onboarding was still its number 11 complaint.
3. **There is no creep (sneak) mode**, so nothing stops a builder walking off an edge. In Boundless, C crept and you could not fall off ledges (wiki *Controls*; t/45812: "I spent 5 hours creep-building floors"). Our C opens crafting.
4. **There is no options screen.** Mouse sensitivity (`MOUSE_SENSITIVITY` 0.0022, `app.rs:112`), field of view (75 degrees, `app.rs:120`) and every key are constants. There is no rebinding, no invert, and no way to hide the HUD. Boundless let you rebind every key (t/22394, t/9669).
5. **There is no third-person camera.** Boundless had one (M toggled it; t/17400, t/16349), and players used it for combat, for spotting resources and for looking at their own character. We have no player model either: other players are tinted boxes (`app.rs` around line 3125).
6. **Combat gives you no idea where a hit came from.** You get the red edge flash and a number, but no direction. Boundless's new players "took a minute to figure out what was happening" when a spitter hit them (t/10446). Add a direction marker, a hit marker on a creature, and a spark where a shot lands (an M13 follow-up).
7. **The right hand is second-class in places.** A sling or bombs in the right hand is not drawn, and Twitchy always fires the left button (`app.rs` around line 2290). The "E" hints under the crosshair cover only shrines, chests, portals and hearts. Doors, signs, helixes and guild controls answer to E but show no hint (`app.rs` around line 2660).
8. **We already beat Boundless on several of its worst feel complaints**, and must keep those wins: there is no wall or impact damage (Nathan's own 2018 thread, t/18612), no fall damage while hooked or in the 3 s grace, a 2.8-block safe drop (t/39625, t/16029), neutral buoyancy, and 0 corrections in steady play. Two of its complaints are still open: falling armour is a flat amount against a share of max health (`food.rs:398`, `combat.rs:195`), and knockback still counts as a correction.

## Where Boundless did it better

| Area | Boundless | Boundmore now | Evidence |
|---|---|---|---|
| Sound | Every swing, break, pickup ("the chomp sound as you collect rocks", t/20855), hit, footstep, grapple and UI action had a sound. Players noticed when one dropped out (t/2850). | None. No audio dependency, no sound hooks (the roadmap's M2 "sound hooks" never landed). | `crates/bm-client/Cargo.toml`; `docs/roadmap.md:35` |
| Held items and swing animation | First-person hands holding the actual tool, with swing, chisel and grapple animations (t/30947 complains the chisel animation was *too* fast; t/6621 shows the third-person animations). | Only slings and bombs are drawn, and only for the left hand. Nothing animates when you swing at a block. | `app.rs` around lines 3159 to 3181, `held_sling` around line 3871 |
| Third-person camera | Toggle (M), with a reticle. Players asked for shoulder offsets (t/17400, t/2788). | First person only, and no player model. | `render_eye`, `app.rs:1014` |
| Creep | C: slower, cannot fall off edges. Builders used it for floors, sideways chiselling and planting (t/45812, t/11919). | Missing. X swims down, C opens crafting. | `sample_input`, `app.rs:1994` |
| Tutorial and objectives | A guided tutorial, then Feats and Objectives that steer the next steps. A tooltip on how to recover health when it ran low (t/10446 #3). | Feats exist (J), but there is no tutorial, objectives or contextual tips. A new player reads `README.md` or nothing. | `game.rs:2164`, no tutorial code anywhere |
| Rebindable keys and options | A full controls panel, multiple bindings, reset (t/22394, t/9669), and network settings (t/10446). | Hardcoded keys, no options screen. Esc only releases the mouse. | `window_event`, `app.rs` around lines 4040 to 4140; `settings.rs` holds only the name and server |
| Compass | A HUD compass with waypoints and markers (t/62811, t/9759 "setting a waypoint on the compass"; players asked for meteors on it, t/27578). | No compass. A one-off chat notice on login gives the shrine's direction (`shrine_hint`, `game.rs:3687`). The atlas (M27) is a menu, not a HUD. | |
| Sprint | Shift, or double-tap W (wiki *Controls*). | Shift or left Ctrl, held only. No double-tap and no toggle. | `app.rs:2002` |
| Labelled bars | Labels on the HP and stamina bars were added after new players were confused (t/10446). | Health shows "now / max"; energy has no label or number; hunger is a shaded end of the energy bar. Not obvious on first sight (a guess, untested with a new player). | `menus.rs` around lines 1650 to 1700 |
| Respawn choice | Return at the sanctum or at a home or beacon, as well as nearby. | Only "Return" nearby (8 to 24 blocks, `combat.rs:35`). | `return_player`, `bm-server/src/game/combat.rs:748` |

## Where we can beat Boundless

Already beaten. Keep these, and note them in `feel.md` so nobody undoes them:
- **No wall or impact damage.** Nathan's own thread, "most of my deaths in Boundless are due to smashing into a wall with my grapple" (t/18612, 2018), and t/21898 (taking damage from running into a wall with a Mega Fast brew). `land` only acts on vertical landings (`movement.rs`, `step_hands` near its end). Never add sideways impact damage.
- **A minimum fall distance.** `SAFE_IMPACT` 12 (about 2.8 blocks, `combat.rs:46`). Boundless's most repeated fall complaint was damage from 1 to 3 block steps (t/39625 with 16 likes, t/16029). A soft landing halves the damage.
- **Grapple safety:** no fall damage while hooked, and a 3 s grace after letting go (`FALL_GRACE_TICKS`). This answers t/23224 ("getting defeated by fall damage is killing grapple fun", a developer agreed with 24 likes).
- **Swimming:** neutral buoyancy and a real swim-down key. Slow, sinking swimming was a top new-player complaint (t/26526 #3, t/26479 "Swimming is h*ll").
- **Netcode:** prediction is exact (0 corrections in steady play), and hits are rewound up to 0.2 s. Boundless's "rubber-banding into lava" was complaint 7 in the forum feedback.
- **Bouncing without bounce damage** (`movement.rs`, BOUNCING).

Still to win:
- **Falling armour that means something.** Boundless players called it useless because it is flat against a percentage (t/23224 #7, t/39625 #7). Ours is the same shape: `fall_damage` is a share of max health, and `falling_armour` is a flat subtraction (`food.rs:398`). Making falling armour a percentage, or capping a fall at, say, 90% of health (t/37250's poll), would fix it. **Needs Nathan** (it departs from Boundless).
- **A creep that lets you build at full speed.** t/45812 asked for this. A builder's creep could slow you to walk speed or not at all. **Nathan decides** what it costs.
- **A sprint state you can see**, and an optional sprint toggle (t/27644). Shown as a small HUD tag or a slight FOV widening.
- **Auto-run** (t/15393: "the pads of my middle finger are bruised" from region hunting; t/11857).
- **A knockback that is predicted.** A grazer charge currently shows as a correction (`combat.rs:648`, "agreed for now"; an M13 follow-up). Sending the impulse at an agreed input, as skill changes are, would make it exact.
- **Telling the player why health went down.** Boundless players were confused about how to heal (t/10446). We could show "Eat until Full to heal" the first time health is low and you are not Full. The Full rule itself is Boundless's.
- **Protection for new players that they can see.** `PROTECTED_BELOW_LEVEL` 5 (`bm-core/src/creatures.rs:570`) exists but is never shown. A HUD line ("Creatures leave you alone until level 5 unless you attack") would teach it. Boundless's own new players died to spitters (t/26526 #1, t/10446).

## Bugs and rough edges found in our code

| # | Where | What is wrong | How it shows in play | Confidence |
|---|---|---|---|---|
| B1 | `bm-server/src/game.rs:2164` and `bm-client/src/app.rs:364` | The hand stone starts in slot 10, but the left hand starts on slot 1 and the right on slot 2, both empty. The no-tool notice then tells the player to craft a hand stone they already hold. | A new player's first click on a block says "You need a tool... A hand stone needs nothing to craft". Pressing 0 is never mentioned. | High |
| B2 | `app.rs` around line 3161 | The held-item draw reads only `s.selected` (the left hand). | A sling or bombs in the right hand are invisible, and so is the fuse ring's in-hand bag. | High |
| B3 | `app.rs` around line 2290 (Twitchy) | A twitch sets `mouse[0]`, so it uses the left hand's item even when the right hand holds the twitchy item. The grapple path inside `step_hands` picks the right hand correctly. | Twitchy on a right-hand sling or tool fires the wrong hand, or nothing. | Medium |
| B4 | `app.rs` around line 2660 (`aim_hint`) | The "E" hint covers the shrine, storage, portal and heart only. E also opens helixes, guild controls, signs and doors (`app.rs` around lines 4092 to 4120). | Doors, signs and guild blocks give no prompt; players will not find out that E works on them. | High |
| B5 | `bm-core/src/movement.rs`, `step_hands` (the "Flung" branch) | The fling test is `flat.length() > cfg.sprint_speed + s.body.sprint_bonus`. Sprinting faster than that through a Swift brew (`brew_run_bonus`) or the Sprinter quirk (sprint about 5.25 untrained) puts an ordinary sprint-jump into "fling" mode, where air control cannot slow you and speed fades by only 0.4 a second. Also `guild_run_bonus` is added to walking but not to sprinting. | With a Swift brew or Sprinter, letting go of W mid-jump does not slow you: jumps feel slippery. With Grapple and Run, sprinting gains nothing from the buff. Deterministic, so there are no corrections. | Medium (read from code; check the guild buff against Boundless's text) |
| B6 | `app.rs:2002` and around line 2075 | Shift is both sprint and "fill" for a chisel. | Sprinting along a wall with a chisel in hand and clicking fills instead of cuts. | High (the effect); low impact |
| B7 | `app.rs` around lines 4040 to 4140 | A key held down while a menu or chat opens is dropped from `keys` only on release, and a key pressed while a menu is open is never added. | Hold W, press I, close the menu: you stand still until you press W again. That is normal, but holding W through chat closing also stops you. | Medium; minor |
| B8 | `app.rs:4183` (MouseWheel) | The wheel changes hotbar slots while the mouse is not grabbed (no menu, cursor free). | Scrolling over the window after Esc changes your hands without you noticing. | High; minor |
| B9 | `app.rs` around line 4130 (Alt) | Alt picks the right hand. On Windows a bare Alt press and release can put the window into menu mode (the system menu takes the next key). winit 0.30.13 is used; whether it suppresses this was not checked. | Possible: a stalled frame or a lost key after pressing Alt alone. Ask Nathan whether Alt felt odd. | Low (needs a Windows check) |
| B10 | `bm-server/src/game.rs:2412` (`check_edit`) against the client's place prediction around `app.rs:2549` | The server refuses placements with "nothing to place against" and other quiet reasons, which the client does not check (it checks only free space, its own body and height). Aiming at a non-solid target, such as a plant or a torch, could predict a block that then vanishes. | A block appears and disappears with no message. | Low (depends on what `aim` returns for non-solid blocks) |
| B11 | `bm-server/src/game/combat.rs:648` | Knockback sets the player's velocity on the server only. | Each grazer charge counts as a prediction correction (title bar). The camera slides by latency times 12 blocks a second, smoothed over 0.15 s (`SMOOTHING_SECONDS`, `predict.rs:24`). A known M13 follow-up. | High |
| B12 | `menus.rs` around lines 1650 to 1760 | The notices area (bottom offset SLOT+52) overlaps the air bar (SLOT+44 to SLOT+60) and the food tags over the energy bar (up to about SLOT+54). | Under water or in bad air with a notice showing, the texts overlap by a few pixels. | Medium (worked out from the offsets, not seen on screen) |
| B13 | `fight_hud.rs`, `defeat` | A defeat says only "defeated by X" or "You were defeated". A fall, drowning, lava or choking defeat names nothing. | The player is not told why they died. | Medium |
| B14 | `bm-core/src/movement.rs`, swimming | Sprint does nothing in water (`speed = swim` whenever in water). Boundless players sprint-swam (t/27644), and Measure A4 asks about it. | Holding Shift in water changes nothing. | High (code); the rule is Measure A4 |
| B15 | `app.rs:2001` with the jump code in `step_hands` | Holding Space jumps again on every landing (`input.jump && s.on_ground`), and each jump costs 1 hunger (`JUMP_HUNGER`). | Holding Space to climb out of water, or by habit, bunny-hops and drains the bar. Probably Boundless's behaviour too; worth a word from Nathan. | Medium |

## Missing feedback

Things the game does, or the server does, with no visible or audible response:
- **Every action is silent:** swings, hits, breaks, item pickup, placing, landing, a hurt sound, a creature's wind-up (the popper's whistle is described as a sound in the README, but there is no audio), grapple fire, stick and reel, footsteps, water entry, UI clicks, and a level-up.
- **A block breaking:** no particles or debris, no pickup flash, and no "+1 Stone" line. The block simply goes. Boundless showed the item flying to you, with its sound.
- **Our swing:** no hand or tool motion, and no hit flash on the block, only the cracks and health bar under the crosshair (`breaking` in `render`, `app.rs` around line 3285).
- **Where a hit came from:** no direction marker (`fight_hud::flash` is all edges at once).
- **A shot landing:** no spark or puff where it hits terrain or a creature (an M13 follow-up). A creature hit shows a number and a body flash (`Session::hit`, `app.rs:934`), but no crosshair hit marker.
- **Energy running out:** a tired swing or shot is a quiet refusal (`try_spend`); only the bar's colour changes (`ENERGY_LOW` under 20%). There is no flash, shake or "too tired" line the first time.
- **Hunger:** the bar shading grows silently. There is no "You are hungry" until Starving.
- **Sprinting:** no indicator (t/27644 asked for one in Boundless).
- **Grapple:** there is no range indicator (whether the aimed block is within the hook's range) and no stuck or missed cue beyond the rope itself. A hook that runs out of range just disappears (`hook_tick` returns None). The rope is 4 one-pixel strands (`app.rs` around line 3230).
- **Fall damage:** shown only as the red flash and a number, which is right, but the grace after a grapple and soft landings are invisible rules.
- **New-player protection** (level 5) and Full healing are invisible rules.
- **Defeat cause** (B13).
- **Door, sign, helix and guild control prompts** (B4).
- **Edits refused quietly** ("out of reach", "editing too fast", "space is occupied"): the predicted block reverts with no message. For reach, an outline that greys out beyond reach would teach it.

## Ranked polish list

Ranked by value to a player for the effort. Effort: S is under a day, M is a few days, L is a week or more. Items marked (guess) rest on judgement, not measurement.

| # | Item | Why it matters | Effort | Nathan's decision? |
|---|---|---|---|---|
| 1 | Start a new character with the left hand on the hand stone (or put it in slot 1), and change the no-tool notice to name the key ("Press 0 for your hand stone") when one is carried (B1) | The first click of every new player fails with wrong advice | S | No |
| 2 | Hints for every E target: door, sign, helix, guild control (B4); a reach-greyed outline | Players find what they can use | S | No |
| 3 | Draw the right hand's held sling and bombs; fix Twitchy's hand (B2, B3) | M28 consistency | S | No |
| 4 | Defeat screen names the cause: fall, lava, drowning, choking, starving (B13) | You learn why you died | S | No |
| 5 | A direction marker for damage taken, a crosshair hit marker, a spark where shots land | Combat readability, the top Boundless new-player confusion (t/10446) | S to M | No |
| 6 | A first-minutes guide: a short checklist panel (break a trunk, craft timber, a workbench, stone tools; eat; travel at a shrine) plus contextual one-time tips (low health and not Full, first hunger, first tired swing, first night, level 5 protection ending) | Onboarding was Boundless's number 11 complaint; Boundless's tutorial was praised | M | Yes: how guided (a checklist or a tutorial island), and whether it pays XP |
| 7 | An options screen (Esc): mouse sensitivity, FOV, invert Y, hide HUD (F1), keys rebindable, saved in `client.toml` | Basic comfort, and the C, X and Alt choices become a player's own | M | No |
| 8 | Sound, first pass: a small `rodio` or `kira` layer with swing, hit, break, pickup, place, land, hurt, grapple fire and stick, UI click, and footsteps on two or three surface kinds; volume in options. Sounds made by us (procedural or CC0, never Boundless's) | The largest feel gap; every action reads twice as well | L | Yes: the sound direction and sources (the roadmap lists sound as his pick) |
| 9 | First-person held items and a swing animation: the item's icon or a box model in each hand, a swing arc on each swing, a recoil on a hit, a chisel tap, a grapple held | The second largest feel gap | M to L | Partly: the look (art direction) |
| 10 | Creep (for example C, with crafting moved, or a rebindable key): no walking off edges, at walk speed or slower | Builders' number one movement tool in Boundless | M (`movement::step`, deterministic, both sides) | Yes: the key, and its speed or cost (t/45812) |
| 11 | Predicted knockback at an agreed input (B11) | Removes the only correction in normal play | M | No |
| 12 | Break particles and an item-pickup flash or line ("+1 Copper ore") | Mining is the most frequent action | S to M | No |
| 13 | Fix the fling test and the guild run bonus on sprinting (B5); decide sprint swimming (B14) | Jumps with speed buffs feel wrong | S | Yes for B14 (Measure A4) and the guild buff's reach |
| 14 | A compass strip at the top with N, E, S, W, the shrine, your claims and meteors | Boundless had one; lost players are a known complaint (t/26526 #7) | M | No (a Boundless feature) |
| 15 | Sprint indicator, optional sprint toggle, double-tap W, auto-run | Comfort; t/27644, t/15393 | S | No |
| 16 | Labels or tooltips on the health, energy, hunger and air bars, and energy numbers on hover | New players read the HUD | S | No |
| 17 | A third-person camera (toggle) with a simple player model | Boundless had it; combat and looking around | L (needs a character model) | Yes: the character look (on his list: "character looks") |
| 18 | Falling armour as a percentage, or a fall cap below 100% (guess) | Boundless's flat armour felt useless | S | Yes (departs from Boundless) |
| 19 | Grapple range cue: the crosshair changes when the aimed point is within the hook's range | Fewer wasted shots and energy | S | No |
| 20 | Fix the HUD overlap (B12); stop the wheel acting while the mouse is free (B8); a key held through chat keeps moving (B7) | Tidiness | S | No |
| 21 | A respawn choice: nearby, or at the planet's shrine | Escapes a bad spot; Boundless offered a choice | S | Yes (Boundless used its sanctum and home) |
| 22 | Check Alt on Windows (B9); offer a second right-hand modifier if it misbehaves | M28's main key | S | Ask Nathan what he saw |
| 23 | Make rope strands thicker and lit (the M7 follow-up) | Grapples are a signature feel | S | No |

## Notes for whoever picks these up

- Items 1 to 5, 12, 15, 16, 19 and 20 need no decision and are all client-side: a good first batch, testable with `--demo-ui` screenshots.
- Anything in `movement::step` (creep, the fling fix, sprint swimming, knockback) is on the deterministic path: change the client and server together and keep the corrections count at 0 (invariant 2).
- Sound and held items deserve their own milestone spec (goal, acceptance, non-goals), with Nathan's direction on the look and the sound.
