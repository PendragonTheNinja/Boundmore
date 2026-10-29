# Audit: crafting, economy and progression

*2026-10-29, branch `ccr-b3c63c22-hfkvc9` (M1 to M28). Read-only pass: docs, code and Boundless research (forum API, fandom wiki). No game code changed and nothing built or run, so every bug below comes from reading the code. Confidence is given per item.*

Scope: the crafting chain (M11), stations, furnaces, machines and spark (M15, M18 coils), the forge (M17), skills and XP (M12), feats and coin (M24), food, farming and brews (M16, M23), shop stands, request baskets, prestige, settlements, footfall (M24), guilds and helixes (M25), kept messages, and the inventory, storage, craft, station and shop windows.

## Summary

1. **Place and break farms everything.** Boundless marked a placed block as "dug up" (`GRAVEL_DEFAULT_BASE_DUGUP`, forum t/19747), so breaking it again gave no XP and no bonus drops. Boundmore has no such mark. A loop on one block of gravel, stone, trunk, soil or foliage pays out mining XP, building XP (on your own claim), bonus items (flint, sap, bark, tubers, dewberries, and on tier 4+ stone also rock salt, opal, olivine and cobalt), feat counts that pay coin, and guild endeavour. This is the top economy hole.
2. **Free accounts create coin.** Every new account starts with 500 coin (`feats::STARTING_COIN`), registration has no limit (`accounts::register`), and footfall pays out on any visitor account, with nothing taken from the visitor. Alts can sell junk to a main for coin, or walk claims for footfall. Boundless had the same problem with paid accounts. Ours costs nothing, so it is worse. Nathan has to decide the rules.
3. **Shop bait and switch.** `ShopAction::Buy { slot, count }` carries no expected price and no expected item. An owner can change the price, or swap a slot's stock through storage, while a buyer's click is on its way. The buyer then pays the new price, up to 1,000,000,000 coin each.
4. **M25 broke an M24 assumption.** M24 says only the owner can open a stand or basket as storage. Since M25, `storage_access` lets anyone with Worker rights (a friend or guildmate) open one with `OpenStorage`. A modified client can then take a stand's stock for free, or sell into a basket and take the items straight back out, over and over, until the basket's till is empty.
5. **Inventory capacity is about a seventh of Boundless's.** Boundless has 32 slots, and each one holds a smart stack of 9 x 100 related items (wiki "Smart Stack", "Citizen"): 28,800 items. Shift-click on a smart stack sorts it into a wall of storage, and touching storage blocks open as one window. Boundmore has 40 slots of 100 (4,000 items) and storage of 16 to 40 slots of 100. This is the biggest gap players will notice.
6. **The craft window does not scale to 582 recipes.** It is one flat list per station (workbench 160, refinery 123), with no search, no categories and no "can make" filter. Queued jobs cannot be cancelled or reordered, and Boundless had both. The "Max (N)" button is broken whenever N is over 100.
7. **Dead ends:** a broken spark generator hands back "Unused Spark", which nothing accepts (it is not fuel and no recipe uses it). Breaking and re-placing a machine resets its wear, so spanners are optional. A forge session left by its forger locks that forge forever, and nobody can stop it or break the forge.
8. **Where we are already ahead of Boundless:** crops on a clock that replant themselves (ADR-0010), reclaim, a deterministic forge seed with visible rules, a 10% fee instead of upkeep, kept messages, guild chat, and short station timers. Keep these.

## Where Boundless did it better

| Area | Boundless | Boundmore now | Evidence |
|---|---|---|---|
| Stack capacity | Smart stacks: 9 sub-slots of 100 per slot, related items mixed (hammers of any metal, glue + nails + sackcloth) | 100 a slot, one kind and colour a slot (`items.rs` `default_stack` 100, `INVENTORY_SLOTS` 40) | wiki Smart_Stack, Citizen |
| Storage | Touching storage blocks open as one combined window; shift-clicking a smart stack sorts it into matching cubbies | Each block is its own window; shift-click moves one stack | wiki Storage; `menus.rs` ~2267 |
| Placed blocks | Placed blocks become "dug up": no XP and no bonus drops when broken again | No mark: full XP, bonus drops and feats every time | t/19747; `game/mining.rs` 427 to 432 |
| Queue control | Cancel a batch, move a batch to the top of the queue | No cancel and no reorder; `ClientMsg` has neither | t/51225, t/20752; `bm-protocol` `ClientMsg` |
| Recipe discovery | Knowledge tab with every recipe, its uses and where it is made (players asked for search in craft windows: t/17267) | Recipes only inside the station that makes them; nowhere shows what a recipe is used for | t/22171 ("the Knowledge tab now basically does this") |
| Unused Spark | Goes back into a generator and moves between grids | Item exists and nothing accepts it (`items.toml` "spark" has no `fuel`; `station_fuel` refuses "That does not burn") | t/43592; `game/stations.rs` ~467 |
| Machine wear | Wear stays with the machine; repair takes spanners | Breaking forgets the record (`stations.rs:478`), so re-placing repairs it fully | M15 Status "Open" |
| Shop finding | Physical only; it was a complaint, not a strength | Same: no directory yet | forum feedback #12 |
| Paid entry | Buying the game was a small barrier to alts | Free registration, 500 coin each | `accounts.rs:87`, `feats.rs:20` |

Systems we have as Boundless had them (not repeated here): feats with Boundless's tiers, the 10% fee (Nathan raised it from 2% on 2026-10-28; CLAUDE.md still says 2% and should be updated), footfall curves, prestige ranks, helix costs, the forge session rules, food buffs and brew grades.

## Where we can beat Boundless

Players' top complaints in this area (`docs/research/boundless-forum-feedback.md`) and where Boundmore stands:

- **Grind and long chains (complaint 1).** Station times top out at 3,600 s (582 recipes, `recipes.toml`) and hand recipes are instant. This is already better than Boundless's 30 to 50 minute glass. Still to do: a 1 to 2 hour "casual session" test path (principle 1 in the feedback doc). Nobody has measured how long the casual path takes to reach iron, a forge or a shop stand. Write an end-to-end "path to first shop stand" checklist with its expected time.
- **Machine babysitting.** Boundless players wanted machines that draw from chests, plus hoppers and lifts (wishes 5). Two cheap steps first:
  - "Craft from nearby storage": a station may take inputs from storage blocks the crafter may Work within N blocks (needs Nathan; it changes how storage walls are built).
  - "Collect all into storage": send a station's output into the open or adjacent storage.
  Both beat Boundless without automation wiring.
- **Inventory pressure.** Smart stacks would match Boundless. To beat it, add:
  - a sort button for the pack and for storage (by kind, then colour);
  - "deposit matching": move every pack stack that already exists in this storage (t/38403 asked for exactly this);
  - a trash slot. `Discard` exists, so this is UI only.
- **Recipe discovery (complaint 11).** Add a Knowledge window (a key, say N): search any item, see its recipes, the station and skill each needs, what it is used in, where it drops (blocks' `drops`/`bonus`, creatures), and which planets carry it. All of this data is already client side (the catalogue and recipes arrive in the handshake). Ores and planets need care because of invariant 6: show only what the server already says about a planet (its kind and listed resources), never seams.
- **Forging as a slot machine (complaint 5).** We copied Boundless's rules exactly. The fix players asked for, "a puzzle, not a gamble", is a design change, so it belongs to Nathan. Low-cost help in the meantime: show the odds of each ingredient's outcome in the forge window (the session engine knows them) and keep the session log visible.
- **Finding things to buy (complaint 12).** A shop directory at a warp shrine: item, planet, settlement, price, stock, and no remote buying. The data is already in `planet_shops`. It keeps footfall meaningful (you still travel) and removes the 2-hour search. Needs Nathan (M24 decided "shops found physically only"; this keeps the purchase physical).
- **Farming (smaller complaints).** Already beaten: crops grow on the clock, tilled soil stays tilled, ripe crops replant themselves, real XP. Left: fertiliser has rules but no recipe (M16 Status).
- **Forced alts (complaint 9).** Plots and coin are per account (good). Watch skill pages: switching is free only on your own claims (M25 known limits).
- **Rarity and recognition (wishes 9).** Feats exist. Add titles from feat tiers, shown on the nameplate. This is cheap, and players asked for it with 23 likes (t/2644 in the feedback doc).

## Bugs, exploits and rough edges found in our code

Ordered by severity. "Conf" is my confidence from reading the code.

| # | Where | What is wrong | How it shows in play | Conf |
|---|---|---|---|---|
| 1 | `crates/bm-server/src/game/mining.rs:420-445` (`break_block` path), `game.rs:2285-2321` | No "placed by a player" mark. Breaking a placed block pays `break_xp`, `bonus_yield` rolls and `mined` feats; placing it on your own claim pays `build_xp` and `placed` feats. Edits may run at 12 a second (`EDIT_RATE`, `game.rs:112`) | A macro farms XP, coin (feat tiers up to 7,700 each), endeavour (500 a week cap per member), flint (5% a break on gravel), sap and bark (trunk), tubers, dewberries, and on tier 4+ stone rock salt, opal, olivine and cobalt, all from one block. **Item creation** | High |
| 2 | `crates/bm-server/src/game/shops.rs:172, 229-280`; `bm-protocol` `ShopAction::Buy` | Buy takes the price and the stack at the moment it arrives; the request names only `slot` and `count` | The owner raises a price (up to `MAX_PRICE` 1e9) or swaps a slot's stock through storage between the buyer's view and click. The buyer pays the new price, or gets a different item | High |
| 3 | `crates/bm-server/src/game.rs:2809` (`storage_access`) with `game/rights.rs:21` | Storage access asks for `Act::Work` only. Shop stands and baskets are storage blocks, and M25 gave Worker to friends and guildmates | With a modified client (`OpenStorage` on a stand), a Worker takes stock without paying. At a basket: `Sell`, then `StorageMove` the item back, repeat, and the owner's till drains. The vanilla client opens the customer view (`app.rs:1390-1403`), so only a modified client gets this far | High (rule), medium (that someone does it) |
| 4 | `crates/bm-core/src/feats.rs:20`, `crates/bm-server/src/accounts.rs:87`, `game/settle.rs:275-330` | 500 coin to every new account; unlimited free registration; footfall pays for any visitor account that is not a rights holder | Alt accounts mint coin: create, buy one dirt for 500 from your own stand (450 after the fee), delete. Each alt also pays 40 coin or more a day per settlement claim it walks into | High |
| 5 | `crates/bm-client/src/menus.rs:2525` vs `crates/bm-server/src/crafting.rs:51, 315` | "Max (N)" sends `count = N` with no cap; the server refuses anything over `MAX_CRAFT_COUNT` (100) with "That is too many at once" | With 101 or more crafts' worth of inputs (300 trunks into timber), Max is enabled and always fails. Cap the button at `min(max, 100)` and label it so | High |
| 6 | `crates/bm-server/src/game/stations.rs:442-479`; `items.toml` "spark" | A generator's refund is "Unused Spark" items, which nothing burns or uses. Spark that does not fit in the pack is lost. Banked spark of a machine's grid is lost on lapse (`crafting.rs:1498`) | Moving a generator destroys its spark. Players learn not to break generators | High |
| 7 | `crates/bm-server/src/game/stations.rs:478` (`forget`) | Breaking a machine that wears forgets its wear and coil wear | Break and re-place = full repair, so spanners are optional. Listed as open in M15. Keeping the wear on the dropped item needs a format change (Nathan) | High |
| 8 | `crates/bm-server/src/game/forge.rs:21-45` | A session belongs to its forger; there is no "give up", no timeout, and a forge with a session cannot be broken | A friend or guildmate who forges at your forge and stops playing locks it for good. Add an abandon action (gear and unused ingredients back to the forger's reclaim) and let the owner end a stale session after, say, 24 h (**guess**) | High |
| 9 | `crates/bm-server/src/crafting.rs:1466-1497` (`collect_xp`), `game/stations.rs` `collect` | Output and all banked XP go to whoever collects. Forged items in a forge's output go to anyone who may Work there (or anyone at all off-claim) | A forger at a guild or public forge can lose a sealed item (hours of work) to someone else's Collect. Station XP goes to the collector (M12 known). Suggest: output tagged with who queued it; others can collect only on the owner's claim | Medium |
| 10 | `crates/bm-server/src/crafting.rs:1499-1540` (`take`) | Refunds of untinted inputs come back natural colour; `Taken.crafts` keeps item ids, not colours | When a claim lapses with jobs queued, coloured inputs used in untinted recipes come back uncoloured. A rare colour can be lost | Medium |
| 11 | `crates/bm-server/src/game/shops.rs:282-340` (`sell`) | The seller's pack and the basket's till change before `ledgers.get_mut(...).ok_or("no ledger")?` | If the ledger is missing (cannot happen for an online player today), the items and till are gone and nobody is paid. Check first | Low |
| 12 | `crates/bm-core/src/guilds.rs` (`end`), `game/guild.rs:594` | A guild's donated coin disappears when its last member leaves | The last leaver loses the treasury. Pay it to the last member, or refuse leaving while it holds coin | Medium |
| 13 | `game/shops.rs:229` `buy` | Fee charged on `cost` rounded up per sale (`trade::fee` minimum 1) | Selling 1-coin items costs the owner 100% (till gets 0). Fine as a sink, but say so in the price window | Medium |
| 14 | `game/settle.rs:316-321` | Footfall uses `unix_now()` (wall clock) while the rest of the game uses ticks | A server clock jump pays everyone or no one. Low impact | Low |
| 15 | M24 Status | Tally by kind: breaking an untouched block of a kind also placed takes one off; released plots still count | Prestige drifts. Known | Known |
| 16 | `game/forge.rs` `forge_begin` | A tool forged once is refused again ("That has been forged already"), which is Boundless's rule. A failed or poor roll on a top-tier tool means crafting a new one | This is where "slot machine" comes from. Design, not a bug | n/a |

Not found: no item duplication through storage moves, shop buys, fuel moves or forge give-backs. Each is all-or-nothing on a cloned inventory (`storage_move_slots`, `buy`, `station_fuel`, `give_back` to output then reclaim), and the single-threaded loop rules out races between them. Breaking storage, machines or shops that hold anything is refused (`mining.rs:145-160`).

## UI and quality of life

Inventory and storage (`crates/bm-client/src/menus.rs` ~1973-2290, `storage.rs`):
- Dragging, half moves (right or Shift drag) and Shift-click across all work.
- Missing: sort, "deposit matching", "take all", a trash slot, a search box in big storage, and split by number (only halves). Touching storage blocks do not open as one window. 40-slot storage walls mean many windows.
- No smart stacks, and no stack over 100. See Summary 5. This is the biggest felt difference from Boundless.

Craft window (`menus.rs:2355-2530`):
- A flat scroll of every recipe for the station. Add a search field, the category tabs Boundless used (tools, blocks, decoration, machines, food...), and a "can make now" toggle.
- Buttons are 1, 5 and Max. Add a number field ("craft 37").
- Gated recipes are greyed in place. Sort them to the bottom.
- A recipe's inputs show "have/need", but you cannot click an input to see how it is made. This is where the Knowledge window links in.
- Hand recipes at a table are instant. Good.

Station window:
- Shows queue, progress, wear, spark and power. Missing: cancel a job (inputs back to the pack, or to the output if the pack is full), move a job up, repeat the last job, and "collect all" into nearby storage.
- Idle reasons are good (`idle_banner`).
- Queue limit is 8 jobs of up to 100 (`MAX_JOBS`, `MAX_CRAFT_COUNT`). Boundless's batches were bigger. Fine for now.

Forge window (`forge_screen.rs`):
- Shows predicted Effectiveness. Missing: give up a session, and odds per ingredient. Effectiveness is a guess (Measure list).

Shops (`shop.rs`, `menus::shop`, `menus::shop_owner`):
- The customer sees price and stock. Missing: a quantity field with total cost, a confirmation over a set amount, and the fee shown at the price the owner types ("you get 90").
- The basket seller sees no "sell all I carry of this".
- No history of sales for the owner (the logs have it). A "sales since you last looked" line would make the first-sale hook land (players named it as a hook).

Skills and feats (`skill_screen.rs`, `menus::feats`):
- Good. Two epics (Gathering, Efficient Crafting) still show "Comes later" though their systems exist (M12 follow-ups).

Messages: kept whispers (50, **guess**) work for friends and guildmates. There is no mail with items or coin attached. Boundless had none either, but players asked for it (complaint 6).

## Ranked polish list

Ranked by value to a player for the effort. Effort: S under a day, M a few days, L a week or more.

| Rank | Item | Why it matters | Effort | Nathan's decision? |
|---|---|---|---|---|
| 1 | "Dug up" mark: a placed natural block gives no XP, no bonus drops and no mined feats when broken (a block-state bit or a per-chunk set; saved, so format and version bump) | Closes the item, XP and coin farm (bug 1); Boundless's own rule | M | No (Boundless's rule). The save format change needs the upgrade check |
| 2 | `Buy` and `Sell` carry the expected price and item; refuse on mismatch ("The price changed") | Stops bait and switch (bug 2) | S | No (protocol bump) |
| 3 | Shop stands and baskets open as storage only for their owner (or an explicit new "Shopkeeper" right) | Closes bug 3 | S | Small: whether friends may stock a shop |
| 4 | Fix the Max button cap at 100 | Fails today for anyone with lots of materials (bug 5) | S | No |
| 5 | Cancel and move-up for queued station jobs | Boundless had it; misclicks are permanent now | S to M | No |
| 6 | Craft window search, categories, "can make" filter, number field | 160+ recipes a station | M | No |
| 7 | Unused Spark accepted back by any generator (1,000 spark each) | Moving generators loses spark (bug 6) | S | No (Boundless's rule) |
| 8 | Forge: give up a session; the owner can end one stale for 24 h (**guess**) | Locked forges (bug 8) | S | Small: the timeout |
| 9 | Inventory and storage sort, "deposit matching", trash slot | Daily inventory chores | M | No |
| 10 | Alt economics: starting coin after a first hour of play or level 5 (**guess**); footfall only from visitors of level 10+ (**guess**) or older than a week; one registration per IP per day | Coin minted by alts (bug 4) | S to M | **Yes**: what limits are fair |
| 11 | Smart stacks (9 x 100 a slot, Boundless's families) or a plain larger stack (e.g. 999 for blocks) | Capacity a seventh of Boundless's; builders feel it at once | L (smart stacks) or S (bigger stacks) | **Yes**: smart stacks vs bigger stacks; touches saves and every UI |
| 12 | Touching storage blocks open as one window | Boundless's storage walls; fewer windows | M | No (Boundless's rule) |
| 13 | Knowledge window: search any item, its recipes, uses, drops, planets (only server-public data) | Top onboarding complaint (11); replaces fan sites | M to L | Small: how much planet info to show (invariant 6) |
| 14 | Machine wear kept on the item when picked up | Spanners stay meaningful (bug 7) | M | **Yes** (open in M15; format change) |
| 15 | Output tagged with its queuer; forge output only to the forger off the owner's claim | Lost forged items (bug 9) | M | Small: the rule at guild machines |
| 16 | Shop window: quantity, total, fee preview, sales since last visit | The first-sale hook | S | No |
| 17 | Shop directory at shrines (where an item is sold, its price; buying still in person) | Complaint 12, the 2-hour search | M | **Yes** (M24 decided shops found physically only) |
| 18 | "Craft from storage within reach" and "collect into storage" | Machine babysitting, storage walks | M | **Yes**: changes how bases are laid out |
| 19 | Guild treasury goes to the last member (or leaving refused while it holds coin) | Lost coin (bug 12) | S | No |
| 20 | Refunds keep input colours (save colours in `Taken.crafts`) | Rare colours lost on lapse (bug 10) | S to M | No (save format) |
| 21 | Gathering and Efficient Crafting epics made real | Their systems exist; points spent on "Comes later" | S each | No |
| 22 | Fertiliser recipe (resins) | Farming loop's missing piece (M16 open) | S | Numbers are a guess |
| 23 | Titles from feat tiers on nameplates | Rarity and recognition wish | S | Small: names |
| 24 | Forge odds shown per ingredient | Fewer "slot machine" complaints without changing rules | M | No |
| 25 | Update CLAUDE.md: the fee is 10% (2026-10-28), not 2% | The docs disagree with the code | S | No |

Guesses in this list: rank 8's 24 h, rank 10's level and age gates, rank 11's 999 example. Everything else is from the code or from Boundless's rules as cited.
