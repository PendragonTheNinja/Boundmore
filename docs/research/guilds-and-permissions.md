# Guilds, friends and claim permissions in Boundless

Research for milestone 25 (2026-10-28). Sources: the Empires release notes (Release 211, January 2019, forum t/24546) and the dev preview (t/24515); the wiki's Guild and Beacon pages (frozen around version 244); Release 182's notes (t/9211, the permission levels); player threads on permissions (t/5060, t/30916, t/50053, t/37644, t/13912). Facts only; names and text are Boundless's, not ours.

## Claim permissions (Release 182, then 211)

| Fact | Source | Confidence |
|---|---|---|
| Before Release 182 a beacon had its owner ("mayor") and "villagers", who could place and break any block, so they could also take what storage held by breaking it | t/5060 (dev post) | H |
| Release 182 split that into three permissions a beacon owner gives a player: **Builder** (place and break ordinary blocks), **Worker** (use what is there: storage, machines, crafting, doors, fuel), **Engineer** (place and break the interactive blocks: machines, coils, storage, plinths, locks) | t/9211; t/10255 ("Worker does not allow players to add or remove blocks ... but will allow them to interact"); t/30916 (repairing needs Worker; seeing a machine's health needs Engineer; an Engineer could take the machine) | H for the three names and their split, M for each edge |
| Only the owner adds or removes plots or removes the beacon, whatever permissions others hold | t/24546 | H |
| An owner gives permissions to Friends (a mutual friends list), and from Release 211 to guild members on aligned beacons | t/24546 | H |
| Locks: a lock next to a door or container keeps out anyone without permission; an advanced lock can be set to which permission holders get through | t/9449, t/30916 | M |
| Complaints that lasted: no "farmhand" right (plant and harvest without breaking blocks), no way to lock yourself out against accidents, Worker cannot see a machine's health | t/50053, t/37644, t/30916 | H (that players asked) |

## Guilds (Release 211, "Empires")

| Fact | Source | Confidence |
|---|---|---|
| Anyone may found a guild by crafting a guild control and placing it in a beacon. A name unique in the universe, at most 20 characters, and a tag of at most 5 letters (need not be unique) | t/24546, wiki Guild | H |
| Others ask to join at a guild control; a member with the membership right accepts. A character may be in up to 5 guilds; one is always their primary | t/24546 | H |
| Four ranks: **Director** (sets every rank and right), **Executive** (sets Operative and Controller ranks and rights), **Controller** (may be given rights over coin, buffs and membership), **Operative** (may be given Builder, Worker and Engineer rights on the guild's beacons) | t/24546 | H |
| Members donate coin to the guild; controllers with the coin right spend or pay it out | t/24546 | H |
| A guild log for controllers: permissions, membership requests, refusals, removals and additions, coin donations and payouts, beacon alignments | t/24546 | H |
| Guild chat channel; messages to friends and guild members from anywhere in the universe; a notifications channel | t/24546 | H |
| **Aligned beacons:** still the owner's; the owner may also give guild members rights there; the prestige counts to the guild; touching aligned beacons are always a named settlement, even beside a richer settlement | t/24546 | H |
| **Controlled beacons:** still owned (only the owner adds plots or removes it), but run by the guild: executives and directors name it, set its warping and its plot colour, give members rights. Meant for guild halls, shared storage, shops, portal networks | t/24546 | H |
| Footfall of aligned beacons could go to guild funds | wiki Guild | M |
| Factions: sub-groups a director makes inside a guild, with their own rights and buffs, inheriting the guild's | t/24546 | H |
| **Endeavour:** every 100 XP a member earns gives their primary guild 1 endeavour, at most 500 a character a week (the weekly reset) | t/24546 | H |
| **Guild buffs:** helix machines, crafted, turn coin and endeavour into a buff for every member whose primary guild it is (set before it was bought); up to 3 at once; each lasts a week; five power levels (minor, improved, greater, super, mega), each buff at three of them. The first four: less defeat penalty, chisel durability and placing speed, run speed and grapple range, lava resistance | t/24546 | H; costs not found yet |

## What this means for Boundmore

Boundmore today: a claim is its owner's alone; doors take a lock that keeps everyone but the owner out (M22 part 2); footfall is paid by everyone but the owner (M24 part 4) because nobody else can have rights; portals are the owner's to set (M8: "permissions come with guilds").

The pieces that fit what is already built: a friends list; the three permissions per claim, given to friends (and later guild members); locks that let permission holders through; then guilds with ranks, aligned and controlled claims, guild coin, chat and a log; then endeavour and buffs.

## Guild buffs and helix machines (for part 3; wiki item pages and its GuildBuff table, version 244)

Endeavour: every 100 XP a member earns gives their primary guild 1 endeavour, at most 500 a week each (t/24546). Buffs last a week, up to 3 at once, each from a helix machine (a workbench-made machine, 5 a craft, 110 wear; the higher tiers need a helix power coil).

**Cost to start a buff, by tier** (the GuildBuff table; the same for every buff whose rows are filled in): Minor (1) 5,500 coin and 400 endeavour, helix wear 2,000; Improved (2) 15,000 and 700, wear 4,000; Greater (3) 40,000 and 1,400, wear 4,000; Super (4) 80,000 and 2,300, wear 6,000; Mega (5) 160,000 and 3,600, wear 6,000. Each buff comes at three of the five tiers.

| Helix | Recipe (workbench) | Prestige rank, craft XP, skill | What it does (item text) | Known numbers |
|---|---|---|---|---|
| Builder's Buff | 20 relic fragments, 12 milk glands, 10 any stones, 16 refined iron; spark 1,000 | 23, 350, machine crafting 1 | chisel durability and block placing speed for members | Minor +15% / +15%, Improved +30% / +30%, Super +50% / +50% |
| Grapple and Run | 16 hopper eyes, 16 refined gold alloy, 10 any stones, 20 relic components; spark 1,600; power 2,000 | 24, 400, 2 | run speed and grapple length | Minor +0.2 m/s and +5 m, Greater +0.4 and +15, Mega +0.6 and +25 |
| Lava Protection | 16 refined iron, 20 relic fragments, 16 volatile blood, 10 any stones; spark 1,000 | 23, 350, 1 | less damage from lava | not recorded (Minor, Improved, Super) |
| Defeat Insurance | 3 refined titanium alloy, 10 any stones, 16 refined amethyst, 20 relic cores, 40 reactive lamellae; spark 2,000; power 3,000 | 26, 500, 3 | a smaller defeat penalty | not recorded (Improved, Super, Mega) |
| Spark and Wear | 10 any stones, 6 creature mantles, 20 relic fragments, 16 refined copper; spark 1,000 | 23, 350, 1 | less wear and spark at machines | not recorded |
| Gatherer Healing | 20 relic fragments, 10 any stones, 25 fibrous leaves, 16 refined copper; spark 1,000 | 23, 350, 1 | health back when gathering surface resources | not recorded |
| Home Healing | 20 relic components, 15 vital orbs, 10 any stones, 16 refined silver alloy; spark 1,600; power 2,000 | 24, 400, 2 | health back inside guild-aligned claims | not recorded |
| Hammer Versatility | 10 any stones, 16 refined silver alloy, 20 relic components, 16 opals; spark 1,600; power 2,000 | 24, 400, 2 | hammers deal more damage to soil blocks | not recorded |
| Weapon Augment Durability | 20 relic components, 10 any stones, 16 bitter beans, 16 refined gold alloy; spark 1,600; power 2,000 | 24, 400, 2 | weapon augments last longer | not recorded |
| Building Safety | 3 refined titanium alloy, 16 refined topaz, 10 any stones, 20 relic cores, 10 floatation sacs; spark 2,000; power 3,000 | 26, 500, 3 | less fall damage and longer placing reach | not recorded |
| Bonus Consumables | 2 refined titanium alloy, 16 refined emerald, 20 relic cores, 2 refined rift, 10 any stones; spark 2,000; power 3,000 | 25, 500, 3 | more bonus items when crafting food and brews | not recorded |
| Brew Extension | 20 relic cores, 6 spitter scale trophies, 3 refined titanium alloy, 16 refined ruby, 10 any stones; spark 2,000; power 3,000 | 26, 500, 3 | a bonus to active brews | not recorded |

Relic fragment, component and core are Boundmore's names for the three relic grades (`m18-data-check.md`). The creature parts (milk glands, hopper eyes, volatile blood, reactive lamellae, creature mantles, spitter scale trophies, floatation sacs, vital orbs) need mapping to Boundmore's drops before the recipes go in. The numbers not recorded are for the Measure list (`docs/measure-in-boundless.md`): Nathan can read them off a helix in the live game.
