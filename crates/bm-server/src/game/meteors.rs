//! Regions and meteorites in play (milestone 14,
//! `docs/milestones/14-meteorites.md`): each planet's named regions, players
//! discovering them, and on tier 3 and higher planets the meteors that fall
//! when a fresh region centre (or, more rarely, a fresh square) is reached:
//! the fall, the crater and shell, the waves dropping through portals, meteor
//! lives, and each player's own share of the heart. The rules and numbers are
//! `bm_core::meteors` and `bm_core::regions`.

use super::creatures::{Fight, Mind, surface};
use super::mining::roll;
use super::{Game, Refusal, send_slots};
use crate::store::{SavedMeteor, SavedSky};
use bm_core::creatures::Behaviour;
use bm_core::meteors as rules;
use bm_core::regions::{LandClass, RegionMap, RegionNames, discovery_xp, layout_seed};
use bm_core::{BlockPos, BlockState, PlanetShape};
use bm_protocol::{MeteorPhase, MeteorView, ServerMsg};
use glam::DVec3;
use std::collections::{HashMap, HashSet};

/// A meteor life as `ServerMsg::MeteorLife` tells it.
type Life = Option<(u32, Option<u32>)>;

/// Lives, zones and waves are checked this often (ticks).
const CHECK_EVERY: u64 = 10;
/// Wave creatures are pointed at a life holder this often (Boundless:
/// "periodically").
const RETARGET_EVERY: u64 = 5 * 60;
/// How far from its portal a wave creature lands (blocks, **guess**).
const DROP_SPREAD: f64 = 3.0;
/// How high the meteor starts above the site (blocks).
const FALL_HEIGHT: f64 = 220.0;
/// Tries at finding a landing site.
const SITE_TRIES: usize = 16;
/// A wave creature that has come no nearer than this to any player in
/// [`STUCK_TICKS`], with no player within [`STUCK_NEAR`], is stuck (in a pit
/// by the shell, say, pacing under its target) and drops back in through a
/// portal, so a meteor can always be beaten (**our rule**). Measured as
/// progress toward the players, not movement: a creature pacing in a pit
/// moves all the time and never arrives.
const STUCK_MOVE: f64 = 1.0;
const STUCK_TICKS: u64 = 8 * 60;
const STUCK_NEAR: f64 = 4.0;

/// One look at a wave creature `gap` from the nearest player at `tick`,
/// against `entry` (its nearest approach and since when): whether it is
/// stuck and should drop back in (the entry starts again either way).
fn stuck_step(entry: &mut (f64, u64), gap: f64, tick: u64) -> bool {
    if gap <= STUCK_NEAR || gap < entry.0 - STUCK_MOVE {
        *entry = (gap, tick);
        false
    } else if tick >= entry.1 + STUCK_TICKS {
        *entry = (gap, tick);
        true
    } else {
        false
    }
}

/// A meteor's phase on the server.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Phase {
    Falling {
        lands_at: u64,
    },
    /// Waves dropping: the current wave (1-based), and when the next drops
    /// once this one is cleared (None while it fights).
    Fighting {
        wave: u8,
        next_at: Option<u64>,
    },
    Cleared {
        until: u64,
    },
    Dormant {
        until: u64,
    },
}

#[derive(Clone, Debug)]
pub(super) struct Meteor {
    pub id: u32,
    /// Where it lands: the heart.
    pub site: BlockPos,
    pub from: DVec3,
    pub level: u8,
    pub phase: Phase,
    pub impact: u64,
    pub beam_until: u64,
    pub portals: [DVec3; rules::PORTALS],
    /// Creatures of the current wave.
    pub creatures: Vec<u32>,
    /// A creature of it has died: lives are handed out from now on.
    pub first_kill: bool,
    /// Life holders by account, and since when each has been outside the
    /// zone (None: inside).
    pub lives: HashMap<String, Option<u64>>,
    /// Accounts owed a full share once cleared (held a life, or in the zone
    /// at the end).
    pub entitled: HashSet<String>,
    pub looted: HashSet<String>,
    /// The shell's blocks, and how many have been broken.
    pub shell: Vec<BlockPos>,
    pub broken: u32,
    /// Each wave creature's nearest approach to a player since it last got
    /// closer, and since when.
    pub stuck: HashMap<u32, (f64, u64)>,
}

impl Meteor {
    fn active(&self) -> bool {
        matches!(self.phase, Phase::Falling { .. } | Phase::Fighting { .. })
    }

    fn dormant(&self) -> bool {
        matches!(self.phase, Phase::Dormant { .. })
    }

    fn view(&self, account: &str) -> MeteorView {
        let waves = rules::waves(self.level);
        let phase = match self.phase {
            Phase::Falling { lands_at } => MeteorPhase::Falling { lands_at },
            Phase::Fighting { wave, .. } => MeteorPhase::Fighting {
                wave,
                waves,
                left: self.creatures.len() as u16,
                lives: self.lives.len() as u16,
            },
            Phase::Cleared { until } => MeteorPhase::Cleared { until },
            Phase::Dormant { until } => MeteorPhase::Dormant { until },
        };
        let portals = if matches!(self.phase, Phase::Fighting { .. }) {
            self.portals.map(|p| p.as_vec3().to_array())
        } else {
            [[0.0; 3]; 3]
        };
        MeteorView {
            id: self.id,
            from: self.from.as_vec3().to_array(),
            site: self.site,
            level: self.level,
            phase,
            portals,
            beam_until: self.beam_until,
            looted: self.looted.contains(account),
        }
    }

    fn centre(&self) -> DVec3 {
        DVec3::new(
            self.site.x as f64 + 0.5,
            self.site.y as f64 + 0.5,
            self.site.z as f64 + 0.5,
        )
    }
}

/// One planet's regions and meteors.
pub(super) struct Sky {
    pub layout_seed: u64,
    pub regions: RegionMap,
    pub names: Vec<String>,
    /// Each region rests until this tick.
    pub rest_until: Vec<u64>,
    /// Wandering-trigger squares rest until these ticks.
    pub squares: HashMap<(i64, i64), u64>,
    pub meteors: Vec<Meteor>,
    pub dirty: bool,
}

impl Sky {
    /// Lays out and names a planet's regions, and restores what was saved.
    pub fn new(
        name: &str,
        kind: &str,
        shape: PlanetShape,
        names: &RegionNames,
        land: impl Fn(i64, i64) -> LandClass,
        saved: Option<SavedSky>,
        tick: u64,
    ) -> Self {
        let seed = layout_seed(name, shape);
        let regions = RegionMap::new(shape, seed);
        let lands: Vec<LandClass> = (0..regions.len())
            .map(|i| {
                let [x, z] = regions.centre(i);
                land(x as i64, z as i64)
            })
            .collect();
        let region_names = names.name_all(kind, &lands, seed);
        let saved = saved.unwrap_or_default();
        let mut rest_until = saved.rest_until;
        rest_until.resize(regions.len(), 0);
        let meteors = saved
            .meteors
            .into_iter()
            .map(|m| {
                let until = m.until.max(tick + rules::HEART_TICKS / 2);
                Meteor {
                    id: m.id,
                    site: m.site,
                    from: DVec3::ZERO,
                    level: m.level,
                    phase: if m.cleared {
                        Phase::Cleared { until }
                    } else {
                        Phase::Dormant { until }
                    },
                    impact: tick,
                    beam_until: 0,
                    portals: [DVec3::ZERO; rules::PORTALS],
                    creatures: Vec::new(),
                    first_kill: true,
                    lives: HashMap::new(),
                    entitled: m.entitled.into_iter().collect(),
                    looted: m.looted.into_iter().collect(),
                    shell: m.shell,
                    broken: m.broken,
                    stuck: HashMap::new(),
                }
            })
            .collect();
        Self {
            layout_seed: seed,
            regions,
            names: region_names,
            rest_until,
            squares: saved.squares.into_iter().map(|(x, z, t)| ((x, z), t)).collect(),
            meteors,
            dirty: false,
        }
    }

    pub fn saved(&self) -> SavedSky {
        SavedSky {
            rest_until: self.rest_until.clone(),
            squares: self.squares.iter().map(|(&(x, z), &t)| (x, z, t)).collect(),
            meteors: self
                .meteors
                .iter()
                .map(|m| {
                    let (until, cleared) = match m.phase {
                        Phase::Cleared { until } => (until, true),
                        Phase::Dormant { until } => (until, false),
                        _ => (0, false),
                    };
                    SavedMeteor {
                        id: m.id,
                        site: m.site,
                        level: m.level,
                        until,
                        cleared,
                        entitled: m.entitled.iter().cloned().collect(),
                        looted: m.looted.iter().cloned().collect(),
                        shell: m.shell.clone(),
                        broken: m.broken,
                    }
                })
                .collect(),
        }
    }

    /// The meteor whose blocks include `canon`.
    fn meteor_at(&self, canon: BlockPos) -> Option<usize> {
        self.meteors
            .iter()
            .position(|m| m.site == canon || m.shell.contains(&canon))
    }
}

/// The land class of column (x, z) on a planet made by `gen`.
pub(super) fn land_of(generator: &bm_worldgen::Generator, x: i64, z: i64) -> LandClass {
    let cfg = generator.config();
    let water = cfg.sea_level + cfg.water_offset;
    LandClass::of(generator.surface_height(x, z), water, cfg.sea_level + cfg.snow_line)
}

impl Game {
    /// The message telling a player a planet's regions.
    pub(super) fn regions_msg(&self, pi: usize, account: &str) -> ServerMsg {
        let sky = &self.skies[pi];
        let id = self.planets[pi].id;
        ServerMsg::Regions {
            layout_seed: sky.layout_seed,
            names: sky.names.clone(),
            discovered: self
                .discoveries
                .get(account)
                .and_then(|d| d.get(&id))
                .cloned()
                .unwrap_or_default(),
        }
    }

    /// Tells a player who just arrived on planet `pi` its regions and meteors.
    pub(super) fn arrived_in_sky(&mut self, id: u32) {
        let Some(p) = self.players.get_mut(&id) else { return };
        p.region = None;
        p.square = None;
        let (pi, account) = (p.planet, p.account.clone());
        // Explorer feats (milestone 24): each world the first time.
        let world = format!("world:{}", self.planets[pi].id);
        if self.ledgers.get(&account).is_some_and(|l| l.count(&world) == 0) {
            self.count(id, &world, 1);
            self.count(id, "worlds", 1);
        }
        let msg = self.regions_msg(pi, &account);
        let p = &self.players[&id];
        p.out.send(&msg);
        for m in &self.skies[pi].meteors {
            p.out.send(&ServerMsg::Meteor(m.view(&account)));
        }
    }

    fn tell_meteor(&self, pi: usize, m: &Meteor) {
        for o in self.players.values().filter(|o| o.planet == pi) {
            o.out.send(&ServerMsg::Meteor(m.view(&o.account)));
        }
    }

    /// Whether a swing or blast may not touch `canon`: a block of a meteor
    /// still guarded by its creatures.
    pub(super) fn meteor_guards(&self, pi: usize, canon: BlockPos) -> Option<Refusal> {
        let sky = &self.skies[pi];
        let m = &sky.meteors[sky.meteor_at(canon)?];
        m.active()
            .then(|| Refusal::Told("The meteorite is guarded until its creatures are beaten".into()))
    }

    /// A shell block of a meteor was broken by player `id`: every fourth
    /// gives a portal conduit.
    pub(super) fn meteor_shell_broken(&mut self, id: u32, pi: usize, canon: BlockPos) {
        let sky = &mut self.skies[pi];
        let Some(i) = sky.meteor_at(canon) else { return };
        let m = &mut sky.meteors[i];
        m.shell.retain(|b| *b != canon);
        m.broken += 1;
        sky.dirty = true;
        if m.broken % rules::SHELL_PER_CONDUIT != 0 {
            return;
        }
        let Ok(conduit) = self.items.id("portal_conduit") else {
            return;
        };
        let Some(p) = self.players.get_mut(&id) else { return };
        let before = p.inventory.clone();
        if p.inventory.add(&self.items, conduit, 1) == 0 {
            p.out.send(&ServerMsg::Notice {
                text: "Your inventory is full".into(),
            });
        }
        send_slots(p, &before);
    }

    /// `LootHeart`: player `id` takes their share from the heart at `pos`.
    pub(super) fn loot_heart(&mut self, id: u32, pos: BlockPos) {
        let Some(p) = self.players.get(&id) else { return };
        let pi = p.planet;
        let shape = self.planets[pi].world.shape;
        if !shape.contains_y(pos.y) {
            return;
        }
        let canon = shape.wrap_block(pos);
        let account = p.account.clone();
        let reach = self.cfg.reach + p.effects.reach_bonus() + 1.0;
        let eye = p.state.eye(&self.movement);
        let mid = DVec3::new(canon.x as f64 + 0.5, canon.y as f64 + 0.5, canon.z as f64 + 0.5);
        if shape.delta(eye, mid).length() > reach + 0.9 {
            return self.notice(id, "Too far from the meteor heart");
        }
        let sky = &self.skies[pi];
        let Some(i) = sky.meteors.iter().position(|m| m.site == canon) else {
            return;
        };
        let m = &sky.meteors[i];
        if m.active() {
            return self.notice(id, "The heart is sealed until its creatures are beaten");
        }
        if m.looted.contains(&account) {
            return self.notice(id, "You have taken your share of this meteor");
        }
        // A cleared meteor pays its full share to those who fought for it;
        // anyone else (and everyone, once dormant) gets the dormant share.
        let full = !m.dormant() && m.entitled.contains(&account);
        let level = m.level;
        let tier = self.planets[pi].tier;
        let rng = &mut self.rng;
        let loot = self.meteor_loot.roll(level, tier, !full, || roll(rng));
        let Some(p) = self.players.get_mut(&id) else { return };
        let before = p.inventory.clone();
        let mut lost = false;
        let mut got = Vec::new();
        for (name, n) in &loot {
            let Ok(item) = self.items.id(name) else { continue };
            let left = p.inventory.add(&self.items, item, (*n).min(u16::MAX as u32) as u16);
            lost |= left > 0;
            let given = *n as u16 - left;
            if given > 0 {
                got.push(format!(
                    "{given} {}",
                    self.items.get(item).map_or(name.as_str(), |d| d.display.as_str())
                ));
            }
        }
        send_slots(p, &before);
        let mut text = format!("From the meteor heart: {}", got.join(", "));
        if lost {
            text.push_str(" (your inventory is full; the rest is lost)");
        }
        p.out.send(&ServerMsg::Notice { text });
        let share = if full { "full" } else { "dormant" };
        tracing::info!(
            "{} looted meteor {} ({share}): {loot:?}",
            p.name,
            self.skies[pi].meteors[i].id
        );
        let sky = &mut self.skies[pi];
        sky.meteors[i].looted.insert(account);
        sky.dirty = true;
        let view = sky.meteors[i].view(&self.players[&id].account);
        self.players[&id].out.send(&ServerMsg::Meteor(view));
        self.count(id, "hearts", 1);
    }

    /// Player `id` returned from defeat: their meteor life is gone.
    pub(super) fn meteor_life_lost(&mut self, id: u32) {
        let Some(p) = self.players.get(&id) else { return };
        let (pi, account) = (p.planet, p.account.clone());
        let mut changed = None;
        for m in &mut self.skies[pi].meteors {
            if m.lives.remove(&account).is_some() {
                changed = Some(m.id);
            }
        }
        if changed.is_some() {
            p.out.send(&ServerMsg::MeteorLife { life: None });
            self.notice(id, "You lost your meteor life");
        }
    }

    /// Each tick: discoveries and triggers as players move, then every
    /// meteor's phase.
    pub(super) fn step_meteors(&mut self) {
        if self.cfg.placed_meteor.is_some() {
            self.place_meteor();
        }
        let ids: Vec<u32> = self.players.keys().copied().collect();
        for id in ids {
            self.watch_player(id);
        }
        for pi in 0..self.planets.len() {
            if self.skies[pi].meteors.is_empty() {
                continue;
            }
            let mut i = 0;
            while i < self.skies[pi].meteors.len() {
                if self.step_meteor(pi, i) {
                    i += 1;
                } else {
                    let m = self.skies[pi].meteors.remove(i);
                    self.skies[pi].dirty = true;
                    self.meteor_closed(pi, m);
                }
            }
        }
    }

    /// A player's region and square: discovery when they enter a new region,
    /// and the two triggers.
    fn watch_player(&mut self, id: u32) {
        let Some(p) = self.players.get(&id) else { return };
        if p.state.down || p.trip.is_some() {
            return;
        }
        let pi = p.planet;
        let pos = p.state.pos;
        let sky = &self.skies[pi];
        let place = sky.regions.place(pos.x, pos.z);
        let square = (pos.x.floor() as i64).div_euclid(rules::SQUARE);
        let square = (square, (pos.z.floor() as i64).div_euclid(rules::SQUARE));
        let entered_region = p.region != Some(place.region);
        let at_centre = place.direction.is_none();
        let was_at_centre = p.at_centre;
        let entered_square = p.square != Some(square);
        let account = p.account.clone();
        let p = self.players.get_mut(&id).expect("present");
        p.region = Some(place.region);
        p.at_centre = at_centre;
        p.square = Some(square);
        if entered_region {
            self.discover(id, pi, &account, place.region);
        }
        if self.planets[pi].tier < rules::MIN_WORLD_TIER {
            return;
        }
        let tick = self.tick;
        if at_centre && !was_at_centre && self.skies[pi].rest_until[place.region] <= tick {
            let rest = rules::REGION_REST_TICKS;
            let span = (rest.1 - rest.0) as f64;
            let until = tick + rest.0 + (roll(&mut self.rng) * span) as u64;
            self.skies[pi].rest_until[place.region] = until;
            self.skies[pi].dirty = true;
            self.start_meteor(pi, pos);
        } else if entered_square {
            let sky = &mut self.skies[pi];
            let fresh = sky.squares.get(&square).is_none_or(|&t| t <= tick);
            sky.squares.insert(square, tick + rules::SQUARE_REST_TICKS);
            sky.squares.retain(|_, t| *t > tick);
            sky.dirty = true;
            if fresh && roll(&mut self.rng) < rules::SQUARE_CHANCE {
                self.start_meteor(pi, pos);
            }
        }
    }

    /// Records a region discovery and gives its XP.
    fn discover(&mut self, id: u32, pi: usize, account: &str, region: usize) {
        let planet_id = self.planets[pi].id;
        let list = self
            .discoveries
            .entry(account.to_string())
            .or_default()
            .entry(planet_id)
            .or_default();
        if list.contains(&(region as u16)) {
            return;
        }
        list.push(region as u16);
        let nth = list.len() as u32;
        self.discoveries_dirty.insert(account.to_string());
        self.count(id, "regions", 1);
        let xp = if self.cfg.discovery_xp { discovery_xp(nth) } else { 0 };
        let name = &self.skies[pi].names[region];
        if let Some(p) = self.players.get(&id) {
            p.out.send(&ServerMsg::Discovered {
                region: region as u16,
                nth,
                xp,
            });
            tracing::debug!("{} discovered {name} ({nth} on {})", p.name, self.planets[pi].name);
        }
        if self.cfg.discovery_xp {
            self.add_xp(id, xp);
        }
    }

    /// The configured test meteor, once a player is on the start planet and
    /// its ground is resident.
    fn place_meteor(&mut self) {
        let pi = self.start;
        let Some([dx, dz]) = self.cfg.placed_meteor else { return };
        let Some(&id) = self.players.iter().find(|(_, p)| p.planet == pi).map(|(id, _)| id) else {
            return;
        };
        let at = self.planets[pi].arrival().pos;
        let (x, z) = ((at.x + dx).floor() as i64, (at.z + dz).floor() as i64);
        let Some(feet) = surface(&self.planets[pi].world, x, z, at.y as i64 + 8, at.y as i64 - 16, 3.0) else {
            return;
        };
        let site = BlockPos::containing(feet.x, feet.y - 1.0, feet.z);
        let near = self.players[&id].state.pos;
        self.cfg.placed_meteor = None;
        self.launch_meteor(pi, near, self.planets[pi].world.shape.wrap_block(site));
    }

    /// Brings a meteor down near `near` on planet `pi`, if there is room and
    /// a good site.
    pub(super) fn start_meteor(&mut self, pi: usize, near: DVec3) -> Option<u32> {
        if self.skies[pi].meteors.iter().filter(|m| m.active()).count() >= rules::PLANET_CAP {
            return None;
        }
        let site = (0..SITE_TRIES).find_map(|_| {
            let a = roll(&mut self.rng) * std::f64::consts::TAU;
            let (lo, hi) = rules::IMPACT_RANGE;
            let r = lo + roll(&mut self.rng) * (hi - lo);
            let (x, z) = (
                (near.x + a.sin() * r).floor() as i64,
                (near.z + a.cos() * r).floor() as i64,
            );
            self.landing_site(pi, x, z, near.y as i64)
        })?;
        Some(self.launch_meteor(pi, near, site))
    }

    /// Starts the fall of a meteor sized by the players near `near`, to land
    /// at `site`.
    fn launch_meteor(&mut self, pi: usize, near: DVec3, site: BlockPos) -> u32 {
        let shape = self.planets[pi].world.shape;
        let players = self
            .players
            .values()
            .filter(|o| o.planet == pi && !o.state.down)
            .filter(|o| shape.delta(near, o.state.pos).length() <= rules::COUNT_RADIUS)
            .count();
        let level = rules::level_for(players);
        let id = self.next_meteor;
        self.next_meteor = self.next_meteor.wrapping_add(1).max(1);
        let tick = self.tick;
        let from = DVec3::new(
            site.x as f64 + 0.5 - 60.0 + roll(&mut self.rng) * 120.0,
            site.y as f64 + FALL_HEIGHT,
            site.z as f64 + 0.5 - 60.0 + roll(&mut self.rng) * 120.0,
        );
        let m = Meteor {
            id,
            site,
            from,
            level,
            phase: Phase::Falling {
                lands_at: tick + rules::FALL_TICKS,
            },
            impact: tick + rules::FALL_TICKS,
            beam_until: tick + rules::FALL_TICKS + rules::BEAM_TICKS,
            portals: [DVec3::ZERO; rules::PORTALS],
            creatures: Vec::new(),
            first_kill: false,
            lives: HashMap::new(),
            entitled: HashSet::new(),
            looted: HashSet::new(),
            shell: Vec::new(),
            broken: 0,
            stuck: HashMap::new(),
        };
        tracing::info!(
            "a level {level} meteor falls on {} at {:?} ({players} players near)",
            self.planets[pi].name,
            site
        );
        self.tell_meteor(pi, &m);
        self.skies[pi].meteors.push(m);
        self.skies[pi].dirty = true;
        id
    }

    /// A place a meteor may land around column (x, z): resident, dry, open
    /// ground, clear of claims, shrines, portals and other meteors. The heart
    /// sits one block into the ground.
    fn landing_site(&self, pi: usize, x: i64, z: i64, near_y: i64) -> Option<BlockPos> {
        let planet = &self.planets[pi];
        let world = &planet.world;
        let shape = world.shape;
        let top = (near_y + 48).min(shape.height_blocks() - 1);
        let feet = surface(world, x, z, top, (near_y - 64).max(1), 3.0)?;
        let ground = BlockPos::containing(feet.x, feet.y - 1.0, feet.z);
        // Everything the crater and shell touch must be resident.
        let r = rules::CRATER_RADIUS + 1;
        for (dx, dz) in [(-r, -r), (r, -r), (-r, r), (r, r)] {
            for dy in [-r, r] {
                let b = ground.offset(dx, dy, dz);
                if shape.contains_y(b.y) && world.state(shape.wrap_block(b)).is_none() {
                    return None;
                }
            }
        }
        let centre = DVec3::new(ground.x as f64 + 0.5, ground.y as f64, ground.z as f64 + 0.5);
        let shrine = planet.shrine;
        let shrine_at = DVec3::new(shrine.x as f64, shrine.y as f64, shrine.z as f64);
        if shape.delta(centre, shrine_at).length() < rules::CLEARANCE {
            return None;
        }
        for c in planet.claims.all() {
            for plot in &c.plots {
                let o = plot.origin();
                let half = bm_core::plots::PLOT_SIZE as f64 * 0.5;
                let mid = DVec3::new(o.x as f64 + half, o.y as f64 + half, o.z as f64 + half);
                if shape.delta(centre, mid).length() < rules::CLEARANCE {
                    return None;
                }
            }
        }
        if self.skies[pi]
            .meteors
            .iter()
            .any(|m| shape.delta(centre, m.centre()).length() < rules::ZONE_RADIUS * 2.0)
        {
            return None;
        }
        Some(shape.wrap_block(ground))
    }

    /// One meteor's tick; false when it is done and should go.
    fn step_meteor(&mut self, pi: usize, i: usize) -> bool {
        let tick = self.tick;
        let phase = self.skies[pi].meteors[i].phase;
        match phase {
            Phase::Falling { lands_at } => {
                if tick >= lands_at {
                    self.impact(pi, i);
                }
                true
            }
            Phase::Fighting { wave, next_at } => {
                if tick % CHECK_EVERY == 0 {
                    self.check_fight(pi, i, wave, next_at);
                }
                true
            }
            Phase::Cleared { until } | Phase::Dormant { until } => tick < until,
        }
    }

    /// The meteor lands: the crater, the shell and heart, the portals, and
    /// the first wave.
    fn impact(&mut self, pi: usize, i: usize) {
        let shape = self.planets[pi].world.shape;
        let site = self.skies[pi].meteors[i].site;
        let (shell, heart) = (self.meteor_shell_id, self.meteor_heart_id);
        let cr = rules::CRATER_RADIUS;
        // The crater: everything breakable in a bowl above the heart goes
        // (no drops); what is guarded or not resident stays.
        for dx in -cr..=cr {
            for dz in -cr..=cr {
                for dy in -1..=cr {
                    let d = ((dx * dx + dz * dz) as f64).sqrt() + (dy.max(0) as f64) * 0.3;
                    if d > cr as f64 || (dy < 0 && d > cr as f64 - 1.5) {
                        continue;
                    }
                    let b = site.offset(dx, dy + 1, dz);
                    if !shape.contains_y(b.y) {
                        continue;
                    }
                    let b = shape.wrap_block(b);
                    if self.planets[pi].guarded(b) {
                        continue;
                    }
                    let Some(state) = self.planets[pi].world.state(b) else {
                        continue;
                    };
                    let def = self.planets[pi].world.registry.get(state.id);
                    if state.id.is_air() || !def.breakable || def.storage > 0 || def.station.is_some() {
                        continue;
                    }
                    self.block_gone(pi, b, state.id, None);
                }
            }
        }
        // The shell: a rough ball round the heart, sunk half into the ground.
        let sr = rules::SHELL_RADIUS;
        let mut placed = Vec::new();
        for dx in -sr..=sr {
            for dy in -sr..=sr {
                for dz in -sr..=sr {
                    let d2 = dx * dx + dy * dy + dz * dz;
                    if d2 > sr * sr + 1 || (dx, dy, dz) == (0, 0, 0) {
                        continue;
                    }
                    let b = site.offset(dx, dy + 1, dz);
                    if !shape.contains_y(b.y) {
                        continue;
                    }
                    let b = shape.wrap_block(b);
                    if self.planets[pi].guarded(b) || self.planets[pi].world.state(b).is_none() {
                        continue;
                    }
                    self.change_block(pi, b, BlockState::plain(shell), None);
                    placed.push(b);
                }
            }
        }
        let heart_at = shape.wrap_block(site.offset(0, 1, 0));
        self.change_block(pi, heart_at, BlockState::plain(heart), None);
        let tick = self.tick;
        let mut portals = [DVec3::ZERO; rules::PORTALS];
        for (k, p) in portals.iter_mut().enumerate() {
            let a = k as f64 / rules::PORTALS as f64 * std::f64::consts::TAU + roll(&mut self.rng);
            *p = DVec3::new(
                site.x as f64 + 0.5 + a.sin() * 7.0,
                site.y as f64 + rules::PORTAL_HEIGHT,
                site.z as f64 + 0.5 + a.cos() * 7.0,
            );
        }
        let m = &mut self.skies[pi].meteors[i];
        m.site = heart_at;
        m.shell = placed;
        m.portals = portals;
        m.impact = tick;
        m.phase = Phase::Fighting { wave: 1, next_at: None };
        self.skies[pi].dirty = true;
        self.drop_wave(pi, i, 1);
        let m = &self.skies[pi].meteors[i];
        self.tell_meteor(pi, m);
    }

    /// Wave `wave` drops through the portals.
    fn drop_wave(&mut self, pi: usize, i: usize, wave: u8) {
        let level = self.skies[pi].meteors[i].level;
        let last = wave == rules::waves(level);
        let portals = self.skies[pi].meteors[i].portals;
        let world_tier = self.planets[pi].tier;
        let kinds: Vec<u16> = (0..self.creature_kinds.defs().len() as u16)
            .filter(|&k| {
                let d = self.creature_kinds.get(k).expect("present");
                d.min_world_tier <= world_tier && d.behaviour != Behaviour::Flee
            })
            .collect();
        if kinds.is_empty() {
            return;
        }
        let mut ids = Vec::new();
        for n in 0..rules::wave_size(level) {
            let kind = kinds[(roll(&mut self.rng) * kinds.len() as f64) as usize % kinds.len()];
            let def = self.creature_kinds.get(kind).expect("present");
            let range = bm_core::creatures::tiers_on(world_tier);
            let range = *range.start()..=(*range.end()).min(def.top_tier());
            let tier = rules::wave_tier(range, last, roll(&mut self.rng));
            let portal = portals[n as usize % rules::PORTALS];
            let a = roll(&mut self.rng) * std::f64::consts::TAU;
            let feet = portal + DVec3::new(a.sin() * DROP_SPREAD * roll(&mut self.rng), 0.0, a.cos() * DROP_SPREAD);
            let rng = &mut self.rng;
            let exotic = bm_core::creatures::roll_exotic(world_tier, || roll(rng));
            let cid = self.add_creature(pi, kind, tier, feet, exotic);
            if let Some(c) = self.herds[pi].get_mut(cid) {
                c.meteor = Some(self.skies[pi].meteors[i].id);
            }
            ids.push(cid);
        }
        self.skies[pi].meteors[i].creatures = ids;
        self.point_wave(pi, i);
    }

    /// Points every creature of a meteor's wave at a life holder (or, before
    /// any life is given, the nearest player in the zone).
    fn point_wave(&mut self, pi: usize, i: usize) {
        let m = &self.skies[pi].meteors[i];
        let shape = self.planets[pi].world.shape;
        let centre = m.centre();
        let mut targets: Vec<(u32, DVec3)> = self
            .players
            .iter()
            .filter(|(_, o)| o.planet == pi && !o.state.down)
            .filter(|(_, o)| {
                if m.lives.is_empty() {
                    shape.delta(centre, o.state.pos).length() <= rules::ZONE_RADIUS
                } else {
                    m.lives.contains_key(&o.account)
                }
            })
            .map(|(id, o)| (*id, o.state.pos))
            .collect();
        targets.sort_by_key(|t| t.0);
        if targets.is_empty() {
            return;
        }
        let tick = self.tick;
        for &cid in &m.creatures {
            let Some(c) = self.herds[pi].get_mut(cid) else { continue };
            if !c.alive() {
                continue;
            }
            let (t, _) = *targets
                .iter()
                .min_by(|a, b| {
                    shape
                        .delta(c.state.pos, a.1)
                        .length()
                        .total_cmp(&shape.delta(c.state.pos, b.1).length())
                })
                .expect("not empty");
            c.add_threat(t, 50.0);
            if !c.angry() {
                c.mind = Mind::Angry {
                    target: t,
                    fight: Fight::Close,
                };
                c.seen_at = tick;
                c.sees = true;
            }
        }
    }

    /// Drops stuck wave creatures back in through a portal.
    fn unstick_wave(&mut self, pi: usize, i: usize) {
        let tick = self.tick;
        let shape = self.planets[pi].world.shape;
        let players: Vec<DVec3> = self
            .players
            .values()
            .filter(|o| o.planet == pi && !o.state.down)
            .map(|o| o.state.pos)
            .collect();
        let m = &mut self.skies[pi].meteors[i];
        let mut drop = Vec::new();
        for &cid in &m.creatures {
            let Some(c) = self.herds[pi].get(cid).filter(|c| c.alive()) else {
                continue;
            };
            let pos = c.state.pos;
            let gap = players
                .iter()
                .map(|p| shape.delta(pos, *p).length())
                .fold(f64::MAX, f64::min);
            let entry = m.stuck.entry(cid).or_insert((gap, tick));
            if stuck_step(entry, gap, tick) {
                drop.push(cid);
            }
        }
        m.stuck.retain(|cid, _| m.creatures.contains(cid));
        for cid in drop {
            let k = (roll(&mut self.rng) * rules::PORTALS as f64) as usize % rules::PORTALS;
            let at = shape.wrap_point(self.skies[pi].meteors[i].portals[k]);
            if let Some(c) = self.herds[pi].get_mut(cid) {
                tracing::debug!("meteor creature {cid} was stuck at {:.1?}; dropped again", c.state.pos);
                c.state.pos = at;
                c.state.vel = DVec3::ZERO;
                c.state.on_ground = false;
            }
        }
    }

    /// Lives, the zone, the wave's dead, the next wave, failure, clearing.
    fn check_fight(&mut self, pi: usize, i: usize, wave: u8, next_at: Option<u64>) {
        let tick = self.tick;
        let shape = self.planets[pi].world.shape;
        let (centre, level, mid) = {
            let m = &self.skies[pi].meteors[i];
            (m.centre(), m.level, m.id)
        };
        // The wave's dead and gone.
        let herd = &self.herds[pi];
        let before = self.skies[pi].meteors[i].creatures.len();
        let mut died = false;
        self.skies[pi].meteors[i].creatures.retain(|&cid| match herd.get(cid) {
            Some(c) if c.alive() => true,
            Some(_) => {
                died = true;
                false
            }
            None => false,
        });
        let changed_count = before != self.skies[pi].meteors[i].creatures.len();
        if died {
            self.skies[pi].meteors[i].first_kill = true;
        }
        // Lives: given in the zone from the first kill; outside it a holder
        // has the grace to come back.
        let mut told: Vec<(u32, Life)> = Vec::new();
        let mut lives_changed = false;
        {
            let m = &mut self.skies[pi].meteors[i];
            for (pid, o) in self.players.iter().filter(|(_, o)| o.planet == pi) {
                let inside = shape.delta(centre, o.state.pos).length() <= rules::ZONE_RADIUS;
                if inside && !o.state.down {
                    m.entitled.insert(o.account.clone());
                }
                match m.lives.get_mut(&o.account) {
                    Some(out) => {
                        if inside && out.is_some() {
                            *out = None;
                            told.push((*pid, Some((mid, None))));
                        } else if !inside {
                            let since = *out.get_or_insert(tick);
                            let left = (since + rules::LIFE_GRACE_TICKS).saturating_sub(tick);
                            if left == 0 {
                                m.lives.remove(&o.account);
                                told.push((*pid, None));
                                lives_changed = true;
                            } else if tick % 60 < CHECK_EVERY {
                                told.push((*pid, Some((mid, Some(left as u32)))));
                            }
                        }
                    }
                    None if inside && m.first_kill && !o.state.down => {
                        m.lives.insert(o.account.clone(), None);
                        told.push((*pid, Some((mid, None))));
                        lives_changed = true;
                    }
                    None => {}
                }
            }
            // A holder who left the planet or the game loses it.
            let here: HashSet<&String> = self
                .players
                .values()
                .filter(|o| o.planet == pi)
                .map(|o| &o.account)
                .collect();
            let n = m.lives.len();
            m.lives.retain(|a, _| here.contains(a));
            lives_changed |= n != m.lives.len();
        }
        for (pid, life) in told {
            if let Some(p) = self.players.get(&pid) {
                p.out.send(&ServerMsg::MeteorLife { life });
                if life.is_none() {
                    p.out.send(&ServerMsg::Notice {
                        text: "You lost your meteor life".into(),
                    });
                }
            }
        }
        let m = &self.skies[pi].meteors[i];
        // Failure: lives were given and none is left; or left too long.
        let failed = m.first_kill && m.lives.is_empty();
        if failed || tick >= m.impact + rules::DORMANT_TICKS {
            return self.go_dormant(pi, i);
        }
        if tick % RETARGET_EVERY < CHECK_EVERY {
            self.point_wave(pi, i);
        }
        self.unstick_wave(pi, i);
        let m = &mut self.skies[pi].meteors[i];
        let mut tell = changed_count || lives_changed;
        if m.creatures.is_empty() {
            match next_at {
                None if wave >= rules::waves(level) => {
                    return self.clear_meteor(pi, i);
                }
                None => {
                    m.phase = Phase::Fighting {
                        wave,
                        next_at: Some(tick + rules::WAVE_PAUSE_TICKS),
                    };
                }
                Some(at) if tick >= at => {
                    m.phase = Phase::Fighting {
                        wave: wave + 1,
                        next_at: None,
                    };
                    self.drop_wave(pi, i, wave + 1);
                    tell = true;
                }
                Some(_) => {}
            }
        }
        if tell {
            let m = &self.skies[pi].meteors[i];
            self.tell_meteor(pi, m);
        }
    }

    /// Every wave beaten: the shell breaks and the heart opens.
    fn clear_meteor(&mut self, pi: usize, i: usize) {
        let tick = self.tick;
        let m = &mut self.skies[pi].meteors[i];
        let holders: Vec<String> = m.lives.keys().cloned().collect();
        m.entitled.extend(holders);
        m.lives.clear();
        m.phase = Phase::Cleared {
            until: tick + rules::HEART_TICKS,
        };
        tracing::info!("meteor {} on {} cleared", m.id, self.planets[pi].name);
        self.skies[pi].dirty = true;
        self.lives_end(pi);
        let m = &self.skies[pi].meteors[i];
        self.tell_meteor(pi, m);
        let centre = m.centre();
        let shape = self.planets[pi].world.shape;
        for o in self.players.values().filter(|o| o.planet == pi) {
            if shape.delta(centre, o.state.pos).length() <= rules::ZONE_RADIUS * 2.0 {
                o.out.send(&ServerMsg::Notice {
                    text: "The meteorite is beaten: break it open and take your share from its heart".into(),
                });
            }
        }
    }

    /// Failed or left too long: its creatures leave, and it pays a little.
    fn go_dormant(&mut self, pi: usize, i: usize) {
        let tick = self.tick;
        let m = &mut self.skies[pi].meteors[i];
        let gone = std::mem::take(&mut m.creatures);
        m.lives.clear();
        m.phase = Phase::Dormant {
            until: tick + rules::HEART_TICKS,
        };
        tracing::info!("meteor {} on {} went dormant", m.id, self.planets[pi].name);
        self.skies[pi].dirty = true;
        self.herds[pi].list.retain(|c| !gone.contains(&c.id) || !c.alive());
        for o in self.players.values().filter(|o| o.planet == pi) {
            for &cid in &gone {
                o.out.send(&ServerMsg::CreatureGone { id: cid });
            }
        }
        for p in self.players.values_mut() {
            p.known.retain(|cid, _| !gone.contains(cid));
        }
        self.lives_end(pi);
        let m = &self.skies[pi].meteors[i];
        self.tell_meteor(pi, m);
    }

    /// Tells everyone on planet `pi` holding no life any more so.
    fn lives_end(&self, pi: usize) {
        let held: HashSet<&String> = self.skies[pi].meteors.iter().flat_map(|m| m.lives.keys()).collect();
        for o in self
            .players
            .values()
            .filter(|o| o.planet == pi && !held.contains(&o.account))
        {
            o.out.send(&ServerMsg::MeteorLife { life: None });
        }
    }

    /// The heart closes: it turns to shell and the meteor is forgotten.
    fn meteor_closed(&mut self, pi: usize, m: Meteor) {
        if self.planets[pi]
            .world
            .state(m.site)
            .is_some_and(|s| s.id == self.meteor_heart_id)
        {
            self.change_block(pi, m.site, BlockState::plain(self.meteor_shell_id), None);
        }
        for o in self.players.values().filter(|o| o.planet == pi) {
            o.out.send(&ServerMsg::MeteorGone { id: m.id });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sky() -> Sky {
        let shape = PlanetShape::new(64, 8);
        Sky::new(
            "Wildmere",
            "lush",
            shape,
            &RegionNames::builtin(),
            |x, _| if x < 1024 { LandClass::Plain } else { LandClass::Sea },
            None,
            0,
        )
    }

    /// A creature pacing in a pit under its target moves all the time but
    /// never gets closer: it is stuck (the meteor test once waited forever
    /// on one 25 blocks under the player). One that closes in is not.
    #[test]
    fn a_creature_pacing_in_a_pit_counts_as_stuck() {
        let mut pit = (25.0, 0);
        let mut dropped = false;
        for t in 1..=STUCK_TICKS {
            let gap = if t % 2 == 0 { 25.0 } else { 24.4 };
            dropped |= stuck_step(&mut pit, gap, t);
        }
        assert!(dropped, "pacing is not progress");
        let mut coming = (25.0, 0);
        for t in 1..=STUCK_TICKS * 2 {
            let gap = 25.0 - t as f64 * 0.02;
            assert!(!stuck_step(&mut coming, gap, t), "closing in at {t}");
        }
    }

    #[test]
    fn a_sky_names_every_region_and_survives_saving_as_dormant() {
        let mut s = sky();
        assert_eq!(s.names.len(), s.regions.len());
        assert_eq!(s.rest_until.len(), s.regions.len());
        s.rest_until[3] = 999;
        s.squares.insert((1, 2), 500);
        s.meteors.push(Meteor {
            id: 7,
            site: BlockPos::new(10, 140, 20),
            from: DVec3::ZERO,
            level: 2,
            phase: Phase::Fighting { wave: 1, next_at: None },
            impact: 0,
            beam_until: 0,
            portals: [DVec3::ZERO; 3],
            creatures: vec![1, 2],
            first_kill: true,
            lives: HashMap::from([("ann".to_string(), None)]),
            entitled: HashSet::from(["ann".to_string()]),
            looted: HashSet::new(),
            shell: vec![BlockPos::new(11, 140, 20)],
            broken: 1,
            stuck: HashMap::new(),
        });
        let saved = s.saved();
        let shape = PlanetShape::new(64, 8);
        let land = |x: i64, _| if x < 1024 { LandClass::Plain } else { LandClass::Sea };
        let back = Sky::new(
            "Wildmere",
            "lush",
            shape,
            &RegionNames::builtin(),
            land,
            Some(saved),
            100,
        );
        assert_eq!(back.names, s.names, "the same planet names its regions the same way");
        assert_eq!(back.rest_until[3], 999);
        assert_eq!(back.squares.get(&(1, 2)), Some(&500));
        let m = &back.meteors[0];
        assert!(m.dormant(), "a fight cut by a restart wakes dormant");
        assert!(m.creatures.is_empty() && m.lives.is_empty());
        assert_eq!(m.shell, vec![BlockPos::new(11, 140, 20)]);
        assert!(back.meteor_at(BlockPos::new(11, 140, 20)).is_some());
        assert!(back.meteor_at(BlockPos::new(10, 140, 20)).is_some(), "the heart");
    }
}
