# What Boundless players liked, disliked and asked for

*Compiled 2026-10-18 from the official forum. Research for design decisions, not a spec: nothing here is decided until it is written into a milestone spec or ADR.*

## Sources and method

| Thread | Posts | Span |
|---|---|---|
| [What turns you off playing Boundless?](https://forum.playboundless.com/t/what-turns-you-off-playing-boundless/9953) | 3,068 | Oct 2017 to 2023 |
| [What turns you ON playing Boundless, makes you want to play?](https://forum.playboundless.com/t/what-turns-you-on-playing-boundless-makes-you-want-to-play/40601) | 15 | Jan 2020 |
| [What would turn you on to playing Boundless?](https://forum.playboundless.com/t/what-would-turn-you-on-to-playing-boundless/53146) | 70 | Nov 2020 to 2021 |

All 3,153 posts were downloaded in full and read, split by year. A theme is ranked by how many distinct people raised it, with forum likes as a second signal.

Posts are split into three eras:

| Era | Years | Posts | Weight |
|---|---|---|---|
| Early access | 2017 to Sep 2018 | ~880 | Low: much of it changed |
| Launch | Sep 2018 to 2019 | ~1,370 | Medium |
| Mature game | 2020 to 2023 | ~960 | Heaviest, as Nathan asked |

The thread is a venting thread by design, so dislikes dominate. The two "turns you on" threads supply the likes.

Raw data and per-year digests are not committed. They can be rebuilt from the forum's JSON API (`/t/<id>.json`, then `/t/<id>/posts.json?post_ids[]=…`; likes are `actions_summary` id 2).

---

## The short version

**People stayed for:**
- building and chiselling;
- exploring beautiful, varied worlds, and portals as windows to elsewhere;
- a gentle, helpful community;
- a player-run economy of shops;
- relaxed gathering;
- seeing a long project come together.

**People left because:**
- the grind was balanced for people who play 8 hours a day;
- upkeep chores held their builds hostage;
- the world felt empty;
- PvE was five recoloured creatures for years;
- forging felt like a slot machine;
- chat was broken;
- the developers went quiet.

---

## Dislikes, ranked (weighted toward 2020 onward)

### 1. Grind balanced only for the hardcore
Raised by about 35 people per year, every year. It was the top theme in four of the six reading batches.

- **The core complaint:** a few minutes of building costs hours of gathering. "If I want to build for a few hours, I have to spend at least 3 to 4 times that just to acquire the materials" (#2291, 2020).
- **Long material chains:** glue from bones, sackcloth from fibrous leaves, spark and coal for every craft, power coils, and chains of six or more steps for one item.
- **Crafting timers:** 30 to 50 minute timers for ordinary blocks like glass, and up to days for some machines.
- **Pacing walls:** mid-levels and mid-tier tools are a wall, and AoE (3×3) tools are mandatory, which is "900% more efficient" (#1406).
- **The top-liked 2020 post** (#2288, 23 likes) argues the game is two games:
  - one for players who can give it most of their waking hours;
  - one for casuals, who hit the wall, ask the forum, are told to "grind it out or buy it", and leave.
- **Hidden recipe changes:** some later recipe changes added steps on purpose (#1021), and players noticed.

**For Boundmore:** keep early crafting fast and cheap, and keep decorative and cosmetic blocks cheap. Make time-gates the exception, not the rule. Test the casual path (1 to 2 hours a day) as a first-class case.

### 2. Upkeep that holds your build hostage
About 22 people a year until reclaim shipped, and still about 7 a year after.

- **Beacon fuel:** missing a refuel used to mean losing the base and its contents. People came back from a break to nothing ("I lost 400 hours", #2862).
- **"Reclaim" (2020) fixed the worst of it:** items survive an expiry. It was called a "HUGE positive".
- **Portals stayed a treadmill:**
  - big portals ran for only about 4.5 days per fill;
  - hunting was the only source of portal fuel, so hunting became a chore;
  - "I'm sick and tired of fueling portals" (#3004, 2021);
  - "I cannot take a break because I am forced to play at least once every 10 days" (#1551).
- **The counter-argument:** expiry frees land from abandoned beacons, and some players enjoy salvaging ruins.

**For Boundmore:** no upkeep that can destroy work. If land needs to free up, do it by inactivity measured in months, with the contents kept for the owner. Portals need no constant refuelling.

### 3. An empty world
About 28 people in 2020 alone, and the top theme from 2021 on.

- **What people saw:**
  - ghost towns, empty malls and dead portal hubs;
  - cities "population: 1";
  - newcomers who never meet anyone.
- **What they blamed:** a small population spread over 50+ worlds, then rentable private worlds that split it further, and no marketing.
- **NPCs:** asked for repeatedly ("the game needs NPCs", #2709, 17 likes).

**For Boundmore:** match the number of worlds to the population, and add worlds as people arrive. Give people reasons to gather in shared places. Consider NPCs or ambient life so settlements don't feel dead.

### 4. Thin PvE: five creatures, no bosses, no dungeons
About 22 people in 2020 and 11 in 2021+. The top wish in the "what would bring you back" thread.

- **No variety:** five mobs, recoloured per world, unchanged for years.
- **Promised but never shipped:** Titans and the Hunter.
- **Missing entirely:** dungeons, melee weapons and armour.
- **Combat feel:** one-shot deaths and homing shots. "Hunting is, easily, the worst part of the game" (#2422).
- **Who it fails:** non-builders, who lose "70% of the content".

**For Boundmore:** a real creature roster with different behaviours per world type, plus bosses and group content. This is a whole milestone, not an afterthought.

### 5. Forging: a slot machine
About 25 people in 2019 and about 7 a year after.

- "A step below lootboxes" (#2130).
- "RNG is for drops. Crafting should be consistent and 100% repeatable" (#1198).
- A bundle of 5 or 9 items shares one roll and fails together.
- The rules were opaque, learnt from videos.
- What people wanted: "a puzzle, not a gamble" (#2921, 2021).

**For Boundmore:** if we have item upgrading, make it deterministic or skill-driven, with visible odds at most. No hidden dice that eat expensive inputs.

### 6. Chat and social tools
About 16 people in 2020 and 7 later.

- **Broken chat:** messages arrived delayed, out of order, or not at all. "The first and only MMO with a completely broken chat system" (#2665); players moved to Discord.
- **Missing features:** no persistent chat box, no global chat, no mail.
- **Do Not Disturb:** asked for repeatedly, because unwanted warps and visits drove social players offline (#2354, 16 likes).

**For Boundmore:** reliable, ordered, persistent chat with channels, mail and DND. Our single reliable ordered stream already gives the ordering.

### 7. Lag, rubber-banding and deaths caused by lag
About 25 people a year at launch and about 8 later.

- Rubber-banding into lava, then a death penalty on top: "The death penalty that punishes YOU for their server architecture" (#1089).
- Long portal loads and crowded hubs.
- Lag compensation that replayed actions client-side.

**For Boundmore:** this is the "runs better than the original" promise, and the netcode is already built for it: prediction, inputs tagged with their tick, 0 corrections in steady play. Keep it that way. Never punish a death the server caused.

### 8. Rare colours locked behind timed events
About 10 people in 2020, with high likes.

- **Timed worlds:** needed colours existed only on temporary "exo" worlds.
- **Being online mattered most:** the first 3 hours took everything, so it depended on time zone and luck. "2 million coin… by random chance" (#2898).
- **Builds blocked:** one builder couldn't start a large build because five colours didn't exist anywhere (#2649, 22 likes).
- **Not too easy either:** players want colours "accessible, but not just handed to you".

**For Boundmore:** every colour and material can be gathered somewhere, at any time. Rarity comes from distance and danger, not from being awake at the right hour. If temporary worlds exist, stagger them across time zones and let them regenerate.

### 9. Forced alts and narrow specialisation
About 25 people a year early on and about 4 later.

- **Too few points:** skill points ran out, so one character couldn't do everything.
- **Alts were the efficient choice,** with more plots and currency per character.
- **Specialising was boring:** after level 20, a new player found that specialising locks you into one activity, and no single activity is fun for more than a couple of hours (#3089, 19 likes).

**For Boundmore:** one character can eventually learn everything. Specialisation can be a choice of order, not a permanent lock. Plots and currency belong to the account.

### 10. Death penalty
About 15 people a year early on, fading later.

- **Early access:** at first death took 25% durability off everything carried.
- **Launch:** then an XP debt that scaled with level.
- **Effect:** both made people log off after dying. Respawns sometimes landed in sealed or lethal spots.

**For Boundmore:** keep death light, for example a corpse run. Respawn somewhere sensible. Never damage the whole inventory.

### 11. Weak onboarding and missing in-game information
About 15 people a year.

- **Tutorial:** it ended early.
- **Starter world:** cliffs and water, with no grapple to get around.
- **Information:** recipes were locked away invisibly, players couldn't tell what to do or where to buy, and they relied on third-party sites and spreadsheets.
- **Losing new players:** "80 to 90% of new players quit within 1 to 2 months" (#1555).

**For Boundmore:** an in-game recipe browser and resource info. A starter world that is easy to move around. A tutorial that reaches building, trading and travel.

### 12. Finding things to buy
About 25 people in 2019.

- **Finding shops:** shopping is fully physical, so it can mean "2 hours… through countless hubs and malls" with nothing found (#2003, 15 likes).
- **No price reference,** so new players were over- or underpaid.
- **The counter-view:** a global index kills the fun of hunting for a deal, and footfall to shops matters.

**For Boundmore:** a shop directory that tells you *where* an item is sold, and maybe a rough going rate, while the purchase still happens in person. This keeps travel meaningful without the 2-hour search.

### 13. Developer silence
About 20 people in 2020 and 25 in 2021+.

- No roadmap.
- Long gaps between updates.
- Promised features that never came (Titans, weapons).
- "Any news from the dev team at this point would be satisfying" (#3011).
- Frequent small updates were preferred to rare huge ones: "Release 752: 1 additional slightly different brick block" (#2, 13 likes).

**For Boundmore:** a public roadmap, and small, frequent releases.

### Smaller but recurring
- **Weather you can't turn off.** Snow covers block colours, which is bad for builders. It should be a visual setting.
- **Inventory pressure:** too many item kinds and not enough space. Also wanted: sorting, a trash action, and "craft all".
- **Land problems:**
  - plot squatting over resource hotspots;
  - settlements that swallow neighbours through merging;
  - wanted: an opt-out.
- **Farming:**
  - random growth;
  - tilled soil untills itself;
  - hand replanting;
  - low XP.
- **Missing building pieces:**
  - furniture, ladders and beds (the 2017 #11 request had 30 likes);
  - block rotation;
  - blueprints;
  - placeable water.
- **Cash shop worries:**
  - paid currency felt pay-to-win;
  - rental planets were randomised and costly;
  - backer rewards went undelivered.

---

## Likes (what kept people playing)

1. **Building and chiselling** (the strongest theme in every thread):
   - shaping blocks into slopes and trim;
   - "just complicated enough to be fun";
   - the satisfaction of a long build coming together.
2. **Exploring:**
   - beautiful, varied worlds;
   - portals as windows you can walk around and step through;
   - finding hidden player builds;
   - the first visit to a fresh world.
3. **The community:** helpful, generous players (starter packs, free grapples), and friendly encounters.
4. **Crafting that keeps early items useful:** glue and cloth stay valuable. "No wasted steps… everything retains its usefulness" (#3089).
5. **The player economy:** the first sale from your own shop stand is a hook (#11).
6. **Relaxing gathering:** "zoning out to mining", collecting, and sorting storage by colour.
7. **Reclaim**, once it existed.

## Wishes (what would bring people back)

From the 2020 to 2021 thread, in order of how many asked:

1. **Combat and PvE:**
   - bosses (Titans), dungeons and temples;
   - new creatures, with ecology per world;
   - melee weapons and armour.
2. **Living worlds:** NPCs, hireable shopkeepers, and an AFK "shop mode" for your character.
3. **Creatures:** pets, taming, mounts ("I just want a Wildstock to ride"), fish, birds and sea life.
4. **More building content:**
   - furniture, rugs, books and bookshelves;
   - blueprints, AoE chisels, AoE placement;
   - display stands.
5. **Automation:** wiring, hoppers, machines that draw from chests, and lifts.
6. **Frequent small updates** and visible communication.
7. **Quality of life:**
   - more saved locations;
   - working chat and DMs;
   - a minimap;
   - account-wide plots.
8. **Portals without constant refuelling.**
9. **Rarity and recognition:** rare finds, trophies, titles and leaderboards. "There isn't any real rarity… nothing to showcase the time spent" (#2644, 23 likes).

The most-liked post in the wishes thread (majorvex, #3, 42 likes) is a list covering all of the above.

---

## What this suggests for Boundmore (to decide with Nathan)

Ten candidate principles, one line each:

1. **The casual path is first-class.** A 1 to 2 hour session must end with something built.
2. **Nothing you made can be lost to upkeep.** No refuelling chores; land frees up only after months of inactivity, and the contents go back to the owner.
3. **Every resource can be gathered at any time.** Rarity comes from distance and danger, not from the clock.
4. **Upgrading is skill or choice, not dice.**
5. **One character can learn everything.** Plots and currency belong to the account.
6. **Death is light** and never caused by lag.
7. **Chat, mail and DND work**, plus in-game recipe, resource and shop-directory information.
8. **PvE gets real depth:** creature variety per world, bosses and group content. It deserves its own milestone.
9. **The world should never feel empty:** match the number of worlds to the population, plus NPCs or ambient life in settlements.
10. **Keep what worked:** chiselling, portals you can see through, player shops, a gentle community, and crafting where early materials stay useful.

## Already relevant to M3

- **Claims:** M3 claims have no fuel and no upkeep (principle 2). Before adding any land-expiry rule, decide it from this page.
- **Crafting timers:** M3 station timers are seconds, not hours (principle 1), and hand recipes are instant.
- **Tool wear:** M3 tools wear only when breaking the material they are made for. Death costs nothing yet (principle 6).
