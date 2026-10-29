# Boundmore

A persistent online voxel universe in the spirit of *Boundless*: a world of planets that wrap around, built, shared and traded on by everyone in it. All art, names and code are Boundmore's own.

![Arriving on four planets](docs/media/m4-real-arrivals.png)

**Status:** milestones 4 to 6 (a universe of four planets, warp shrines, iron; storage chests and chat; block colours, planet colours, claims lapsing to coloured ash) are built and awaiting a play-test. Milestones 1 to 3 (foundation; light and textures; accounts, inventory, crafting and claims) are built. See `docs/roadmap.md` for what comes next.

## Running it

You need [Rust](https://rustup.rs) (1.85 or newer).

```
cargo run --release -p bm-server
```

In a second terminal, from the same folder:

```
cargo run --release -p bm-client
```

The game opens on a login screen: pick a name and password and press **Create account** the first time, **Log in** after. The name and server are remembered (never the password).

Controls:
- Click to capture the mouse; Esc releases it or closes a menu.
- WASD to move, Space to jump (and to swim up; push against a shore with Space held to climb out), X to swim down, Shift to sprint. Let go of both in water and you stay where you are, or float at the surface; the air bar runs while your head is under, and when it is empty you drown. Brews (made in the mixer) are drunk like food: left click.
- **Two hands:** the hotbar has a left hand (gold, L) and a right hand (blue, R). 1 to 0 or the mouse wheel pick the left hand's slot; **Alt** with a number or the wheel picks the right hand's. Each mouse button uses its own hand's item, whatever it is: a tool swings, a block places, a grapple fires, food is eaten. One hand acts at a time.
- **Hold** a button with a tool in that hand to break a block; it goes into your inventory.
- **I** (or Tab) opens the inventory: drag to move, right drag or Shift drag to split, drop on Throw away to discard. Your coin shows at its top.
- **J** opens your feats: each pays coin and XP at five tiers as you build, mine, gather, craft, explore and hunt.
- **G** opens your guilds (coin, members, your primary guild, giving the claim you stand in to a guild); **E** on a guild control founds a guild, asks to join one, or runs it. `/g text` talks to your primary guild. **E** on a helix machine (on a claim given to a guild) starts a guild buff for a week.
- **O** opens your friends: ask someone by name (or type `/friend name` in chat), accept requests, and, standing in your own claim, tick what each friend may do there (Builder, Worker, Engineer, Farmhand).
- **C** opens crafting: things you make by hand, or, looking at a workbench or furnace, what it makes. Stations keep working while you are away; collect from them later.
- **P** toggles claim mode: left click a plot beside your claim to add it, right click to give it up. Place a claim stone to start a claim. A claim stays yours as long as you visit at least once every 6 months.
- **E** at a warp shrine (the glowing violet block at each planet's spawn) opens the travel menu: pick a planet to go there.
- **E** on a storage chest (workbench, 8 timber) opens it beside your inventory: drag between them, or Shift-click to send a whole stack across.
- Blocks have colours: what you break keeps its colour in your pack (stacks of different colours stay apart) and is placed in it. New planets colour their ground, rock and trees from a palette of 255 colours, in large patches worth exploring for.
- A claim whose owner has not visited for 6 months crumbles to ash in the colours of what was built; anything stored in it waits in the owner's **Reclaim** tab at any warp shrine.
- **Chisels** (workbench: square from stone, slope from copper, bevel, round and the precise ones from iron): its hand's button cuts the part of the block you aim at (outlined in yellow); Shift and the button fills it back (blue). Natural ground takes slope chisels only. You can walk up half-block cuts without jumping.
- **Portals** (workbench: portal conduits from stone, a copper ingot and a wayshard, the violet ore deep underground): build a filled upright rectangle of conduits, 1 to 6 wide and 2 to 6 tall, on your claim, and press **E** on it. *Make token*, carry the token to another portal of the same size and *Link* there. Add wayshards to each side's tank: walking through costs 1 (2 to another planet), at most once an hour per traveller. An empty tank sleeps until refuelled; nothing burns while idle.
- **Grapples** (workbench: copper grapple, short and quick; iron grapple, long and slow to reel): its hand's button fires the hook where you aim (a grapple in each hand puts two hooks out, each let go with its own button); it sticks in the first block it hits. **R** reels in (climbs), **F** lets rope out; WASD leans to pump a swing. Click again to let go (that costs a use, as a shot does): letting go at speed flings you. Putting the grapple away lets go for free.
- **Creatures** live on every planet: grazers (herds; they charge if you hurt them), pelters (they shoot at you on tier 2 worlds and up, once you are level 5) and dashers (rare; they run, and drop wayshards). Hit them with a tool, or craft a **sling** (stone sling at the crafting table; copper and iron slings at the workbench with Weaponsmithing) and click to shoot. Click a body to take your share of its drops: bone makes glue, tallow makes the brighter tallow torch. On Wildmere (tier 3) live poppers (they whistle, then explode: run, or stun them with a hammer) and drifters (they fly and shoot). **Bags of bombs** (workbench, from a bomb charge: saltpetre, flint and a popper's volatile blood): hold to light, let go to throw.
- **Health** sits beside energy. Long falls hurt (less on soil, sand and snow; never while your grapple is hooked, or just after letting go). Health returns slowly once you are out of a fight. If you are defeated, nothing is lost: press Return to come back nearby, with an XP debt (half XP until it is paid).
- **Enter** or **T** opens chat. `/l`, `/p` and `/u` switch between local (nearby), planet and universe; `/w name text` whispers and `/r text` replies. Enter sends, Esc closes.
- F11 for fullscreen.

**Creative mode (for testing).** Start the server with `cargo run --release -p bm-server -- --creative` (or put `creative = true` in `server.toml`). In game, **F8** (or `/creative` in chat) switches it on and off: you fly where you look (Space rises, X sinks, Shift flies fast), any block breaks with one swing of anything, and health, energy, hunger and air stay full. `/give item count` puts items in your pack (`/give iron_ingot 50`, `/give basalt 200`; names as in `assets/items.toml` and `assets/blocks.toml`, or as shown in game), `/xp amount` adds XP. On a server started without `--creative` all of this is refused.

**Far views.** The land is drawn out to 1024 blocks: past 320 blocks in 2-block cells, past 640 in 4-block cells, as Boundless drew its distant terrain. On the first visit to a planet the server builds the horizon in the background (a minute or two; the title bar's "far" count climbs) and saves it, so later visits are quick. `far_radius = 0` in `server.toml` (or `--far 0`) turns it off; `--far 16` halves it.

**Wandering worlds.** Every two days or so a temporary world arrives beside one of the planets and stays about five days; one arrives at once in a new universe. The chat says where; every warp shrine's travel window lists it with its host and how long it stays. Nothing can be claimed there, and everything left on it is lost when it goes (in its last half hour its air thickens; anyone still there is carried to the host). Some are tier 7 worlds, the only home of the three astral ores. To see more of them while testing, shorten their times in `server.toml`, for example `exo_every_hours = 0.5` and `exo_life_hours = [1.0, 1.0]` (worlds already in the sky keep their times); `exo_slots = 0` turns them off.

Getting started: break tree trunks by hand, craft timber, then a workbench; break stone for a furnace and stone tools; coal and copper from the rocks make copper tools. Iron is only found on the tier 2 and 3 planets (Emberdune, Rimehold, Wildmere), deeper than copper; iron tools are the fastest yet.

The server hosts thirteen planets by default: Verdance (temperate, tier 1, where everyone starts), Emberdune (arid), Rimehold (frozen) and Wildmere (lush), then nine tier 4 to 6 worlds whose air needs protection (milestone 18). `server.example.toml` shows how to change the list. An older save (`data/verdance.redb`) is upgraded into `data/universe.redb` on first start, keeping everything; the old file is left as a backup. If you log in far from the warp shrine, a notice tells you which way it is.

To play across machines, open UDP port 7777 on the server and give each player the server's `data/server.cert.der`:

```
cargo run --release -p bm-client -- --server 203.0.113.5:7777 --cert server.cert.der
```

Load testing: `cargo run --release -p bm-server --example bots -- --count 50 --planets 2` (bots spread over the first two planets).

## Updating

```
git pull
cargo run --release -p bm-server
cargo run --release -p bm-client
```

Stop the running server (Ctrl+C in its window) and close the game before rebuilding: Windows will not let a running program be replaced, and the build fails with `failed to remove file ... bm-server.exe: Access is denied`. If a copy is still running somewhere, `taskkill /IM bm-server.exe /F` and `taskkill /IM boundmore.exe /F` stop them.

## Layout

| Crate | What |
|---|---|
| `bm-core` | blocks, chunks, coordinates, planets, movement shared by server and client |
| `bm-worldgen` | planet generation (server only) |
| `bm-mesh` | chunk meshing |
| `bm-protocol`, `bm-net` | the wire protocol and QUIC transport |
| `bm-server` | the authoritative server |
| `bm-client` | the game (`boundmore`) |

Design decisions are in `docs/decisions/`; the rules every change follows are in `CLAUDE.md`.
