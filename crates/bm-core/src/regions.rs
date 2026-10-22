//! Named regions (milestone 14, `docs/milestones/14-meteorites.md`).
//!
//! A planet is divided into regions: the cells of a jittered grid, about
//! [`SPACING`] blocks apart, wrapping with the planet. The layout comes from a
//! seed made from the planet's name and its size, never its terrain seed, so
//! client and server can both know it (ADR-0003 guards terrain). Names are
//! made on the server from the land at each centre ([`LandClass`], judged by
//! worldgen) and sent to the client.
//!
//! The HUD says a region's name near its centre and "North of ..." farther
//! out, as Boundless did; reaching a centre is what brings a meteorite.

use std::collections::{BTreeMap, HashSet};

use serde::Deserialize;

use crate::planet::PlanetShape;

/// Distance between region centres, in blocks. Boundless's 3 km worlds
/// (3072 blocks) had 32 to 34 regions and its 4 km worlds up to 64: a grid
/// about 512 blocks square (world announcements on the forum, tuning
/// 3.2.5). A 2048-block planet has 16.
pub const SPACING: i64 = 512;

/// How far the jitter moves a centre from its grid cell's middle, as a share
/// of the cell.
const JITTER: f64 = 0.3;

/// Within this many blocks of a centre the HUD gives the region's name
/// alone, and a meteorite may be called (**guess**).
pub const CENTRE_RADIUS: f64 = 40.0;

/// Discovery XP: the n-th region discovered on a planet gives `50 x n`
/// (Boundless, R218/219, tuning 5.5).
pub fn discovery_xp(nth: u32) -> u64 {
    50 * nth as u64
}

/// The layout seed for a planet: FNV-1a of its name, mixed with its width.
pub fn layout_seed(planet_name: &str, shape: PlanetShape) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in planet_name.bytes().chain(shape.width_chunks.to_le_bytes()) {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn mix(mut x: u64) -> u64 {
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

fn unit(h: u64) -> f64 {
    (h >> 11) as f64 / (1u64 << 53) as f64
}

/// Shortest signed difference on a loop of length `w`.
fn shortest(d: f64, w: f64) -> f64 {
    let d = d.rem_euclid(w);
    if d > w * 0.5 { d - w } else { d }
}

/// A planet's regions: where each centre is and which region holds a point.
#[derive(Clone, Debug, PartialEq)]
pub struct RegionMap {
    width: f64,
    /// Grid cells per side.
    cells: u32,
    /// Centre of each region (x, z), canonical, by index `cz * cells + cx`.
    centres: Vec<[f64; 2]>,
}

impl RegionMap {
    pub fn new(shape: PlanetShape, seed: u64) -> Self {
        let width = shape.width_blocks() as f64;
        let cells = ((width / SPACING as f64).round() as u32).max(1);
        let size = width / cells as f64;
        let mut centres = Vec::with_capacity((cells * cells) as usize);
        for cz in 0..cells {
            for cx in 0..cells {
                let h = mix(seed ^ mix(((cz as u64) << 32) | cx as u64));
                let jx = (unit(h) - 0.5) * 2.0 * JITTER;
                let jz = (unit(mix(h)) - 0.5) * 2.0 * JITTER;
                let x = (cx as f64 + 0.5 + jx) * size;
                let z = (cz as f64 + 0.5 + jz) * size;
                centres.push([x.rem_euclid(width), z.rem_euclid(width)]);
            }
        }
        Self { width, cells, centres }
    }

    pub fn len(&self) -> usize {
        self.centres.len()
    }

    pub fn is_empty(&self) -> bool {
        self.centres.is_empty()
    }

    /// Centre of region `i` (x, z).
    pub fn centre(&self, i: usize) -> [f64; 2] {
        self.centres[i]
    }

    /// Horizontal offset from region `i`'s centre to (x, z), across the seam.
    pub fn offset(&self, i: usize, x: f64, z: f64) -> [f64; 2] {
        let [cx, cz] = self.centres[i];
        [shortest(x - cx, self.width), shortest(z - cz, self.width)]
    }

    /// Distance from region `i`'s centre to (x, z).
    pub fn distance(&self, i: usize, x: f64, z: f64) -> f64 {
        let [dx, dz] = self.offset(i, x, z);
        dx.hypot(dz)
    }

    /// The region holding (x, z): the nearest centre. A centre never leaves
    /// its own cell's neighbourhood, so the 3 x 3 cells around the point
    /// hold the answer.
    pub fn locate(&self, x: f64, z: f64) -> usize {
        let n = self.cells as i64;
        let size = self.width / n as f64;
        let gx = (x.rem_euclid(self.width) / size).floor() as i64;
        let gz = (z.rem_euclid(self.width) / size).floor() as i64;
        let mut best = (f64::MAX, 0);
        for dz in -1..=1 {
            for dx in -1..=1 {
                let i = ((gz + dz).rem_euclid(n) * n + (gx + dx).rem_euclid(n)) as usize;
                let d = self.distance(i, x, z);
                if d < best.0 {
                    best = (d, i);
                }
            }
        }
        best.1
    }

    /// Where (x, z) is, as the HUD says it.
    pub fn place(&self, x: f64, z: f64) -> Place {
        let region = self.locate(x, z);
        let [dx, dz] = self.offset(region, x, z);
        let at_centre = dx.hypot(dz) <= CENTRE_RADIUS;
        Place {
            region,
            direction: (!at_centre).then(|| Direction::of(dx, dz)),
        }
    }
}

/// A point's region, and which way it lies from the centre (None near it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Place {
    pub region: usize,
    pub direction: Option<Direction>,
}

impl Place {
    /// "Amber Hollows" or "North of Amber Hollows".
    pub fn describe(&self, name: &str) -> String {
        match self.direction {
            None => name.to_string(),
            Some(d) => format!("{} of {name}", d.word()),
        }
    }
}

/// Eight compass directions; north is -z, east is +x (as `Mount`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    North,
    Northeast,
    East,
    Southeast,
    South,
    Southwest,
    West,
    Northwest,
}

impl Direction {
    /// The direction of offset (dx, dz) from a centre.
    pub fn of(dx: f64, dz: f64) -> Self {
        // Angle from north, clockwise (towards east).
        let a = dx.atan2(-dz).rem_euclid(std::f64::consts::TAU);
        let k = ((a / (std::f64::consts::TAU / 8.0)).round() as usize) % 8;
        [
            Self::North,
            Self::Northeast,
            Self::East,
            Self::Southeast,
            Self::South,
            Self::Southwest,
            Self::West,
            Self::Northwest,
        ][k]
    }

    pub fn word(self) -> &'static str {
        match self {
            Self::North => "North",
            Self::Northeast => "Northeast",
            Self::East => "East",
            Self::Southeast => "Southeast",
            Self::South => "South",
            Self::Southwest => "Southwest",
            Self::West => "West",
            Self::Northwest => "Northwest",
        }
    }
}

/// The kind of land at a region's centre, which picks its name's words.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LandClass {
    Sea,
    Shore,
    Plain,
    Hills,
    Mountain,
    Snow,
}

impl LandClass {
    /// Judges a column by its surface height against the planet's water
    /// level and snow line (heights in blocks).
    pub fn of(surface: i64, water_level: i64, snow_line: i64) -> Self {
        if surface < water_level {
            Self::Sea
        } else if surface >= snow_line {
            Self::Snow
        } else if surface <= water_level + 3 {
            Self::Shore
        } else if surface >= water_level + 45 {
            Self::Mountain
        } else if surface >= water_level + 18 {
            Self::Hills
        } else {
            Self::Plain
        }
    }

    fn key(self) -> &'static str {
        match self {
            Self::Sea => "sea",
            Self::Shore => "shore",
            Self::Plain => "plain",
            Self::Hills => "hills",
            Self::Mountain => "mountain",
            Self::Snow => "snow",
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
struct Words {
    #[serde(default)]
    first: Vec<String>,
    #[serde(default)]
    second: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct NamesFile {
    land: BTreeMap<String, Words>,
    #[serde(default)]
    kind: BTreeMap<String, Words>,
}

/// The word lists region names are made from (`assets/regions.toml`).
#[derive(Clone, Debug)]
pub struct RegionNames {
    land: BTreeMap<String, Words>,
    kind: BTreeMap<String, Words>,
}

impl RegionNames {
    pub fn from_toml(source: &str) -> Result<Self, String> {
        let f: NamesFile = toml::from_str(source).map_err(|e| e.to_string())?;
        for class in [
            LandClass::Sea,
            LandClass::Shore,
            LandClass::Plain,
            LandClass::Hills,
            LandClass::Mountain,
            LandClass::Snow,
        ] {
            let w = f
                .land
                .get(class.key())
                .ok_or(format!("no words for land `{}`", class.key()))?;
            if w.first.is_empty() || w.second.is_empty() {
                return Err(format!("land `{}` needs first and second words", class.key()));
            }
        }
        Ok(Self {
            land: f.land,
            kind: f.kind,
        })
    }

    pub fn builtin() -> Self {
        Self::from_toml(include_str!("../../../assets/regions.toml")).expect("assets/regions.toml is valid")
    }

    /// Names every region of a planet of `kind`, from the land at each
    /// centre (`lands`, by region index), deterministic for `seed`, never
    /// repeating a name on the planet.
    pub fn name_all(&self, kind: &str, lands: &[LandClass], seed: u64) -> Vec<String> {
        let extra = self.kind.get(kind).map(|w| w.first.as_slice()).unwrap_or(&[]);
        let mut used = HashSet::new();
        let mut names = Vec::with_capacity(lands.len());
        for (i, class) in lands.iter().enumerate() {
            let w = &self.land[class.key()];
            let firsts: Vec<&String> = w.first.iter().chain(extra).collect();
            let mut h = mix(seed ^ mix(i as u64 + 1));
            let mut name = String::new();
            // Try fresh pairs; with ~60 pairs a class and ~50 regions a
            // planet this almost never needs the numbered fallback.
            for _ in 0..64 {
                let a = firsts[(h % firsts.len() as u64) as usize];
                h = mix(h);
                let b = &w.second[(h % w.second.len() as u64) as usize];
                h = mix(h);
                let candidate = format!("{a} {b}");
                if !used.contains(&candidate) {
                    name = candidate;
                    break;
                }
            }
            if name.is_empty() {
                let base = format!("{} {}", firsts[0], w.second[0]);
                let mut k = 2;
                name = loop {
                    let c = format!("{base} {}", roman(k));
                    if !used.contains(&c) {
                        break c;
                    }
                    k += 1;
                };
            }
            used.insert(name.clone());
            names.push(name);
        }
        names
    }
}

fn roman(n: u32) -> String {
    let table = [(10, "X"), (9, "IX"), (5, "V"), (4, "IV"), (1, "I")];
    let mut n = n;
    let mut s = String::new();
    for (v, r) in table {
        while n >= v {
            s.push_str(r);
            n -= v;
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map() -> RegionMap {
        let shape = PlanetShape::new(64, 8);
        RegionMap::new(shape, layout_seed("Hearthmoor", shape))
    }

    #[test]
    fn a_2048_planet_has_sixteen_regions_inside_it() {
        let m = map();
        assert_eq!(m.len(), 16, "Boundless's 512-block spacing");
        let big = PlanetShape::new(128, 8);
        assert_eq!(
            RegionMap::new(big, 1).len(),
            64,
            "a 4 km world: 64, as Boundless's large worlds"
        );
        for i in 0..m.len() {
            let [x, z] = m.centre(i);
            assert!((0.0..2048.0).contains(&x) && (0.0..2048.0).contains(&z));
        }
        assert_eq!(m, map(), "the layout is deterministic");
        let other = PlanetShape::new(64, 8);
        assert_ne!(
            m,
            RegionMap::new(other, layout_seed("Wildmere", other)),
            "planets differ"
        );
    }

    #[test]
    fn locate_finds_the_nearest_centre_across_the_seam() {
        let m = map();
        let mut seed = 7u64;
        for _ in 0..2000 {
            seed = mix(seed);
            let (x, z) = (unit(seed) * 2048.0, unit(mix(seed)) * 2048.0);
            let brute = (0..m.len())
                .min_by(|&a, &b| m.distance(a, x, z).total_cmp(&m.distance(b, x, z)))
                .unwrap();
            assert_eq!(m.locate(x, z), brute, "at ({x}, {z})");
        }
        // Either side of the seam is the same place.
        assert_eq!(m.locate(-1.0, 5.0), m.locate(2047.0, 5.0));
    }

    #[test]
    fn the_hud_names_the_centre_and_points_from_it() {
        let m = map();
        let i = 10;
        let [cx, cz] = m.centre(i);
        assert_eq!(m.place(cx + 5.0, cz).direction, None);
        assert_eq!(m.place(cx, cz).describe("Amber Hollows"), "Amber Hollows");
        let north = m.place(cx, cz - 80.0);
        assert_eq!(north.region, i);
        assert_eq!(north.describe("Amber Hollows"), "North of Amber Hollows");
        assert_eq!(m.place(cx + 70.0, cz + 70.0).direction, Some(Direction::Southeast));
        assert_eq!(Direction::of(-1.0, 0.0), Direction::West);
    }

    #[test]
    fn discovery_xp_grows_by_fifty_a_region() {
        assert_eq!(discovery_xp(1), 50);
        assert_eq!(discovery_xp(3), 150);
        assert_eq!(
            (1..=50).map(discovery_xp).sum::<u64>(),
            63_750,
            "Boundless's full world"
        );
    }

    #[test]
    fn land_is_judged_by_height() {
        assert_eq!(LandClass::of(120, 128, 240), LandClass::Sea);
        assert_eq!(LandClass::of(130, 128, 240), LandClass::Shore);
        assert_eq!(LandClass::of(140, 128, 240), LandClass::Plain);
        assert_eq!(LandClass::of(150, 128, 240), LandClass::Hills);
        assert_eq!(LandClass::of(180, 128, 240), LandClass::Mountain);
        assert_eq!(LandClass::of(250, 128, 240), LandClass::Snow);
    }

    #[test]
    fn names_are_unique_deterministic_and_fit_their_land() {
        let names = RegionNames::builtin();
        let lands: Vec<LandClass> = (0..16).map(|i| [LandClass::Plain, LandClass::Sea][i % 2]).collect();
        let a = names.name_all("lush", &lands, 42);
        assert_eq!(a, names.name_all("lush", &lands, 42));
        assert_ne!(a, names.name_all("lush", &lands, 43));
        let set: HashSet<&String> = a.iter().collect();
        assert_eq!(set.len(), a.len(), "no repeats: {a:?}");
        let seas = ["Reach", "Shallows", "Expanse", "Sound", "Gulf", "Waters", "Mere", "Bay"];
        assert!(seas.iter().any(|s| a[1].ends_with(s)), "{} is a sea name", a[1]);
        // Even with far more regions than pairs, names stay unique.
        let many = vec![LandClass::Shore; 200];
        let b = names.name_all("temperate", &many, 1);
        assert_eq!(b.iter().collect::<HashSet<_>>().len(), 200);
    }
}
