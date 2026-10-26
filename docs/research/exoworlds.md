# Exoworlds and tier 7 worlds: research (2026-10-26)

What Boundless's temporary worlds were, from the developers' release notes and from the player archive that logged every exoworld for five years. Facts only (numbers, rules); names and text stay Boundless's. Confidence: H high, M medium, L low. Lucent gear stats and recipes are already in `boundless-tuning.md` (grapples 1.x, tools 2.x, slingbows 3.4, recipes 4.6 to 4.10, creature multipliers 3.2.2); not repeated here.

## Sources

| Key | Source |
|---|---|
| [R218] | Testing 218 notes, james, https://forum.playboundless.com/t/28046 (2019-03-28) |
| [R219] | Release 219 notes, james, https://forum.playboundless.com/t/28550 (2019-04-10) |
| [ARCH] | The player archive: index https://forum.playboundless.com/t/28861 (2019 to 2020), then one topic per exoworld in forum category 31 ("Exoworlds"), 2019-12 to 2024-12. Listed with `search.json?q=category%3A31%20in%3Afirst&page=N`; 500 topics read (titles), 34 sampled in full (`/t/<id>.json`). Each gives type, tier, atmosphere, size, host world and distance, warp cost, appeared and last-until times. |
| [F] | Forum posts as cited |

## How they worked

| Fact | Value | Source | Conf. |
|---|---|---|---|
| What they are | Temporary worlds "orbiting around another world" for some days, then gone for good; spawned automatically by the universe | [R218][R219] | H |
| Lifetime | **113 to 128 hours** (34 sampled 2023-06 to 2024-11: min 112.8, median 121.7, max 127.7); about five days, a little random | [ARCH] | H |
| How often | A new one about **every 2 days**, steadily: 419 dated topics over 1,199 days (2021 to 2024); 281 of 418 gaps exactly 2 days, 58 one day, 32 three days | [ARCH] | H |
| How many at once | About 2 to 3 (five-day lives, one arriving every two days), spread over Boundless's server regions (Australia, US East, US West, EU Central) | [ARCH] (derived) | M |
| Tiers | **4 to 7 only** in 2023 to 2024: of 283 titled, T4 29 (10%), T5 87 (31%), T6 81 (29%), T7 86 (30%). The developers had said "they can be any level" (james, t/26463, 2019-03-04) and the 2019 archive has some lower ones | [ARCH] | H |
| Kinds per tier | T4: lush 18, coal 6, metal 5 (the tier 1 to 3 types raised to tier 4). T5 and T6: the six gem-world types and coal and metal, about evenly. T7: the three specials only, about evenly (rift 32, blink 28, umbris 26) | [ARCH] | H |
| Size | 3 km (34 regions; one 25), the same for every exoworld; "Exoworlds are 3072 m across" (dev, t/41263, 2020-02-06) | [ARCH][F] | H |
| Liquids | By type as on permanent worlds (water over water, water over lava on gem worlds) | [ARCH] | H |
| Reached from | One host world, by warp. The host's tier sets the distance: tier 4 exos from tier 1 or 2 hosts at 12 or 21 "blinksecs", tier 5 from tier 3 hosts at 30, tier 6 and 7 from tier 4 to 5 hosts at 42 | [ARCH], [R219] ("distances are based on the world level that they orbit") | H |
| Warp price | Coin by distance: 980 (12), 1,700 (21), 2,400 (30), 3,400 (42); about 80 coin a blinksec. Boundmore's shrines are free and it has no coin: not carried over | [ARCH] | H |
| Protection | No beacons (claims) or campfires: nothing protected, everything open to everyone | [R219] | H |
| Regeneration | None: what is built stays until the world leaves | [R219] | H |
| Leaving | "Atmosphere level slowly increases in the last 30 minutes ... level up your protection, eat some food or get off the world" [R219]. Players riding it out: within the last 20 minutes the atmosphere climbs until you die (noniethepup t/55588, 2021-03-28; Alwin t/48295, 2020-07-06) | [R219][F] | H for the rule, M for the shape |
| After leaving | Everything on it is lost ("Be careful what you put there") | [R219] | H |
| Atmosphere | The tier's own hazard and line (T4 1, T5 3, T6 5, T7 7; caustic, potent or volatile by type); the specials: rift potent, blink volatile, umbris caustic | [R218][ARCH] | H |
| Colours | Each exoworld has its own palette, often colours found nowhere else; collecting them was much of the draw ("unique colours to snag") | [R219][ARCH] | H |
| Resin and oil | Pockets of two rare liquids on exoworlds only | `boundless-worldgen.md` | H |
| Tier 7 specials | New ore (three lucent kinds), creatures made of it channelling two elements each (rift shock + chill, blink blast + burn, umbris corrosion + toxin; damage uses whichever element does most), +200% health, +100% damage and armour, triple drops, +75% XP; lucent meteorites drop meteorite ichor | [R218], tuning 3.2.2, 3.5, m18-data-check 17 | H |
| Lucent seams per world | Blink about 5 to 25, rift 20 to 60, umbris 120 to 160 (player counts) | tuning 4.3 | M |
| Tier 8 | In the data (atmosphere 10, block health x6, armour 1000) but never live | tuning 2.6, 4.13 | H |

## What players said

- They loved the colours, the fresh untouched ground, and the tier 7 hunts. Builds on exos were a known risk and accepted.
- They disliked missing a world because of time zones or a busy week (`boundless-forum-feedback.md`, "Timed worlds"; FOMO over colours). The principle there: rarity from distance and danger, not from being awake at the right hour. A steady stream (one every two days, five-day lives) softens this; making the exclusive things (lucent ore) available on every tier 7 visit rather than once softens it more.
- Nathan himself, 2020-09-19 (t/50855): "Player-made exoworlds that last longer. Such a fun idea."

## Open / not found

- The exact shape of the leaving atmosphere (how many levels a minute, its ceiling). Read as: the hazard climbs from the world's own to 10 over the last 30 minutes, past any protection (Boundless's maximum is 10 at tier 7 with the best food); a **guess**.
- How the host was picked among worlds of the right tier (looks random; several hosts recur).
- Lifetime of the 2019 exos (the wiki says "a few dozen real-life hours"; the later archive's five days is what the live game settled on).
