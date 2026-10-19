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

## Grapples (`assets/items.toml`; swing feel in `MoveConfig`)

Seeded from a 2017 player measurement of Boundless's grapples (research §2.2), which the same thread later called out of date. Ranges are in blocks, speeds in blocks per second.

| Value | Now | Check in Boundless |
|---|---|---|
| Copper grapple | range 16, shot 55, reel 4 | Boundless copper: about 11 to 13 range, reel 2.9. Too generous for a first grapple? |
| Iron grapple | range 26, shot 65, reel 7 | Boundless's long ones reached 26 to 36 |
| Iron hookshot | range 20, shot 70, reel 16, reels itself | Boundless gold and diamond hookshots: 16 to 26 range, reel 28 to 34 |
| Swing lean | 9 blocks/s² (`swing_accel`) | How much can WASD pump a swing? |
| Fling fade | 0.4 of the speed per second (`fling_drag`) | How far does letting go at the bottom of a swing carry you? |
| Shortest rope | 1 block (`ROPE_MIN`) | How close does reeling in pull you to the hook? |
| Durability | copper 300, iron 700, hookshot 500 shots | |

## Portals (`ServerConfig`, `assets/planets.toml`; M8, ADR-0009)

Boundless burned shards every hour whether a portal was used or not (1 to 9 an hour by size, 50 to 450 to open). Boundmore charges per trip instead (Nathan, 2026-10-19), so these are not comparable one for one: check how long a stocked hub lasts in practice.

| Value | Now | Check |
|---|---|---|
| Trip cost | 1 wayshard on the same planet, 2 between planets | Does a busy hub drain too fast, or never? |
| Charge cooldown | 1 hour per player per portal | Back-and-forth trips should feel free |
| Tank | 500 wayshards | How often does an owner restock? |
| Portal sizes | 1 to 6 wide, 2 to 6 tall | Boundless went up to 24 conduits for its longest links |
| Wayshard rarity | vein size 6, threshold 0.64, below y 68 and 24 under the surface | How long to mine a stack of 100? |
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
