# Milestone 24: Coin and Trade

*Spec drafted 2026-10-27; Nathan answered Decisions 2 to 5 the same day (see Decided).* Nathan, 2026-10-27: "Merge to main and at least start on the next milestone." Every system left on the roadmap shapes the economy (`CLAUDE.md`: shops, settlements, guilds, mail, sovereign worlds), and this is the one the others stand on: Boundless was "built and traded on by its players", and the first sale from your own shop stand was one of the hooks players named (`research/boundless-forum-feedback.md`, #11). Boundmore has no money yet (marks come only from levelling, M12), so this milestone starts with coin itself.

## What Boundless did (wiki pages for Coin, Feat, Shop Stand, Request Basket, Plinth, Footfall; `research/boundless-mechanics.md` "Shops and trading")

- **Coin** was the one currency. It was earned **mostly from feats**, fixed rewards by tier (100, 280, 860, 2,560, 7,700 coin for tiers 1 to 5), and from **footfall** (settlements; a later milestone). Nothing NPC bought your goods: every other coin came from another player.
- **Coin went out** on warps (distance-priced, at least 100), portal opening fees, and the crafting and fuel players paid each other for. (Boundmore has no warp or portal opening fees: shrines are free and portals run on wayshards.)
- **A plinth** (refinery: 2 base metal, 10 stones, 2 glue; decoration crafting 1) is furniture, and the base of both stands.
- **A shop stand** (refinery: 2 storage blocks, a plinth, 2 cotton cloth; 7 min 30 s, spark 200; decoration crafting 2) sells: the owner fills it and sets a price an item; anyone who comes pays and takes, the owner need not be there; the coin waits in the stand.
- **A request basket** (the same recipe, decoration crafting 3) buys: the owner puts in one of the item to set what is wanted (colour counts) and a price, and charges it with coin; anyone who comes can fill it and is paid at once; the owner collects later.
- Prices could be set for similar items at once; a price-search API (Release 242) let players build price tools.
- **What players disliked:** finding anything could take "2 hours through countless hubs and malls" (#2003). They asked for a shop directory, while others feared a global index would kill hunting for deals and the footfall shops earned.

## Decided (Nathan, 2026-10-27)

- **Coin comes from feats, as Boundless, and from footfall, as Boundless did** (Decision 2). Footfall in Boundless paid only beacons inside a named settlement, at a base set by the settlement's rank (Outpost 20 coin at 10,000 prestige, Hamlet 30, Village 40, ...), once per visitor a day; so it needs prestige and settlements, and comes as part 4 after the shops.
- **The sink is a small sales fee** on stand and basket sales, paid by the seller and burnt (Decision 3). Built at 2% as recommended; **but research since found Boundless's own tax was 10%**, and the Merchant epic (Boundless's Tax Epic) cut it by a quarter (`research/coin-and-trade.md`). Nathan to choose 2% or Boundless's 10% (one constant).
- **Shops are found physically only**, as in Boundless: no directory (Decision 4). Part 3 below is dropped.
- **A new character starts with 500 coin** (Decision 5).
- Still open, built with the recommended default until he says otherwise (both cheap to change): the currency is called **coin** (Decision 1), and there is **no direct trade window**, stands only (Decision 6).

## Decisions for Nathan (as asked)

These shape what players invest in, so they are his (`CLAUDE.md`, "Stop and ask first"):

1. **The currency.** One coin, as Boundless? Its name in Boundmore (a plain word: "coin", "marks" is taken by the skill shop)?
2. **Where coin comes from.** Boundless's way is feats (one-off achievements paying 100 to 7,700 by tier). Options: (a) feats as Boundless, which means a feat list (Boundless had hundreds; we would write our own in Boundless's tiers); (b) feats plus a small coin reward for levelling up; (c) something else. Recommended: (a), with a first set of about 60 feats across mining, crafting, exploring, combat and building.
3. **Where coin goes.** Boundmore removed Boundless's main sinks (paid warps, beacon fuel). Without sinks coin inflates. Options: a small sales fee on shop stands (for example 2%, burnt), paid warps between planets (Boundless's cost, shrine to shrine), coin for extra plots (today marks buy them), or none for now. Recommended: a small sales fee only, and look again when settlements and guilds come.
4. **Finding shops.** Purely physical as in Boundless, or a directory? Recommended, from the forum research: a directory at warp shrines that says *which* shops sell or buy an item and where (planet and place), sorted by price, but the purchase still happens at the stand in person.
5. **Starting coin.** Does a new character start with any (for example 500, enough to buy a few starter goods)?
6. **Trading between two players directly** (a trade window), or only through stands as Boundless? Recommended: stands only for now (Boundless never had a trade window; dropping items works for gifts).

## Feats (part 1)

Boundless's feat list (`research/coin-and-trade.md`), each a counter with five thresholds paying 100, 280, 860, 2,560 and 7,700 coin, kept where Boundmore has the system and in Boundmore's own names: builder (blocks placed, wood, rock and soil placed, chiselled, plots), combat (creatures defeated, each kind, by bomb, by sling), crafter (by hand, each station, unique recipes), explorer (sprinted, swum, climbed, travelled, worlds and regions discovered), gatherer (trunks, foliage, plants, fungi, soil, boulders), miner (rock, coal, metal ore, relics, fossils, gems, unique items), core (level, feats done, meteor hearts, skill points spent), trader (stand and basket sales, coin spent, tax paid). Left out until their systems exist: objectives, beacon fuel, settlements and prestige (part 4), warps, creeping. XP per feat tier: not on the wiki (**guess**: the tier's coin as XP).

## The plan once decided

- **Part 1, coin and feats:** coin on the character (saved; a new table or a save version), feats with their rewards and a feats window, coin shown in the inventory.
- **Part 2, plinths, shop stands and request baskets:** Boundless's recipes; a stand's window for its owner (fill, price each item, take the coin) and for buyers (see, pay, take); a basket's window (set the wanted item and colour from one put in, price, charge with coin; sellers fill and are paid); stands on a claim belong to the claim's owner; every trade on the server in one save batch so coin and items can never be duplicated or lost.
- ~~Part 3, the directory~~: dropped (Decision 4).
- **Part 4, footfall:** prestige on claims, settlements of touching claims and their ranks, footfall coin collected at the claim. Specced in its own section before it is built.

## Non-goals

Guilds, mail, sovereign worlds, a premium currency (Boundless's cubits), NPC traders.

## Status

- [x] Research from the wiki and the existing research files (2026-10-27)
- [x] Nathan's answers to Decisions 2 to 5 (2026-10-27); 1 and 6 built with the defaults
- [x] Part 1, coin and feats, mostly built (2026-10-27): `bm_core::feats` (feats as data, the ledger, `mined_key`, `placed_key`), `assets/feats.toml` (51 feats), the server's ledger per account (`game/feats.rs`; a new `player_ledger` table, read with the character and never started afresh over an unreadable row), counted at their events: blocks placed by material, chiselled, plots, mined and gathered by kind and each new kind, kills and each creature kind, crafts by hand and at each station (counted when queued) and each new recipe, sprinted, swum, climbed and travelled, worlds and regions, meteor hearts, level; each tier pays its coin and XP with a notice ("Feat: Seasoned I, 100 coin"); `ServerMsg::Ledger` (protocol 43); coin in the inventory. Tests: 5 core, 1 client, 1 end to end, the store round trip.
- [ ] Part 1, left: a feats window (list, progress, next threshold); kills by bomb and by sling, skill points spent (counters exist, not yet counted).
- Upgrade check (2026-10-27): a universe made by `main` (protocol 42, two bots on two planets) opened with no warnings; its players logged back in and were given new ledgers (500 coin).
- [ ] Part 2, plinths, shop stands, request baskets
- [ ] Part 4, footfall (with prestige and settlements)
