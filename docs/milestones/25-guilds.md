# Milestone 25: Friends, Permissions and Guilds

*Spec drafted 2026-10-28; Nathan answered Decisions 1 to 5 the same day (see Decided).* Nathan, 2026-10-28: "Merge and continue." M24 gave Boundmore coin, shops and settlements; the roadmap's next society systems are guilds and mail (`docs/roadmap.md`), and both shape what players invest in, so they need his decisions before they are built (`CLAUDE.md`, "Stop and ask first"). Research: `docs/research/guilds-and-permissions.md`.

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

## Decided (Nathan, 2026-10-28)

- **Friends only** get rights on a claim (Decision 2), and **part 1 comes first** (Decision 1).
- **Boundless's three rights plus Farmhand** (Decision 3).
- **Guilds as Boundless** (Decision 4): up to 5 a character with one primary, four ranks, guild coin, aligned and controlled claims, guild chat and log, a crafted guild block; no factions at first.
- **Guild buffs as Boundless, built last** (Decision 5).
- Not asked, built with the recommended defaults until he says otherwise: footfall of an aligned claim goes to its owner (Decision 6); messages are text only (Decision 7).

## Decisions for Nathan (as asked)

1. **Order.** Part 1 (friends and permissions) first, then guilds? Recommended: yes; guilds need permissions to mean anything.
2. **Who gets permissions on a claim.** Friends only, as Boundless (friend first, then give rights), or any player by name? Recommended: friends only.
3. **The permission set.** Boundless's three (Builder, Worker, Engineer) exactly, or those plus a Farmhand right (plant and harvest only)? Recommended: the three plus Farmhand.
4. **Guilds as Boundless.** Up to 5 guilds a character with one primary, four ranks, aligned and controlled claims, guild coin, a guild control block to found and join? Or simpler: one guild a character, three ranks (leader, officer, member)? Recommended: as Boundless, without factions at first (they add a second layer of rights few guilds need).
5. **Guild buffs.** Boundless's endeavour and helix machines (a guild works together for a week-long buff)? Or no guild buffs? Recommended: as Boundless, built last (part 3), with the four first buffs and costs researched or marked as guesses.
6. **Footfall and guilds.** A guild-aligned claim's footfall to its owner (as now) or to the guild's coin? Recommended: to the owner, with the owner able to choose the guild instead.
7. **Mail.** Text messages only, as Boundless? Or also sending items and coin by mail (Boundless never had it; players asked for "post office" drop boxes)? Recommended: text only with part 2; item mail later, if ever (it would undercut shops and footfall).

## Part 2 in detail (guilds and messages)

- **The guild control** (Boundless's recipe: workbench, 1 any stones, 3 any base metal bars, 2 relic fragments, 6 relic components; 5 minutes; 60 craft XP; prestige rank 5, build XP 8). Placed on your own claim; E opens the guild window. One with no guild yet founds one there: a name unique in the universe (at most 20 characters) and a tag of at most 5 letters. A control set to a guild is where others ask to join and where members manage the guild from.
- **Members:** up to 5 guilds a character, one primary (the first joined, until changed; leaving it picks another). Ranks Director (one, the founder; can hand it on), Executive, Controller, Operative. Each member holds claim rights for the guild's claims (Builder, Worker, Engineer, Farmhand) and Controllers may hold Coin, Membership and Buff rights. Executives set Operative and Controller ranks and rights; the Director sets all.
- **Joining:** ask at a control; a member with the Membership right accepts or declines; members leave freely; Executives and the Director remove members below them.
- **Coin:** any member donates from their coin; a Controller with the Coin right pays out to a member. Never below zero; every movement in the log.
- **Claims:** the owner aligns a claim to a guild they are in (guild members get their guild rights there; its prestige counts to the guild's total), or gives it to the guild's control (as aligned, and the Director and Executives also rename its settlement and give rights there). Only the owner adds or gives up plots or removes the claim, and unaligns it. Touching aligned or controlled claims of one guild form one settlement named for the guild, whatever their prestige, kept apart from neighbours of other guilds (Boundless's rule).
- **Chat and messages:** a guild channel for the primary guild (`/g text`); messages to a friend or guild member who is away are kept until they next play (text only, at most 50 waiting).
- **The log:** joins, leaves, removals, rank and right changes, donations and payouts, alignments; the last 200 entries, for Controllers and up.
- **Saved** in new tables (guilds, guild members by account, claim alignment, waiting messages); no save version change.

## Non-goals

Sovereign (rented) worlds; guild wars or PvP; factions (unless Nathan asks); item mail (unless Decision 7 says otherwise).

## Status

- [x] Research (`docs/research/guilds-and-permissions.md`, 2026-10-28)
- [x] Spec drafted (2026-10-28)
- [x] Nathan's answers to Decisions 1 to 5 (2026-10-28); 6 and 7 built with the defaults
- [x] Part 1, friends and claim rights (2026-10-28): `bm_core::rights` (Builder, Worker, Engineer, Farmhand; a builder may farm too); `bm-server/src/social.rs` (mutual friends asked and accepted, rights per claim, ending a friendship takes the rights each held on the other's claims; new tables `player_friends` and `claim_rights`, no save version change; at most 200 friends, 50 requests waiting, 50 friends with rights on one claim, all **guesses**); `game/rights.rs` (`Game::may`): placing and breaking need the block's right (working blocks such as storage, machines, coils, locks and conduits Engineer, crops Farmhand, the rest Builder), chiselling and signs Builder, storage, stations, the forge and locked doors Worker, mending with a spanner Worker, tilling Farmhand, bombs skip blocks the thrower has no right to, only the owner removes the claim or adds and gives up plots; building XP for builders too; friends with any right pay no footfall there. `ClientMsg::{Friend, SetRights}`, `ServerMsg::Friends`, `ClaimView::rights` (protocol 48). Client: the friends window (O; ask, accept, decline, remove, who is playing and where, and each friend's rights on the claim you stand in), `/friend name` and `/unfriend name`, storage and signs opened by rights. Tests: 1 core, 2 social, the store round trip, chat, 1 end to end (a stranger refused, friends asked and accepted, a friend without the right refused and told which, Builder given and used, only the owner removes the claim, unfriending takes the right).
- Part 1's known limits: portals stay the owner's (a guild's portal networks come with controlled claims in part 2); shops stay the owner's (a friend with Worker opens the stand's storage only through the owner); skill pages switch free only on your own claims.
- [ ] Part 2, guilds and messages
- [ ] Part 3, endeavour and guild buffs
