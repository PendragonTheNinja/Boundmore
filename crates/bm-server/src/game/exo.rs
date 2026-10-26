//! Wandering worlds in the game (milestone 21, ADR-0011): arrivals, the
//! warnings before one leaves, its air climbing in its last stretch, and
//! leaving (its players carried to the host, its rows deleted, the slot's
//! next wanderer made and kept hidden until it arrives). The schedule and
//! what a wanderer is are `crate::exo`'s.

use super::*;
use crate::exo::{FIRST_EXO_ID, HOUR};

/// Wanderers are looked over this often (ticks).
pub(super) const EXO_EVERY: u64 = 60;

/// What a player is told who tries to claim on a wanderer.
pub(super) const NO_CLAIM: &str = "Nothing can be claimed on a wandering world: it will leave, and all on it";

/// When players on a wanderer are told it is leaving, before it goes.
const WARNINGS: [u64; 4] = [24 * HOUR, HOUR, 30 * 60 * 60, 5 * 60 * 60];

impl Game {
    /// The list index of planet `id` (permanent planets keep their config
    /// index as their id; wanderers live in the slots after them).
    pub(super) fn index_of(&self, id: u16) -> Option<usize> {
        if id < FIRST_EXO_ID {
            let i = id as usize;
            return (i < self.permanent).then_some(i);
        }
        self.planets[self.permanent..]
            .iter()
            .position(|p| p.id == id)
            .map(|i| i + self.permanent)
    }

    /// Where a player saved on planet `id` comes back: the planet if it is
    /// here, else the host of the wanderer it was (told why), else nowhere.
    pub(super) fn returning_to(&self, id: u16) -> Option<(usize, Option<String>)> {
        if let Some(i) = self.index_of(id)
            && self.planets[i].present(self.tick)
        {
            return Some((i, None));
        }
        let host = self.store.exo_host(id).unwrap_or_else(|e| {
            tracing::error!("reading departed wandering world {id}: {e:#}");
            None
        })?;
        let hi = self.index_of(host)?;
        Some((
            hi,
            Some(format!(
                "The wandering world you were on has left the sky; you are back on {}",
                self.planets[hi].name
            )),
        ))
    }

    /// Whether this player may warp to planet index `to` from where they
    /// stand: a wanderer only from its host. Says why not.
    pub(super) fn exo_reachable(&self, id: u32, to: usize) -> bool {
        let Some(e) = &self.planets[to].exo else { return true };
        let Some(p) = self.players.get(&id) else { return false };
        if self.planets[p.planet].id == e.host {
            return true;
        }
        let host = self
            .index_of(e.host)
            .map_or("its host", |h| self.planets[h].name.as_str());
        let text = format!("{} is reached only from {host}'s warp shrine", e.name);
        self.notice(id, &text);
        false
    }

    /// Looks over the wanderers (every [`EXO_EVERY`] ticks).
    pub(super) fn step_exos(&mut self) {
        if self.tick % EXO_EVERY != 0 {
            return;
        }
        let (from, now) = (self.exo_checked, self.tick);
        self.exo_checked = now;
        let rules = self.cfg.exo_rules();
        for i in self.permanent..self.planets.len() {
            let Some(rec) = self.planets[i].exo.clone() else {
                continue;
            };
            let crossed = |t: u64| t > from && t <= now;
            if crossed(rec.arrives) {
                self.exo_arrived(i);
            }
            if now >= rec.arrives && now < rec.leaves {
                for before in WARNINGS {
                    if rec.leaves - rec.arrives > before && crossed(rec.leaves - before) {
                        let host = self.host_name(&rec);
                        self.server_line(i, leaving_text(&rec.name, &host, before));
                    }
                }
            }
            let air = rec.air_at(now, &rules);
            if air != self.planets[i].air {
                self.set_planet_air(i, air);
            }
            if now >= rec.leaves {
                self.exo_leaves(i);
            }
        }
    }

    fn host_name(&self, rec: &crate::exo::ExoRecord) -> String {
        self.index_of(rec.host)
            .map_or_else(|| "its host".to_string(), |h| self.planets[h].name.clone())
    }

    /// A wanderer has come: everyone is told, and the planet list goes out.
    fn exo_arrived(&mut self, i: usize) {
        let p = &self.planets[i];
        let Some(rec) = &p.exo else { return };
        let host = self.host_name(rec);
        let text = format!(
            "A wandering world, {}, has come into {host}'s sky: {}, tier {}. Reach it from {host}'s warp shrine.",
            rec.name, p.kind_display, rec.tier
        );
        tracing::info!("wandering world {} ({}) arrived by {host}", rec.name, rec.id);
        self.universe_line(text);
        self.universe_dirty = true;
    }

    /// A line from the server to everyone online.
    fn universe_line(&self, text: String) {
        let line = ChatLine {
            channel: Channel::Universe,
            from: String::new(),
            text,
            time: unix_now(),
        };
        self.send_line(line, |_| true);
    }

    /// A planet's air changes: everyone on it breathes it from right after
    /// their latest input, as the server and their prediction both apply it.
    fn set_planet_air(&mut self, i: usize, air: bm_core::atmosphere::Air) {
        self.planets[i].air = air;
        for p in self.players.values_mut().filter(|p| p.planet == i) {
            p.state.air = air;
            p.out.send(&ServerMsg::Air { at: p.last_seq, air });
        }
    }

    /// A wanderer leaves: its players are carried to its host, players who
    /// left from it will come back there, its rows are deleted at the next
    /// save, and its slot's next wanderer is made (hidden until it arrives).
    fn exo_leaves(&mut self, i: usize) {
        let Some(rec) = self.planets[i].exo.clone() else { return };
        let host = self.index_of(rec.host).unwrap_or(self.start);
        let host_name = self.planets[host].name.clone();
        let here: Vec<u32> = self
            .players
            .iter()
            .filter(|(_, p)| p.planet == i)
            .map(|(id, _)| *id)
            .collect();
        for id in here {
            self.travel(id, host, None);
            let text = format!("{} has left the sky. You were carried to {host_name}.", rec.name);
            self.notice(id, &text);
        }
        let arrival = self.planets[host].arrival();
        for d in self.departed.values_mut().filter(|d| d.planet == i) {
            d.planet = host;
            d.state = PlayerState {
                pos: arrival.pos,
                vel: DVec3::ZERO,
                on_ground: true,
                hook: None,
                air: self.planets[host].air,
                ..d.state
            };
            // Saved again, now on the host.
            d.written = None;
        }
        self.shots.retain(|s| s.planet != i);
        self.forget_planets.push((rec.id, rec.host));
        tracing::info!("wandering world {} ({}) left the sky", rec.name, rec.id);
        self.universe_line(format!("{} has left the sky.", rec.name));
        self.universe_dirty = true;
        let rules = self.cfg.exo_rules();
        let next = self.next_exo_planet(&rec, &rules);
        let planet = match next {
            Ok(p) => p,
            Err(e) => {
                // Hidden for good until a restart makes the slot's saved
                // record (the store already has it) or a new one.
                tracing::error!("making the next wandering world for slot {}: {e:#}", rec.slot);
                let mut gone = self.planets[i].exo.take().expect("a wanderer");
                gone.arrives = u64::MAX;
                gone.leaves = u64::MAX;
                self.planets[i].exo = Some(gone);
                return;
            }
        };
        self.install_planet(i, planet);
    }

    /// Makes the slot's next wanderer after `prev`.
    fn next_exo_planet(&self, prev: &crate::exo::ExoRecord, rules: &crate::exo::ExoRules) -> Result<Planet> {
        let hosts: Vec<(u16, u8)> = self.planets[..self.permanent].iter().map(|p| (p.id, p.tier)).collect();
        let names: Vec<&str> = self.planets.iter().map(|p| p.name.as_str()).collect();
        let salt = crate::planet::exo_salt(&self.store)?;
        let rec = crate::planet::next_exo(
            &self.store,
            prev.slot,
            rules.next_arrival(prev),
            rules,
            &self.kinds,
            &hosts,
            &names,
            salt,
        )?;
        let registry = self.planets[0].world.registry.clone();
        let cat = crate::planet::Catalogue {
            registry: &registry,
            items: &self.items,
            recipes: &self.recipes,
            kinds: &self.kinds,
            biomes: self.cfg.biomes,
        };
        crate::planet::open_exo(
            &self.store,
            &rec,
            rules,
            self.cfg.height_chunks,
            self.cfg.shrine_guard,
            &cat,
            &self.colours,
        )
    }

    /// Puts a new planet in list slot `i`, with fresh creatures, damage and
    /// sky. Nobody may be on the old one.
    fn install_planet(&mut self, i: usize, mut planet: Planet) {
        planet.farm = crate::farm::Farm::load(self.store.crops(planet.id).unwrap_or_default(), &self.crops);
        planet.stations.forge_seconds = self.cfg.forge_queue_seconds;
        if let Some(wear) = self.cfg.machine_wear_start {
            planet.stations.start_wear = wear.min(bm_core::spark::MACHINE_WEAR);
        }
        let generator = planet.world.generator().clone();
        let region_names = bm_core::regions::RegionNames::builtin();
        self.skies[i] = meteors::Sky::new(
            &planet.name,
            &planet.kind,
            planet.world.shape,
            &region_names,
            |x, z| meteors::land_of(&generator, x, z),
            None,
            self.tick,
        );
        self.damage[i] = DamageMap::default();
        self.herds[i] = creatures::Herds::default();
        self.planets[i] = planet;
    }
}

/// What players on a wanderer read `before` ticks before it leaves.
fn leaving_text(name: &str, host: &str, before: u64) -> String {
    let lost = "Anything left on it will be lost.";
    match before {
        b if b >= 24 * HOUR => format!("{name} leaves the sky in a day. {lost}"),
        b if b >= HOUR => format!("{name} leaves the sky in an hour. {lost}"),
        b if b >= 30 * 60 * 60 => format!(
            "{name} is leaving: its air thickens from now on. Leave by its warp shrine, \
             or you will be carried to {host} when it goes in half an hour. {lost}"
        ),
        _ => format!("{name} leaves the sky in 5 minutes. {lost}"),
    }
}
