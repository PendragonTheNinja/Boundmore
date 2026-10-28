# Milestone 8: Portals

*Spec written 2026-10-19. Built 2026-10-20; awaiting Nathan's play-test.*

## Goal

Players build their own permanent portals between places they care about, on one planet or between planets, as in Boundless, without Boundless's refuelling treadmill (`docs/research/boundless-forum-feedback.md`, principle 8). Warp shrines (M4) stay as the free public network; portals are what players build and keep.

## Decisions (Nathan)

- 2026-10-19 (roadmap): portals need fuel, but less punishing than Boundless. A portal never disappears (dormant when empty, reopens when refuelled), uses no fuel while idle, and charges each player at most once per portal per cooldown, so back-and-forth trips cannot drain it.
- 2026-10-19: the fuel is a **new mined crystal** (deep underground on every planet; hunting can add a second source later). The **portal's tank pays** for each trip: whoever stocks it is providing a service, as with Boundless hubs. Portals are **linked with a token**, as in Boundless. **See-through windows come later**, as their own milestone; this one shows a shimmering surface with the destination's name.

Design rationale and the alternatives weighed: ADR-0009.

## Acceptance

1. **Wayshards.** A new ore, `wayshard_vein`, lies deep in rock on every planet kind (rare, below about 60 blocks under sea level). Breaking it (a hammer helps) gives `wayshard`. Existing planets gain it in the parts nobody has edited yet (see *Existing planets*).
2. **Building.** `portal_conduit` (workbench: 4 stone, 1 copper ingot, 1 wayshard, makes 2) can only be placed on your own claim ("Portal conduits go only on your own claim"). A portal is a flat, upright, filled rectangle of conduits, 1 to 6 wide and 2 to 6 tall, standing in an X-Y or Z-Y plane. Its frame is the conduits themselves: in Boundless you build the surface and it opens in place.
3. **The portal window.** E on any conduit of a complete rectangle (in reach, on your claim) opens the portal window: its size, its link, its tank, and buttons. A rectangle that is not complete, or is a cross or an L, says why ("A portal is a filled rectangle of conduits, 1 to 6 wide and 2 to 6 tall").
4. **Tokens.** *Make token* gives a `portal_token` bound to this portal (it shows "Portal token: 2 by 3, Verdance, near 812, 140"). Tokens stack to 1, can be stored and traded like any item. At another portal of the **same width and height**, *Link with token* (choose the token in your pack) consumes it and links the two, both ways. Refused, with nothing consumed: a token for this same portal, a token whose portal is gone, or a different size. Linking a portal that was linked elsewhere replaces its old link (the old partner becomes unlinked).
5. **Fuel tank.** The window has a tank: *Add fuel* moves wayshards from your pack in (up to 500), *Take fuel* moves them back. Only the claim owner can take fuel out; anyone who can open the window (the owner, for now) can add it. A portal with fuel in its tank cannot be broken ("Empty the portal's tank before breaking it"), so nothing is lost.
6. **Open and dormant.** A linked portal whose tank holds at least one trip's cost is **open**: its conduits show as `portal_surface` (not solid, glowing, translucent) on both sides as each side's own state allows. A side with an empty tank is **dormant**: it keeps its link and shows plain conduits until refuelled, then reopens by itself. Nothing burns while a portal stands idle.
7. **Travelling.** Walking into an open surface takes you to the other portal, standing on the far side's ground just outside its surface, facing away from it. Same planet: you move without the travel screen. Another planet: the M4 journey screen, as with shrines. Arriving never drops you back into the portal you came out of (you must step out of the surface first).
8. **Charging.** A trip costs `portal_trip_cost` wayshards (1 on the same planet, 2 between planets) from the tank of the portal you walk into. Each player is charged at most once per portal per `portal_charge_cooldown` (1 hour, a tunable in `ServerConfig`); trips within the cooldown are free. If the tank cannot pay, the portal goes dormant before you step through, and you are told ("This portal is out of wayshards").
9. **Unlink and break.** *Unlink* closes both sides and forgets the link. Breaking any conduit of an unlinked or linked portal with an empty tank removes the portal (its partner becomes unlinked). Surfaces cannot be broken directly; unlink first.
10. **Claims and lapse.** Portals are blocks on claims and follow claim rules: only the claim owner opens the window, places or breaks conduits. When a claim lapses (M6), its portals unlink, the conduits turn to ash like any block, and any fuel in the tank goes to the owner's reclaim.
11. **Saved.** Portals (shape, place, link, tank, which side opened) are saved with the planet in the same write batch as everything else. Tokens save by item name with their portal number (`portal_token@17`), as colours do (`stone#17`), so `SAVE_VERSION` does not change.

## Existing planets

A planet keeps the generator settings it was made with (`PlanetGenConfig`). On opening a planet whose kind's ore list now extends its saved list (the saved ores first, then more), the server appends the new ores to the saved settings and saves them, logging "Verdance: wayshard_vein added to its ores". Ores are tried in order and the first match wins, so earlier ores lie exactly where they did; only stone that no earlier ore claimed can become wayshard, and only in chunks never saved (every edited chunk is saved whole). `GENERATOR_VERSION` does not change: the generator code is the same, and the default settings still give the pinned terrain.

## Shared contract (built first, by the main session)

- **Data:** blocks `wayshard_vein`, `portal_conduit`, `portal_surface` (end of `blocks.toml`); items `wayshard`, `portal_token`; recipe for conduits; `wayshard_vein` appended to every kind's ores in `planets.toml` (temperate lists its ores explicitly for this).
- **bm-core:** `ItemStack::tag` (what a single stack is bound to; 0 for nothing; stacks merge only with the same tag); `ItemKind::PortalToken`; `portal.rs`, the pure rules: finding the rectangle a conduit belongs to, sizes, the exit spot.
- **bm-protocol 12:** `ClientMsg::OpenPortal { pos: Option<BlockPos> }`, `ClientMsg::Portal { pos, action: PortalAction }` (`MakeToken`, `Link { slot }`, `Unlink`, `AddFuel { slot, count }`, `TakeFuel { count }`), `ServerMsg::Portal(PortalView)`, `ServerMsg::Teleport { spawn, tick }` (same planet moves).

## Plan and who does what

| Part | Owner |
|---|---|
| Spec, ADR, contract, data | main session |
| Server: portal records and saves, window rules, linking, tank, open and dormant surfaces, travel and charging, claim lapse, ore extension of existing planets | agent (bm-server, worktree) |
| Client: portal window, E on conduits, token names in tooltips, `Teleport`, `--demo-ui portal` | agent (bm-client, worktree) |
| Art: wayshard vein, conduit, surface textures, wayshard and token icons | agent (bm-texgen, worktree) |
| Integration, headless play-test, docs | main session |

## Non-goals

See-through portal windows (a later milestone), portals that load the far side's chunks, warp conduits (one-shot warps), a world graph with distances, portal skills, hunting as a fuel source, portals usable by people other than the claim owner's choosing (permissions come with guilds), more than one hub portal per token.

## Status

- [x] Spec, ADR-0009, contract, data (wayshard ore on every kind, re-pinned with the old pins kept for older planets)
- [x] Server: portal records in their own table, the window and its actions, links, tanks, open and dormant surfaces, travel (same planet by `Teleport`, other planets by the journey, far chunks loaded without the loop waiting), charging once per player per portal per hour, breaking rules, claim lapse (fuel to reclaim), tokens saved as `portal_token@id`, wayshards appended to existing planets (upgrade check from `main`: "Verdance: wayshard_vein added to its ores", players kept). 6 unit and 4 end-to-end tests.
- [x] Client: E on conduits and surfaces, the portal window (size, link, open or dormant and why, tank bar, buttons with reasons when disabled, carried tokens by name), token tooltips, `Teleport` taken as a jump with no correction, `--demo-ui portal`; screenshot `docs/media/m8-portal-window.png`
- [x] Art: wayshard vein, conduit, portal surface, wayshard and token icons
- [x] Integrated: 329 tests, fmt and clippy clean
- [x] Follow-ups done 2026-10-27 (protocol 41): `ServerMsg::PortalClosed { id }` names the portal it closes, so a late close never shuts a window reopened on another; `PortalView::id` lets the window grey out this portal's own token.
- [x] Follow-up: arriving players face out of the far portal, the way they walked in (`Teleport::yaw`, `Travel::yaw`, `movement::yaw_toward`; protocol 47, 2026-10-28)
- [ ] Nathan's play-test
