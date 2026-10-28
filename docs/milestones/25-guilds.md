# Milestone 25: Friends, Permissions and Guilds

*Spec drafted 2026-10-28, waiting on Nathan's decisions (below).* Nathan, 2026-10-28: "Merge and continue." M24 gave Boundmore coin, shops and settlements; the roadmap's next society systems are guilds and mail (`docs/roadmap.md`), and both shape what players invest in, so they need his decisions before they are built (`CLAUDE.md`, "Stop and ask first"). Research: `docs/research/guilds-and-permissions.md`.

Today a claim is its owner's alone. Nobody can help build it, use its storage or run its machines; a door lock keeps out everyone but the owner (M22); footfall is paid by everyone but the owner (M24 part 4); portals are the owner's to set (M8: "permissions come with guilds"). Boundless solved this in two steps, and this milestone follows them.

## What Boundless did

- **Friends:** a mutual list. A beacon's owner gives each friend permissions there.
- **Three permissions per beacon** (Release 182): **Builder** places and breaks ordinary blocks; **Worker** uses what is there (storage, machines, crafting, doors, fuel, repairs); **Engineer** places and breaks the working blocks (machines, coils, storage, plinths, locks). Only the owner adds or removes plots or removes the beacon. Locks keep out anyone without permission.
- **Guilds** (Release 211): founded with a crafted guild control placed in a beacon; a unique name of up to 20 characters and a tag of up to 5 letters; join by asking at a guild control, accepted by a member with the membership right; up to 5 guilds a character, one of them primary. Ranks: Director, Executive, Controller, Operative, each giving more rights (Operatives get Builder, Worker, Engineer on guild beacons; Controllers coin, buffs, membership). Guild coin from donations. A guild log. Guild chat and messages across the universe.
- **Aligned beacons** stay their owner's, let guild members in with the owner's chosen rights, count their prestige to the guild, and touching aligned beacons are always one named settlement. **Controlled beacons** are run by the guild (its leaders name them, set warping and plot colour, give rights) while still owned: guild halls, shared storage, shops, portal networks.
- **Guild buffs:** every 100 XP a member earns gives their primary guild 1 endeavour, at most 500 a week each; crafted helix machines turn coin and endeavour into week-long buffs for the members (up to 3 at once; defeat insurance, building, run and grapple, lava resistance).
- **What players kept asking for:** a farmhand right (plant and harvest without breaking blocks), locking yourself out against your own accidents, Workers seeing a machine's health.

## The plan (once decided)

- **Part 1, friends and claim permissions.** A friends list (asked and accepted; saved per account). On each claim, the owner gives a friend Builder, Worker and Engineer as Boundless did, plus a **Farmhand** right (plant, tend and harvest crops only), the request players made most. Every rule that now asks "is this the owner?" asks "may this player do this here?" instead: placing, breaking, chiselling, storage, stations and the forge, doors and locks, shops (stocking stays the owner's: coin goes to the owner), signs, portals (setting a link needs Engineer). Footfall is not paid by visitors with any right there. The claim window gets a Permissions tab. Saved in a new table (no save version change).
- **Part 2, guilds.** Founding, joining, ranks and rights as Boundless; guild coin with donations and payouts; aligned and controlled claims; guild chat; a guild log; settlements of aligned claims named for the guild.
- **Part 3, endeavour and guild buffs**, if Nathan wants them (Decision 5).
- **Messages** (Boundless's cross-universe messages to friends and guild members, kept until read) with part 2.

## Decisions for Nathan (as asked)

1. **Order.** Part 1 (friends and permissions) first, then guilds? Recommended: yes; guilds need permissions to mean anything.
2. **Who gets permissions on a claim.** Friends only, as Boundless (friend first, then give rights), or any player by name? Recommended: friends only.
3. **The permission set.** Boundless's three (Builder, Worker, Engineer) exactly, or those plus a Farmhand right (plant and harvest only)? Recommended: the three plus Farmhand.
4. **Guilds as Boundless.** Up to 5 guilds a character with one primary, four ranks, aligned and controlled claims, guild coin, a guild control block to found and join? Or simpler: one guild a character, three ranks (leader, officer, member)? Recommended: as Boundless, without factions at first (they add a second layer of rights few guilds need).
5. **Guild buffs.** Boundless's endeavour and helix machines (a guild works together for a week-long buff)? Or no guild buffs? Recommended: as Boundless, built last (part 3), with the four first buffs and costs researched or marked as guesses.
6. **Footfall and guilds.** A guild-aligned claim's footfall to its owner (as now) or to the guild's coin? Recommended: to the owner, with the owner able to choose the guild instead.
7. **Mail.** Text messages only, as Boundless? Or also sending items and coin by mail (Boundless never had it; players asked for "post office" drop boxes)? Recommended: text only with part 2; item mail later, if ever (it would undercut shops and footfall).

## Non-goals

Sovereign (rented) worlds; guild wars or PvP; factions (unless Nathan asks); item mail (unless Decision 7 says otherwise).

## Status

- [x] Research (`docs/research/guilds-and-permissions.md`, 2026-10-28)
- [x] Spec drafted (2026-10-28)
- [ ] Nathan's answers to Decisions 1 to 7
- [ ] Part 1, friends and claim permissions
- [ ] Part 2, guilds and messages
- [ ] Part 3, endeavour and guild buffs
