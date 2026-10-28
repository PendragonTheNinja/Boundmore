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
