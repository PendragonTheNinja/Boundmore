# Feel: numbers to calibrate against Boundless

Every value here was chosen as a sensible starting point, not measured from Boundless. Nathan (who plays it) checks each one in play and reports what is off; the fix is a one-line change.

## Movement (`bm-core/src/movement.rs`, `MoveConfig::default`)

| Value | Now | Check in Boundless |
|---|---|---|
| Walk speed | 4.6 blocks/s | Time a walk across a known distance |
| Sprint speed | 7.6 blocks/s | Same, sprinting |
| Jump height | about 1.45 blocks (velocity 8.7, gravity 26) | Can you jump onto a 1-block ledge? A 2-block one? |
| Gravity | 26 blocks/s² | Does falling feel floaty or heavy? |
| Ground acceleration | 55 blocks/s² (full speed in about 0.14 s) | Is starting and stopping instant or eased? |
| Air control | 14 blocks/s² | How much can you steer mid-jump? |
| Swim speed | 3.2 blocks/s | |
| Player size | 0.6 wide, collides 1.9 tall, drawn 2.0, eyes at 1.75 | **Set by Nathan (2026-10-18):** two blocks tall; nobody fits under less than two blocks. Height sliders in Boundless are visual only |
| Reach | 7 blocks (`ServerConfig::reach`) | How far away can you break a block? |

## Grapples (`assets/items.toml`; rope feel in `bm-core/src/movement.rs`)

Matched to Boundless's final numbers (Nathan, 2026-10-19: Boundless first; `docs/research/boundless-tuning.md` 1.1 to 1.4, from boundlessinfo.com's item data, checked against in-game tooltips). Ranges in blocks, speeds in blocks per second.

| Value | Now | Source, or guess |
|---|---|---|
| Copper grapple | range 10, shot 45, reel in 5, reel out 15, durability 500 | Boundless copper, exact |
| Iron grapple | range 20, shot 30, reel in 2, reel out 10, durability 1000 | Boundless iron ("long range"), exact |
| Wear | one per shot and one per release by firing again; putting it away is free | Boundless, medium |
| Hookshot | no base grapple; the ability waits for forging (a boon there, +20 reel in) | Boundless 1.0+ |
| Speed cap | 40 for everything; terminal fall 40 (was 55) | Boundless dev, high |
| Reel gathering | 20 blocks/s² up to the reel speed (`REEL_ACCEL`) | derived from one player's measurement, medium |
| Swing lean | 9 blocks/s² (`swing_accel`) | guess: Boundless's rope constants are known but not what they mean |
| Fling fade | 0.4 of the speed per second (`fling_drag`) | guess |
| Shortest rope | 1 block (`ROPE_MIN`) | guess |

**Nathan, 2026-10-20 (first play):** copper and iron grapples "feel really nice for their tier" and "the physics seem pretty sound". He is used to fully forged rift grapples (40 base range, up to about 120 with boons, mastery and guild buffs, tuning 1.3), which wait for exotic tiers and forging.

Not built yet, from Boundless: energy per shot (copper 60, iron 80, of a 600 bar), grapple mastery (+5 range and +5 shot speed per rank, 5 ranks), forge boons (+3 range, +5 shot, +5 reel per level), no fall damage while attached.

## Portals (`ServerConfig`, `assets/planets.toml`; M8, ADR-0009)

Boundless burned shards every hour whether a portal was used or not (1 to 9 an hour by size, 50 to 450 to open). Boundmore charges per trip instead (Nathan, 2026-10-19), so these are not comparable one for one: check how long a stocked hub lasts in practice.

| Value | Now | Check |
|---|---|---|
| Trip cost | 1 wayshard on the same planet, 2 between planets (`portal_trip_cost_same_planet`, `portal_trip_cost_other_planet`) | Does a busy hub drain too fast, or never? |
| Charge cooldown | 1 hour per player per portal (`portal_charge_cooldown`, seconds) | Back-and-forth trips should feel free |
| Tank | 500 wayshards (`portal_tank_capacity`) | How often does an owner restock? |
| Portal sizes | 1 to 6 wide, 2 to 6 tall | Boundless went up to 24 conduits for its longest links |
| Wayshard rarity | vein size 6, threshold 0.64, below y 68 and 24 under the surface: about 9 blocks per 32x32 column, heights 1 to 68 (median 30) under a surface near 147, about 30 times rarer than copper (measured 2026-10-20) | Nathan, 2026-10-20: could not find any in his first play. Too deep to reach without a light; proposal: raise to below y 100, a little richer |
| Conduit recipe | 4 stone, 1 copper ingot, 1 wayshard for 2 | A 1 by 2 portal costs one wayshard |

## World (`bm-server/src/config.rs`, `bm-worldgen`)

| Value | Now | Check |
|---|---|---|
| Planet width | 2 048 blocks around | How long to walk all the way round a small world? |
| Planet height | 384 blocks, sea level 128 | Deepest caves, tallest mountains |
| View radius | 10 chunks (320 blocks) | How far can you see terrain detail? |
| Edit rate | 12 per second, burst 24 | Fast building feel |

## Planets (`bm-server/src/config.rs`, `assets/planets.toml`)

| Value | Now | Check |
|---|---|---|
| Planet widths | Verdance and Wildmere 2 048 blocks, Emberdune and Rimehold 1 536 | Do small worlds feel small enough to know, big enough to explore? |
| Day length | Verdance 20 min; the others 15, 30 and 25 min | Boundless days varied by planet; do these feel right? |
| Shrine guard | 4 blocks around each warp shrine | Enough to keep it reachable without wasting the spawn |

## Not built yet, but to measure while playing

- Break time by hand for soil, stone, wood; with a basic tool.
- Day length.
- How dark caves are without light.
