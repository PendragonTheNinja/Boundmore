# Milestone 30: Sound

*Spec written 2026-10-29. Nathan asked for it the same day ("Yes sound", polish batch 3, `docs/feel.md`). Earlier: "I don't really know how sound and music will work for the game. I assume these are things you can't generate yourself."*

Boundmore is silent. Every swing, break, pickup, landing and hit is felt only through numbers and cracks; the polish audit ranks it the largest feel gap against Boundless (`docs/audit/feel-and-controls.md`, Sound, ranked item 8). This milestone gives every common action a sound, made by us, and a volume page in the options window. Music waits for Nathan (decision 1).

## What Boundless sounded like (forum; the wiki has no audio page)

- **Every swing of a tool on a block made an impact sound, and the break its own.** Players counted swings by ear: "It is cool to be able to count along while mining to see how many impacts are needed to break a material" (topic 2850, 2015, reporting a bug where every third impact was silent). The pickup had its own "chomp" as the item flew in (audit, topic 20855).
- **Footsteps changed with the surface** (asked for in 2015, topic 349: "solid rock/stone, wood, sand, dirt, snow, swamp, grass and the leafs of trees"), and players noticed when the wrong one played: a "squishy floor sound" of puddles on brick and metal floors after an update was reported as a bug (topic 36506, 2019).
- **Doors gained sounds in January 2019**; players liked them but found them "crazy loud" at first (topic 24565). The same month's **combat sounds** were disliked by some ("like someone bashing me in the ear with a bongo drum", a Wildstock hit "BWOOM", the new fall sound "not needed", topic 25340); others liked hearing that damage was blocked. Lesson: hurt and hit sounds should be soft and short, and the fall sound quiet.
- **Sounds dropped out** when many played at once ("it did sound like it was hitting a maximum voice count", topic 22100, 2018); mining strikes went silent until a restart. Spanners other than stone had no sound at all (same topic).
- **Ambience:** birds on the surface, a howling wind underground and high in the sky (topics 9792, 40111, 6522; ecosystem ambiences were added in 2017). A repetitive bird on peaceful worlds was "the most rage-inducing thing I've ever run into" (topic 16920), and players asked for quiet ("Silence/near silence is ambient as well", topic 40111). Flowing water had no sound and players asked for one (topics 38023, 31821). Players wished machines would make a small sound when done ("like a kitchen timer ding", topic 17961).
- **Music:** composed by Peter Gregson, 49 themes in 4 variations each, chosen by situation (combat, weather, world tier...; topic 37673). Players loved it (topic 34497) but wanted it more often (topic 15943), or less in fights; many turned it off and played their own.
- **Accessibility:** hard-of-hearing players asked that sounds that warn (an attack, a direction of damage) also show on screen (topic 8482). Our HUD already shows the direction of a hit and a creature's wind-up pose.

Nothing of Boundless's audio is copied or sampled: every Boundmore sound is synthesised by our own generator from noise, filters and envelopes.

## What is built

1. **The generator, `tools/bm-soundgen`.** Like `bm-texgen`, it makes every sound from code: layered noise, filtered impacts, pitched partials and envelopes, aimed at a soft, warm, rounded fantasy feel (no harsh clicks, low-passed tops, short tails). `cargo run --release -p bm-soundgen` rewrites `assets/sounds/`. It is deterministic: the same code makes the same bytes. Each sound comes in several variants (`swing_hammer_1.wav` to `_4`), each a little different in pitch, colour and timing.
2. **Files are data.** `assets/sounds/<name>_<n>.wav` (16-bit mono, 22,050 Hz, which keeps the folder near 2 MB and suits soft sounds). Any file may be replaced with a hand-made or free (CC0) one of the same name, as `.wav` or `.ogg` (an `.ogg` wins over a `.wav` of the same name); a sound may have 1 to 8 variants, and the game uses as many as exist. A missing sound is a warning in the log and the title bar, never a crash. `--assets DIR` points at another set, as for textures.
3. **Playback, `crates/bm-audio`.** A small layer over `rodio` (WASAPI on Windows, ALSA on Linux). A sound from the world plays positionally: its volume falls with distance from the camera and it pans left or right by direction (both computed by us, across the planet's seam with `PlanetShape::delta`). Interface sounds play flat. Each play picks a random variant and a slight random pitch, never the same variant twice in a row. At most 32 sounds play at once and the same sound at most a few times in 50 ms (repeats are dropped, never queued), so a burst of breaks cannot swamp the mixer or drop the sounds that matter (Boundless's lost mining strikes, topic 22100). With no audio device (a server box, the Linux sandbox, CI) the game runs silently: no error dialogs, no stalls, one line in the log.
4. **Volumes in the options window:** master, effects and interface sliders, 0 to 100 (music joins them with music). Old settings files load with the defaults (master 80, effects 100, interface 70).
5. **The first sounds** (below), hooked where the client already learns of each event. No protocol change.

| Sound | When | Where from |
|---|---|---|
| `swing_hammer`, `swing_axe`, `swing_shovel`, `swing_light` | Each swing (by tool class; the hand stone and others swing light) | Ours, flat |
| `hit_stone`, `hit_wood`, `hit_soil`, `hit_plant`, `hit_metal`, `hit_glass`, `hit_crystal` | A swing landing on a block | The block |
| `break_stone`, `break_wood`, `break_soil`, `break_plant`, `break_metal`, `break_glass`, `break_crystal` | A block breaking, ours or another player's | The block |
| `place` (soft) and `place_hard` | Placing a block; another player's placing | The block |
| `chisel` | A chisel cut or fill | The block |
| `pickup` | Items coming into the pack | Flat |
| `step_stone`, `step_soil`, `step_wood`, `step_sand`, `step_snow`, `step_water` | Footsteps by the surface under the feet, ours and other players' | The feet |
| `jump`, `land` (scaled by the fall), `splash` | Leaving the ground, landing, entering water | Ours |
| `hurt`, `defeat` | Health lost; defeated | Ours, flat |
| `creature_hit` | Our strike or shot landing on a creature | The creature |
| `creature_windup`, `popper_whistle` | A creature rearing or aiming; a popper lit | The creature |
| `sling`, `throw` | A sling shot; a bomb thrown | Ours |
| `blast` | A bomb or popper blast | The blast |
| `grapple_fire`, `grapple_stick`, `grapple_release`, `reel` | A hook flying, sticking, let go; ticks as the rope is reeled in | Ours, flat |
| `door_open`, `door_close` | A door or trapdoor swinging, anyone's | The door |
| `click` | A button in a menu | Flat |
| `craft` | A craft or station job asked for | Flat |
| `coin` | Coin coming in | Flat |
| `notice` | A notice line | Flat |
| `warp` | Arriving on another planet; a portal trip | Flat |
| `meteor_land` | A meteor landing | The site |

Materials come from the block's catalogue `material` (rock, soil, wood, plant, crystal) with its name for what the catalogue does not say yet (metal, glass, sand, snow, gravel, water). A sound catalogue field could replace the name rules later.

## Acceptance

- `cargo run --release -p bm-soundgen` writes every sound the game names; two runs give identical files (a test); the folder stays under 3 MB.
- A test fails if the game names a sound with no file.
- Positional maths is tested: full volume near, quieter with distance, silent past the range, left and right pan from the camera's yaw, the same across the seam as beside it.
- The options window shows the three volume sliders; old settings files load; settings round-trip.
- Without an audio device the client starts, plays, and every play is a no-op (a test opens playback the way the game does and plays every sound).
- Every hook above plays in a real game on Nathan's machine (his listening test, below).

## Non-goals (this milestone)

- **Music** (decision 1).
- **Ambience**: birds, wind, caves, rain, flowing water, lava (decision 2).
- Machine hums and a job-done chime (decision 3), spanner and repair sounds, eating and drinking sounds, sign writing, chat pings.
- Reverb, occlusion through walls, Doppler.
- A sound field in the block catalogue (the name rules do for now).

## Decisions for Nathan

1. **Music.** We cannot compose like Peter Gregson by code. Choices: (a) commission or buy a small set of tracks (the game would play them by situation, as Boundless did, with a music volume and a "how often" setting players asked for, topic 15943); (b) free CC0 or CC-BY music, chosen with him; (c) a procedural ambient score from our generator (soft pads and plucked notes in a key per world kind; cheap, but it will sound generated); (d) none for now. Recommended: (d) until the sound effects feel right, then (a) or (b).
2. **Ambience.** Boundless had birds by day, wind high up and in caves. Players loved it but hated repetition. Build it (per world kind, with long gaps and a separate volume), or leave the world quiet but for what players do? Recommended: build it next, sparse, with its own slider.
3. **Machines.** A small chime when a station's job is done (topic 17961), or silence?
4. **The character of the set.** After listening: softer, crisper, deeper or brighter? Which sounds grate after an hour of mining? His words become generator settings.

## Listening guide (for Nathan)

Build and run as usual (`cargo run --release -p bm-server -- --creative`, then the client). Turn the game's master volume to taste in Options (Esc twice).

1. **Mining (most heard).** Hammer a stone face for a minute: does each impact sound like a hit, can you count swings, does the break stand out? Then wood with an axe and soil with a shovel. Does any repeat grate?
2. **Walking.** Walk on stone, turf, timber, sand and snow, and wade in water: does each surface sound different, is the pace right at walk and sprint? Jump and fall a few blocks: is the landing too loud (Boundless's was disliked)?
3. **Building.** Place a row of blocks and chisel one: soft enough to repeat a thousand times?
4. **Fighting.** Hit a grazer, let it hit you, fight a popper (listen for its whistle before it blows) and throw a bomb.
5. **Grapple.** Fire, stick, reel in, let go.
6. **Menus.** Craft something, open and close a door, travel through a shrine.
7. **With a friend** (or `bots --count 2`): do their footsteps and breaks come from where they are, left and right?

Say which sounds are wrong and how (too loud, too sharp, too long, wrong feel); the generator's settings change from his words.

## Status

- [x] Research and spec (2026-10-29)
- [ ] Generator and the first sounds
- [ ] Playback crate, volumes in options
- [ ] Hooks in the client
- [ ] Nathan's listening test and decisions 1 to 4
