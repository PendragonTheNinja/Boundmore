//! Crafting: instant hand recipes and timed jobs queued at stations.
//!
//! A station job takes its inputs the moment it is queued, then finishes one
//! craft every `seconds` of planet time, server side, whether or not anyone
//! watches. Times are planet ticks (`started`), and the planet clock is saved
//! with every station in the same batch, so a job continues across restarts
//! exactly where it stopped. Finished items wait in the station's output
//! (a few stacks); when the next craft would not fit, the queue pauses until
//! someone collects.
//!
//! A furnace (milestone 11) also has a fuel slot and a bank of heat: each
//! craft draws its recipe's heat as it starts, burning fuel items from the
//! slot as needed; without enough, the craft waits, with no progress, until
//! fuel arrives.
//!
//! Spark (milestone 15): a spark generator burns its fuel slot into spark,
//! one item at a time over the item's burn time, into its group's store (a
//! group's record is kept at its first block); burning pauses while the
//! store is full. A recipe with spark starts only when the generators its
//! machine's links reach can pay it all, fullest first; short of it, the
//! craft waits (retried every [`RETRY_EVERY`] ticks). Where the links go is
//! the world's business: [`Surroundings`] answers for it.
//!
//! Wear (milestone 15): machines that wear (`spark::wears`) lose each
//! recipe's `wear` as a craft starts, and wait at 0 until a spanner repairs
//! them ([`Stations::repair`]).
//!
//! A station has data only while it holds jobs, output, fuel, banked heat,
//! spark or wear; jobs, output and fuel stop it being broken.
//!
//! Hand recipes are instant wherever they are asked for, a crafting table
//! included (Boundless queued them at a table; instant keeps the hand and the
//! table the same and costs nothing).

use crate::forging::{self, ForgeData, Forging, Sealed};
use crate::machines::Coil;
use crate::store::{SavedJob, SavedStack, SavedStation, load_slots, save_slots};
use bm_core::forge::QueueBand;
use bm_core::recipes::Craft;
use bm_core::spark::{self, MACHINE_WEAR};
use bm_core::{BlockPos, Colour, Inventory, ItemId, ItemRegistry, ItemStack, Recipe, RecipeBook, Station, TICK_HZ};
use bm_protocol::{JobView, StationView};
use std::collections::BTreeMap;
use std::collections::{BTreeSet, HashMap, VecDeque};

/// Stacks of finished items a station holds.
pub const OUTPUT_STACKS: usize = 10;
/// Jobs one station queues at most.
pub const MAX_JOBS: usize = 8;
/// Crafts one request may ask for.
pub const MAX_CRAFT_COUNT: u16 = 100;
/// Crafts waiting for spark try again this often (ticks).
pub const RETRY_EVERY: u64 = 30;

/// Said by a machine whose links reach no generator.
pub const NO_SPARK: &str = "No spark: link this machine to a spark generator";
/// Said by a worn-out machine.
pub const WORN_OUT: &str = "Worn out: repair it with a spanner";

/// What a station's world around it says, which the station records cannot:
/// where its spark links lead and what its furnace base is. Each answer is
/// None when the blocks it needs are not resident; the station then uses
/// what it last heard.
pub trait Surroundings {
    /// The keys of the generator groups the machine keyed at `machine`
    /// reaches through links (empty: none).
    fn generators(&self, machine: BlockPos) -> Option<Vec<BlockPos>>;
    /// The share of a recipe's heat the furnace keyed at `furnace` burns.
    fn heat_share(&self, furnace: BlockPos) -> Option<f32>;
    /// The power coils serving the machine keyed at `machine` (milestone
    /// 18; empty: none).
    fn coils(&self, _machine: BlockPos) -> Option<Vec<Coil>> {
        Some(Vec::new())
    }
}

/// No world: no links, stone furnaces (tests, and stations nobody built).
pub struct Nowhere;

impl Surroundings for Nowhere {
    fn generators(&self, _: BlockPos) -> Option<Vec<BlockPos>> {
        Some(Vec::new())
    }
    fn heat_share(&self, _: BlockPos) -> Option<f32> {
        Some(1.0)
    }
}

/// Why a station's next craft has not started.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wait {
    /// A furnace without the heat; starts when fuel arrives.
    Heat,
    /// Its links reach no generator.
    NoGrid,
    /// Its generators hold less than the craft's spark.
    Spark(u32),
    /// Worn out: waits for a spanner.
    Worn,
    /// Its coils give less power than the recipe needs (milestone 18).
    Power(u32),
}

impl Wait {
    /// In plain words, for the machine's window (None for heat, which the
    /// furnace window shows by itself).
    pub fn text(self) -> Option<String> {
        match self {
            Wait::Heat => None,
            Wait::NoGrid => Some(NO_SPARK.into()),
            Wait::Spark(n) => Some(format!("Not enough spark (needs {n})")),
            Wait::Worn => Some(WORN_OUT.into()),
            Wait::Power(n) => Some(format!("Needs {n} power: add power coils")),
        }
    }
}

/// Ticks a fuel item worth `spark` burns for in a generator (at least one).
fn burn_ticks(spark: u32) -> u32 {
    let s = spark.min(u16::MAX as u32) as u16;
    ((spark::burn_seconds(s) as f64 * TICK_HZ as f64).round() as u32).max(1)
}

/// Ticks one craft of `recipe` takes (at least one).
pub fn craft_ticks(recipe: &Recipe) -> u64 {
    ((recipe.seconds as f64 * TICK_HZ as f64).round() as u64).max(1)
}

/// Ticks one craft takes at `scale` thousandths of its base time (power,
/// milestone 18; 1,000 with none).
pub fn scaled_ticks(recipe: &Recipe, scale: u16) -> u64 {
    ((craft_ticks(recipe) as f64 * f64::from(scale.clamp(1, 1000)) / 1000.0).round() as u64).max(1)
}

/// What a queued request took: the items of each craft, in order (a
/// group's members may differ craft by craft, milestone 19), and the full
/// containers among them one by one (each with its wear: fills used), for
/// the station to give back empty (milestone 16).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Taken {
    pub crafts: Vec<Craft>,
    pub containers: Vec<ItemStack>,
    /// The colour the output takes (milestone 22): a tinted recipe's chosen
    /// colour, natural otherwise.
    pub colour: Colour,
}

impl Taken {
    /// `times` crafts of `recipe` taking each input's first item and no
    /// containers (tests, and saves from before groups).
    pub fn plain(recipe: &Recipe, times: u16) -> Self {
        let craft: Craft = recipe.inputs.iter().map(|i| (i.items[0], i.count)).collect();
        Self {
            crafts: vec![craft; times as usize],
            containers: Vec::new(),
            colour: Colour::NATURAL,
        }
    }
}

/// Whether `item` is a full container a recipe gives back empty.
fn is_container(items: &ItemRegistry, item: ItemId) -> bool {
    items.get(item).is_some_and(|d| d.returns.is_some())
}

/// Takes `times` crafts' inputs from `inv`, all or nothing: each input from
/// its items in order (`Recipe::plan`), each item from the slots
/// `Inventory::remove` takes first.
pub fn take_inputs_keeping(inv: &mut Inventory, items: &ItemRegistry, recipe: &Recipe, times: u16) -> Option<Taken> {
    take_inputs_keeping_in(inv, items, recipe, times, None)
}

/// The colour a craft of `recipe` from `inv` makes (milestone 22): for a
/// tinted recipe the one asked for, or by default the colour most crafts
/// can be made in; natural for an untinted one.
pub fn craft_colour(inv: &Inventory, recipe: &Recipe, asked: Option<Colour>) -> Option<Colour> {
    recipe.tint?;
    Some(
        asked
            .or_else(|| recipe.colour_choices(inv).first().map(|c| c.0))
            .unwrap_or(Colour::NATURAL),
    )
}

/// Removes one craft's `n` of `item` from `c`: a tinted input only in
/// `colour`, any other in whatever colour comes first.
fn remove_input(c: &mut Inventory, recipe: &Recipe, item: ItemId, n: u16, colour: Option<Colour>) -> Option<()> {
    match colour {
        Some(col) if recipe.tint.is_some() && recipe.input_of(item).map(|k| k as u8) == recipe.tint => {
            c.remove_coloured(item, col, n).map(|_| ())
        }
        _ => c.remove(item, n).map(|_| ()),
    }
}

/// [`take_inputs_keeping`] in `colour` (`craft_colour`).
pub fn take_inputs_keeping_in(
    inv: &mut Inventory,
    items: &ItemRegistry,
    recipe: &Recipe,
    times: u16,
    colour: Option<Colour>,
) -> Option<Taken> {
    let colour = craft_colour(inv, recipe, colour);
    let crafts = recipe.plan_in(inv, times, colour)?;
    let mut c = inv.clone();
    let mut containers = Vec::new();
    for craft in &crafts {
        for &(item, n) in craft {
            if is_container(items, item) {
                for _ in 0..n {
                    let slot = c.slots.iter().position(|s| s.is_some_and(|s| s.item == item))?;
                    containers.push(c.take_one(slot)?);
                }
            } else {
                remove_input(&mut c, recipe, item, n, colour)?;
            }
        }
    }
    *inv = c;
    Some(Taken {
        crafts,
        containers,
        colour: colour.unwrap_or(Colour::NATURAL),
    })
}

/// The empty containers one craft that took `craft` gives back from the
/// front of `containers` (taking them off): each one fill more worn, unless
/// that was its last.
fn returned(items: &ItemRegistry, craft: &[(ItemId, u16)], containers: &mut Vec<ItemStack>) -> Vec<ItemStack> {
    let mut out = Vec::new();
    for (item, n) in craft {
        let Some(empty) = items
            .get(*item)
            .and_then(|d| d.returns.as_ref())
            .and_then(|name| items.id(name).ok())
        else {
            continue;
        };
        let fills = match items.get(empty).map(|d| &d.kind) {
            Some(bm_core::items::ItemKind::Bucket { fills }) => *fills,
            _ => 1,
        };
        for _ in 0..*n {
            let Some(pos) = containers.iter().position(|c| c.item == *item) else {
                break;
            };
            let full = containers.remove(pos);
            let wear = full.wear + 1;
            if wear < fills {
                out.push(ItemStack {
                    item: empty,
                    count: 1,
                    wear,
                    ..full
                });
            }
        }
    }
    out
}

/// Takes `times` crafts' inputs from `inv`, all or nothing.
pub fn take_inputs(inv: &mut Inventory, recipe: &Recipe, times: u16) -> bool {
    take_inputs_in(inv, recipe, times, None).is_some()
}

/// [`take_inputs`] in `colour` (`craft_colour`); the colour made, if taken.
pub fn take_inputs_in(inv: &mut Inventory, recipe: &Recipe, times: u16, colour: Option<Colour>) -> Option<Colour> {
    let colour = craft_colour(inv, recipe, colour);
    let crafts = recipe.plan_in(inv, times, colour)?;
    let mut c = inv.clone();
    for &(item, n) in crafts.iter().flatten() {
        remove_input(&mut c, recipe, item, n, colour)?;
    }
    *inv = c;
    Some(colour.unwrap_or(Colour::NATURAL))
}

/// Why a craft was refused, worded for the player.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CraftError {
    Materials,
    NoRoom,
    QueueFull,
    TooMany,
}

impl std::fmt::Display for CraftError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CraftError::Materials => write!(f, "You do not have the materials"),
            CraftError::NoRoom => write!(f, "Your inventory is full"),
            CraftError::QueueFull => write!(f, "This station's queue is full"),
            CraftError::TooMany => write!(f, "That is too many at once"),
        }
    }
}

/// A hand recipe, `times` over, into `inv`: inputs out, outputs in, all or
/// nothing (the outputs may use the slots the inputs free).
pub fn craft_by_hand(inv: &mut Inventory, items: &ItemRegistry, recipe: &Recipe, times: u16) -> Result<(), CraftError> {
    craft_by_hand_in(inv, items, recipe, times, None)
}

/// [`craft_by_hand`] in `colour` (`craft_colour`).
pub fn craft_by_hand_in(
    inv: &mut Inventory,
    items: &ItemRegistry,
    recipe: &Recipe,
    times: u16,
    colour: Option<Colour>,
) -> Result<(), CraftError> {
    if times == 0 || times > MAX_CRAFT_COUNT {
        return Err(CraftError::TooMany);
    }
    let out = u16::try_from(recipe.output_count as u32 * times as u32).map_err(|_| CraftError::TooMany)?;
    let mut c = inv.clone();
    let Some(made) = take_inputs_in(&mut c, recipe, times, colour) else {
        return Err(CraftError::Materials);
    };
    c.add_all_coloured(items, recipe.output, made, out)
        .ok_or(CraftError::NoRoom)?;
    *inv = c;
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Job {
    pub recipe: u16,
    /// Crafts still to finish, including the one in progress.
    pub remaining: u16,
    /// The items each of those crafts took, in order (`remaining` lists).
    pub taken: VecDeque<Craft>,
    /// The full containers (buckets of water or milk, milestone 16) taken
    /// for the crafts still to finish, one item each, in order: each craft
    /// gives its own back empty, one fill more worn.
    pub containers: Vec<ItemStack>,
    /// The output's colour (milestone 22).
    pub colour: Colour,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StationData {
    pub kind: Station,
    pub jobs: VecDeque<Job>,
    /// Planet tick the head job's current craft began.
    pub started: u64,
    /// `OUTPUT_STACKS` slots.
    pub output: Inventory,
    /// A furnace's fuel slot: one stack of something that burns.
    pub fuel: Option<ItemStack>,
    /// Heat banked from fuel already burnt, drawn by each craft as it starts.
    pub heat: u32,
    /// The head job's current craft is waiting (for heat, spark or a
    /// repair): it has not started (no progress).
    pub wait: Option<Wait>,
    /// Craft XP of the finished output (milestone 12): each finished craft
    /// adds its recipe's `xp`, and whoever next collects output gets it all.
    pub xp: u32,
    /// Wear left (machines that wear; `MACHINE_WEAR` when repaired).
    pub wear: u32,
    /// A generator group's stored spark.
    pub spark: u32,
    /// A generator: the spark of the item burning (0: none) and the ticks it
    /// has burnt; it gives its spark evenly over its burn time.
    pub burn_total: u32,
    pub burn_elapsed: u32,
    /// A generator group's blocks, for its capacity.
    pub blocks: u32,
    /// A forge's session and queue (milestone 17).
    pub forge: Option<Box<ForgeData>>,
    /// Each power coil's wear left, by where it stands (milestone 18): the
    /// coils this machine has heard of. Saved.
    pub coils: BTreeMap<BlockPos, u32>,
    /// The current craft's time and spark, in thousandths of the recipe's
    /// (power cuts them; 1,000 with none). Saved, so a restart resumes at
    /// the same pace.
    pub scale: u16,
    /// What the surroundings last said: the generators a machine's links
    /// reach, a furnace's heat share (not saved; asked again).
    grid: Heard<Vec<BlockPos>>,
    heat_share: Heard<f32>,
    heard_coils: Heard<Vec<Coil>>,
}

/// Something a station last heard from its surroundings: kept for when
/// they cannot answer, never part of what the station is (equal always).
#[derive(Clone, Debug, Default)]
struct Heard<T>(Option<T>);

impl<T> PartialEq for Heard<T> {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl StationData {
    fn new(kind: Station, wear: u32) -> Self {
        Self {
            kind,
            jobs: VecDeque::new(),
            started: 0,
            output: Inventory {
                slots: vec![None; OUTPUT_STACKS],
            },
            fuel: None,
            heat: 0,
            wait: None,
            xp: 0,
            wear: if spark::wears(kind) {
                wear.min(spark::machine_wear(kind))
            } else {
                0
            },
            spark: 0,
            burn_total: 0,
            burn_elapsed: 0,
            blocks: 1,
            forge: None,
            coils: BTreeMap::new(),
            scale: 1000,
            grid: Heard(None),
            heat_share: Heard(None),
            heard_coils: Heard(None),
        }
    }

    /// Asks which coils serve it now: coils gone are forgotten, new ones
    /// start mended. Returns the power.
    pub fn hear_coils(&mut self, pos: BlockPos, env: &dyn Surroundings) -> u32 {
        if let Some(coils) = env.coils(pos) {
            self.coils.retain(|p, _| coils.iter().any(|c| c.pos == *p));
            for c in &coils {
                self.coils.entry(c.pos).or_insert(spark::COIL_WEAR);
            }
            self.heard_coils.0 = Some(coils);
        }
        self.power()
    }

    /// Power from the coils it last heard of that are not worn out.
    pub fn power(&self) -> u32 {
        self.heard_coils.0.as_ref().map_or(0, |cs| {
            cs.iter()
                .filter(|c| self.coils.get(&c.pos).is_some_and(|w| *w > 0))
                .map(|c| c.power)
                .sum()
        })
    }

    /// How many coils serve it, and how many of those are worn out.
    pub fn coil_count(&self) -> (usize, usize) {
        let worn = self.coils.values().filter(|w| **w == 0).count();
        (self.coils.len(), worn)
    }

    /// Nothing worth keeping a record for.
    fn is_empty(&self) -> bool {
        !self.holds_anything()
            && self.heat == 0
            && self.spark == 0
            && self.burn_total == 0
            && (!spark::wears(self.kind) || self.wear >= spark::machine_wear(self.kind))
            && self.coils.values().all(|w| *w >= spark::COIL_WEAR)
    }

    /// A generator group's store: the most it holds.
    pub fn capacity(&self) -> u32 {
        spark::capacity(self.blocks.max(1) as usize)
    }

    /// A generator: one tick of burning. Takes the next fuel item when the
    /// last is spent; waits while the store is full (or this tick's spark
    /// would overfill it), losing nothing. Returns whether it took an item
    /// (a change worth showing at once).
    fn burn(&mut self, items: &ItemRegistry) -> bool {
        let cap = self.capacity();
        if self.spark >= cap {
            return false;
        }
        let mut took = false;
        if self.burn_total == 0 {
            let Some(f) = &mut self.fuel else { return false };
            let value = items.get(f.item).map_or(0, |d| d.fuel);
            if value == 0 {
                return false;
            }
            f.count -= 1;
            if f.count == 0 {
                self.fuel = None;
            }
            self.burn_total = value as u32;
            self.burn_elapsed = 0;
            took = true;
        }
        let n = burn_ticks(self.burn_total);
        let done = |e: u32| (self.burn_total as u64 * e.min(n) as u64 / n as u64) as u32;
        let give = done(self.burn_elapsed + 1) - done(self.burn_elapsed);
        if self.spark + give > cap {
            return took;
        }
        self.spark += give;
        self.burn_elapsed += 1;
        if self.burn_elapsed >= n {
            self.burn_total = 0;
            self.burn_elapsed = 0;
        }
        took
    }

    /// A generator: spark still to come from the item burning.
    pub fn burn_left(&self) -> u32 {
        if self.burn_total == 0 {
            return 0;
        }
        let n = burn_ticks(self.burn_total);
        self.burn_total - (self.burn_total as u64 * self.burn_elapsed.min(n) as u64 / n as u64) as u32
    }

    /// Whether a generator has anything to burn.
    fn burns(&self) -> bool {
        self.kind == Station::SparkGenerator && (self.burn_total > 0 || self.fuel.is_some())
    }

    /// Jobs, output or fuel: what stops it being broken. Banked heat alone
    /// does not (a fire going out loses it): a furnace's leftover heat is
    /// rarely exactly what any recipe takes, so it would never come free.
    fn holds_anything(&self) -> bool {
        !self.jobs.is_empty()
            || self.output.slots.iter().any(Option::is_some)
            || self.fuel.is_some()
            || self.forge.as_ref().is_some_and(|f| !f.is_empty())
    }

    fn sealed(&self) -> Option<&Sealed> {
        self.forge.as_ref()?.queued.as_ref()
    }

    /// Draws the head job's next craft's heat from the bank (`share` of the
    /// recipe's: an iron furnace burns half), burning fuel from the slot one
    /// item at a time as needed. With not enough it keeps what it burnt
    /// banked and returns false.
    fn pay_heat(&mut self, items: &ItemRegistry, recipes: &RecipeBook, share: f32) -> bool {
        let need = self
            .jobs
            .front()
            .and_then(|j| recipes.get(j.recipe))
            .map_or(0, |r| spark::furnace_heat(r.heat, share) as u32);
        while self.heat < need {
            let Some(f) = &mut self.fuel else { break };
            let value = items.get(f.item).map_or(0, |d| d.fuel as u32);
            if value == 0 {
                break;
            }
            self.heat += value;
            f.count -= 1;
            if f.count == 0 {
                self.fuel = None;
            }
        }
        if self.heat < need {
            return false;
        }
        self.heat -= need;
        true
    }
}

pub struct Stations {
    map: HashMap<BlockPos, StationData>,
    /// Stations whose head job is running (not finished-and-waiting for
    /// room, nor waiting for heat, spark or a repair).
    active: BTreeSet<BlockPos>,
    /// Generators with fuel to burn or an item burning.
    burning: BTreeSet<BlockPos>,
    /// Stations waiting for spark, tried again every [`RETRY_EVERY`] ticks.
    retry: BTreeSet<BlockPos>,
    /// Changed since the last save.
    dirty: BTreeSet<BlockPos>,
    /// Forges with a sealed item in their queue (milestone 17).
    forging: BTreeSet<BlockPos>,
    /// The wear a machine without a record has (`MACHINE_WEAR`; tests set
    /// less).
    pub start_wear: u32,
    /// Seconds a sealed item takes in the queue, instead of its band's
    /// (tests).
    pub forge_seconds: Option<u32>,
}

impl Stations {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            active: BTreeSet::new(),
            burning: BTreeSet::new(),
            retry: BTreeSet::new(),
            dirty: BTreeSet::new(),
            forging: BTreeSet::new(),
            start_wear: MACHINE_WEAR,
            forge_seconds: None,
        }
    }

    /// Rebuilds from the save. A job whose recipe no longer exists is
    /// refunded into the station's output, so a recipe change never eats
    /// materials.
    pub fn load(saved: Vec<(BlockPos, SavedStation)>, items: &ItemRegistry, recipes: &RecipeBook) -> Self {
        let mut s = Self::new();
        for (pos, saved) in saved {
            let mut st = StationData::new(saved.kind, saved.wear);
            st.started = saved.started;
            st.coils = saved
                .coils
                .iter()
                .map(|(p, w)| (*p, (*w).min(spark::COIL_WEAR)))
                .collect();
            st.scale = saved.scale.clamp(1, 1000);
            st.heat = saved.heat;
            st.xp = saved.xp;
            if saved.kind == Station::SparkGenerator {
                st.blocks = saved.blocks.max(1);
                st.spark = saved.spark;
                st.burn_total = saved.burn_total;
                st.burn_elapsed = saved.burn_elapsed.min(burn_ticks(saved.burn_total));
            }
            let what = format!("station at {pos:?}");
            let valid = |s: &ItemStack| s.count > 0 && items.get(s.item).is_some_and(|d| s.count <= d.stack);
            for (slot, stack) in st.output.slots.iter_mut().zip(load_slots(items, &saved.output, &what)) {
                *slot = stack.filter(valid);
            }
            st.fuel = load_slots(items, std::slice::from_ref(&saved.fuel), &what)
                .into_iter()
                .next()
                .flatten()
                .filter(|s| valid(s) && items.get(s.item).is_some_and(|d| d.fuel > 0));
            if let Some(f) = &saved.forge {
                let f = forging::load_forge(f, items, &what);
                if let Some(q) = &f.queued {
                    s.forging.insert(pos);
                    if q.started.is_none() {
                        st.wait = Some(Wait::Spark(q.band.spark));
                    }
                }
                if !f.is_empty() {
                    st.forge = Some(Box::new(f));
                }
            }
            let containers = &saved.containers;
            for (j, job) in saved.jobs.into_iter().enumerate() {
                match find_recipe(&job, saved.kind, items, recipes) {
                    Some((recipe, craft)) if job.remaining > 0 => {
                        let crafts = vec![craft; job.remaining as usize];
                        let held = load_containers(items, &crafts, containers.get(j));
                        let colour = saved.job_colours.get(j).copied().unwrap_or_default();
                        // One request whose crafts took different items was
                        // saved as several jobs: they join again.
                        match st.jobs.back_mut().filter(|b| {
                            b.recipe == recipe && b.colour == colour && b.remaining < u16::MAX - job.remaining
                        }) {
                            Some(back) => {
                                back.remaining += job.remaining;
                                back.taken.extend(crafts);
                                back.containers.extend(held);
                            }
                            None => st.jobs.push_back(Job {
                                recipe,
                                remaining: job.remaining,
                                taken: crafts.into(),
                                containers: held,
                                colour,
                            }),
                        }
                    }
                    Some(_) => {}
                    None => refund(&mut st, &job, items, &what),
                }
            }
            // Why it waited is asked again: heat in a furnace, a repair when
            // worn out, else spark (tried again soon).
            st.wait = match st.jobs.front().and_then(|j| recipes.get(j.recipe)) {
                Some(_) if !saved.waiting => None,
                None => st.wait,
                Some(_) if st.kind == Station::Furnace => Some(Wait::Heat),
                Some(_) if spark::wears(st.kind) && st.wear == 0 => Some(Wait::Worn),
                Some(r) if r.power > 0 => {
                    s.retry.insert(pos);
                    Some(Wait::Power(r.power))
                }
                Some(r) => {
                    s.retry.insert(pos);
                    Some(Wait::Spark(r.spark))
                }
            };
            if st.is_empty() {
                s.dirty.insert(pos);
                continue;
            }
            if !st.jobs.is_empty() && st.wait.is_none() {
                s.active.insert(pos);
            }
            if st.burns() {
                s.burning.insert(pos);
            }
            s.map.insert(pos, st);
        }
        s
    }

    pub fn get(&self, pos: BlockPos) -> Option<&StationData> {
        self.map.get(&pos)
    }

    /// Whether a record is kept at `pos` (contents, banked heat, spark or
    /// wear).
    pub fn exists(&self, pos: BlockPos) -> bool {
        self.map.contains_key(&pos)
    }

    /// Whether the station holds jobs, output or fuel (and so cannot be
    /// broken).
    pub fn has_contents(&self, pos: BlockPos) -> bool {
        self.map.get(&pos).is_some_and(StationData::holds_anything)
    }

    /// Moves a record to another block (a station saved at one block of a
    /// four-block machine before milestone 11, to its group's first block).
    /// Does nothing if `to` has one already.
    pub fn relocate(&mut self, from: BlockPos, to: BlockPos) {
        if from == to || self.map.contains_key(&to) {
            return;
        }
        let Some(st) = self.map.remove(&from) else { return };
        for set in [&mut self.active, &mut self.burning, &mut self.retry, &mut self.forging] {
            if set.remove(&from) {
                set.insert(to);
            }
        }
        self.map.insert(to, st);
        self.dirty.insert(from);
        self.dirty.insert(to);
    }

    /// A new record for a station of `kind`.
    fn fresh(&self, kind: Station) -> StationData {
        StationData::new(kind, self.start_wear)
    }

    /// Queues the crafts of recipe `index` whose inputs were `taken`. The
    /// first craft of an idle station starts at once, drawing its heat,
    /// spark and wear.
    #[allow(clippy::too_many_arguments)]
    pub fn queue(
        &mut self,
        pos: BlockPos,
        kind: Station,
        index: u16,
        taken: Taken,
        tick: u64,
        items: &ItemRegistry,
        recipes: &RecipeBook,
        env: &dyn Surroundings,
    ) -> Result<(), CraftError> {
        let fresh = self.fresh(kind);
        let st = self.map.entry(pos).or_insert(fresh);
        if st.jobs.len() >= MAX_JOBS {
            return Err(CraftError::QueueFull);
        }
        let Ok(remaining) = u16::try_from(taken.crafts.len()) else {
            return Err(CraftError::TooMany);
        };
        if remaining == 0 {
            return Err(CraftError::TooMany);
        }
        st.jobs.push_back(Job {
            recipe: index,
            remaining,
            taken: taken.crafts.into(),
            containers: taken.containers,
            colour: taken.colour,
        });
        if st.jobs.len() == 1 {
            self.try_start(pos, tick, items, recipes, env);
        }
        self.dirty.insert(pos);
        Ok(())
    }

    /// Starts the head job's next craft of the station at `pos` at `tick`,
    /// or records why it waits. Returns whether it is running.
    fn try_start(
        &mut self,
        pos: BlockPos,
        tick: u64,
        items: &ItemRegistry,
        recipes: &RecipeBook,
        env: &dyn Surroundings,
    ) -> bool {
        let Some(mut st) = self.map.remove(&pos) else {
            return false;
        };
        let wait = self.start(&mut st, pos, items, recipes, env);
        st.wait = wait;
        st.started = tick;
        let running = wait.is_none() && !st.jobs.is_empty();
        if running {
            self.active.insert(pos);
        } else {
            self.active.remove(&pos);
        }
        if matches!(wait, Some(Wait::NoGrid | Wait::Spark(_) | Wait::Power(_))) {
            self.retry.insert(pos);
        } else {
            self.retry.remove(&pos);
        }
        self.map.insert(pos, st);
        self.dirty.insert(pos);
        running
    }

    /// The checks and costs of starting `st`'s next craft (its record taken
    /// out of the map, so generators can be drawn from): wear left, then
    /// heat, then spark; the craft's wear is taken once it starts.
    fn start(
        &mut self,
        st: &mut StationData,
        pos: BlockPos,
        items: &ItemRegistry,
        recipes: &RecipeBook,
        env: &dyn Surroundings,
    ) -> Option<Wait> {
        let recipe = recipes.get(st.jobs.front()?.recipe)?;
        if spark::wears(st.kind) && st.wear == 0 {
            return Some(Wait::Worn);
        }
        if st.kind == Station::Furnace {
            if let Some(share) = env.heat_share(pos) {
                st.heat_share.0 = Some(share);
            }
            if !st.pay_heat(items, recipes, st.heat_share.0.unwrap_or(1.0)) {
                return Some(Wait::Heat);
            }
        }
        // Power (milestone 18): the coils as they stand now.
        st.hear_coils(pos, env);
        let Some(scale) = spark::power_scale(recipe.power, st.power()) else {
            return Some(Wait::Power(recipe.power));
        };
        let scale = ((scale * 1000.0).round() as u16).clamp(1, 1000);
        let spark_cost = ((recipe.spark as u64 * scale as u64).div_ceil(1000)) as u32;
        if spark_cost > 0 {
            if let Some(grid) = env.generators(pos) {
                st.grid.0 = Some(grid);
            }
            let grid = st.grid.0.clone().unwrap_or_default();
            if grid.is_empty() {
                return Some(Wait::NoGrid);
            }
            if !self.draw_spark(&grid, spark_cost) {
                return Some(Wait::Spark(spark_cost));
            }
        }
        st.scale = scale;
        st.wear = st.wear.saturating_sub(recipe.wear as u32);
        // Working coils wear with the craft, by the same share of their
        // durability as the machine (forum check 4b).
        let coil_wear = spark::coil_wear(st.kind, recipe.wear as u32);
        if let Some(heard) = &st.heard_coils.0 {
            for c in heard {
                if let Some(w) = st.coils.get_mut(&c.pos).filter(|w| **w > 0) {
                    *w = w.saturating_sub(coil_wear);
                }
            }
        }
        None
    }

    /// The power of the forge (or any machine) keyed at `pos`, its coils
    /// asked again (milestone 18).
    pub fn forge_power(&mut self, pos: BlockPos, env: &dyn Surroundings) -> u32 {
        match self.map.get_mut(&pos) {
            Some(st) => {
                let p = st.hear_coils(pos, env);
                self.dirty.insert(pos);
                p
            }
            None => env.coils(pos).map_or(0, |cs| cs.iter().map(|c| c.power).sum()),
        }
    }

    /// Spark the generator groups keyed at `grid` hold together.
    pub fn grid_spark(&self, grid: &[BlockPos]) -> u32 {
        grid.iter()
            .filter_map(|g| self.map.get(g))
            .filter(|st| st.kind == Station::SparkGenerator)
            .fold(0u32, |sum, st| sum.saturating_add(st.spark))
    }

    /// Takes `need` spark from the generator groups keyed at `grid`, fullest
    /// first, all or nothing.
    fn draw_spark(&mut self, grid: &[BlockPos], need: u32) -> bool {
        if self.grid_spark(grid) < need {
            return false;
        }
        let mut stores: Vec<(u32, BlockPos)> = grid
            .iter()
            .filter_map(|g| {
                self.map
                    .get(g)
                    .filter(|st| st.kind == Station::SparkGenerator)
                    .map(|st| (st.spark, *g))
            })
            .collect();
        stores.sort_by(|a, b| b.0.cmp(&a.0).then(order(&a.1).cmp(&order(&b.1))));
        stores.dedup_by_key(|s| s.1);
        let mut left = need;
        for (_, g) in stores {
            if left == 0 {
                break;
            }
            let Some(st) = self.map.get_mut(&g) else { continue };
            let take = st.spark.min(left);
            st.spark -= take;
            left -= take;
            self.dirty.insert(g);
            if st.is_empty() {
                self.map.remove(&g);
            }
        }
        true
    }

    /// Whether a queue of this station has room for one more job.
    pub fn has_room(&self, pos: BlockPos) -> bool {
        self.map.get(&pos).is_none_or(|s| s.jobs.len() < MAX_JOBS)
    }

    /// Puts up to `stack.count` of `stack` into the fuel slot of the station
    /// at `pos` (merging with what is there, up to a stack). Returns how many
    /// went in. A craft waiting for heat starts if it now has enough; a
    /// generator starts burning.
    #[allow(clippy::too_many_arguments)]
    pub fn add_fuel(
        &mut self,
        pos: BlockPos,
        kind: Station,
        stack: ItemStack,
        tick: u64,
        items: &ItemRegistry,
        recipes: &RecipeBook,
        env: &dyn Surroundings,
    ) -> u16 {
        let Some(def) = items.get(stack.item).filter(|d| d.fuel > 0) else {
            return 0;
        };
        let fresh = self.fresh(kind);
        let st = self.map.entry(pos).or_insert(fresh);
        let put = match &mut st.fuel {
            None => {
                let n = stack.count.min(def.stack);
                st.fuel = Some(ItemStack { count: n, ..stack });
                n
            }
            Some(f)
                if f.item == stack.item && f.colour == stack.colour && f.tag == stack.tag && f.wear == stack.wear =>
            {
                let n = stack.count.min(def.stack.saturating_sub(f.count));
                f.count += n;
                n
            }
            Some(_) => 0,
        };
        if put > 0 {
            self.dirty.insert(pos);
            if st.burns() {
                self.burning.insert(pos);
            }
            if st.wait == Some(Wait::Heat) {
                self.try_start(pos, tick, items, recipes, env);
            }
        } else if st.is_empty() {
            self.map.remove(&pos);
        }
        put
    }

    /// Sets the block count of the generator group keyed at `key` (its
    /// capacity), if it has a record.
    pub fn set_generator_blocks(&mut self, key: BlockPos, blocks: usize) {
        if let Some(st) = self.map.get_mut(&key).filter(|st| st.kind == Station::SparkGenerator) {
            st.blocks = blocks.max(1) as u32;
            self.dirty.insert(key);
        }
    }

    /// Takes up to `count` from the fuel slot of the station at `pos`.
    pub fn take_fuel(&mut self, pos: BlockPos, count: u16) -> Option<ItemStack> {
        let st = self.map.get_mut(&pos)?;
        let f = st.fuel.as_mut()?;
        let n = count.min(f.count);
        if n == 0 {
            return None;
        }
        let taken = ItemStack { count: n, ..*f };
        f.count -= n;
        if f.count == 0 {
            st.fuel = None;
        }
        self.dirty.insert(pos);
        if st.is_empty() {
            self.map.remove(&pos);
        }
        Some(taken)
    }

    /// Puts back fuel taken by [`Self::take_fuel`] that found no room.
    pub fn return_fuel(&mut self, pos: BlockPos, kind: Station, stack: ItemStack) {
        let fresh = self.fresh(kind);
        let st = self.map.entry(pos).or_insert(fresh);
        match &mut st.fuel {
            Some(f) => f.count = f.count.saturating_add(stack.count),
            None => st.fuel = Some(stack),
        }
        if st.burns() {
            self.burning.insert(pos);
        }
        self.dirty.insert(pos);
    }

    /// Repairs the machine keyed at `pos` by `amount` wear (a spanner hit),
    /// up to `MACHINE_WEAR`; a worn-out queue starts again. Returns the wear
    /// it has now.
    /// Mends the coil at `coil` that serves the machine keyed at `pos` by
    /// `amount` (milestone 18); returns the coil's wear now. A worn-out
    /// machine waiting for power tries again.
    pub fn repair_coil(&mut self, pos: BlockPos, kind: Station, coil: BlockPos, amount: u32) -> u32 {
        let fresh = self.fresh(kind);
        let st = self.map.entry(pos).or_insert(fresh);
        let w = st.coils.entry(coil).or_insert(spark::COIL_WEAR);
        *w = w.saturating_add(amount).min(spark::COIL_WEAR);
        let wear = *w;
        if matches!(st.wait, Some(Wait::Power(_))) {
            self.retry.insert(pos);
        }
        self.dirty.insert(pos);
        wear
    }

    #[allow(clippy::too_many_arguments)]
    pub fn repair(
        &mut self,
        pos: BlockPos,
        kind: Station,
        amount: u32,
        tick: u64,
        items: &ItemRegistry,
        recipes: &RecipeBook,
        env: &dyn Surroundings,
    ) -> u32 {
        let fresh = self.fresh(kind);
        let st = self.map.entry(pos).or_insert(fresh);
        st.wear = st.wear.saturating_add(amount).min(spark::machine_wear(kind));
        // Its coils mend by the same share (Boundless since Release 235).
        let mend = spark::coil_mend(kind, amount);
        let mut coil_mended = false;
        for w in st.coils.values_mut() {
            coil_mended |= *w == 0 && mend > 0;
            *w = w.saturating_add(mend).min(spark::COIL_WEAR);
        }
        if coil_mended && matches!(st.wait, Some(Wait::Power(_))) {
            self.retry.insert(pos);
        }
        let wear = st.wear;
        let worn = st.wait == Some(Wait::Worn);
        let empty = st.is_empty();
        self.dirty.insert(pos);
        if worn {
            // The craft that starts takes its wear at once.
            self.try_start(pos, tick, items, recipes, env);
            return self.wear(pos);
        }
        if empty {
            self.map.remove(&pos);
        }
        wear
    }

    /// The wear the machine keyed at `pos` has left.
    pub fn wear(&self, pos: BlockPos) -> u32 {
        self.map.get(&pos).map_or(self.start_wear, |st| st.wear)
    }

    /// Forgets the record of a machine whose block was broken, when nothing
    /// in it stops that (its wear, banked heat; contents stop a break).
    pub fn forget(&mut self, pos: BlockPos) {
        if self.map.get(&pos).is_some_and(|st| !st.holds_anything()) {
            self.map.remove(&pos);
            self.active.remove(&pos);
            self.burning.remove(&pos);
            self.retry.remove(&pos);
            self.forging.remove(&pos);
            self.dirty.insert(pos);
        }
    }

    /// Generator blocks have joined into one group keyed at `key`, of
    /// `blocks` blocks: the records of the groups that joined (kept at
    /// `others`) add up at the key: spark, fuel, the items burning.
    pub fn join_generators(&mut self, key: BlockPos, others: &[BlockPos], blocks: usize) {
        for &o in others {
            if o == key || !self.map.get(&o).is_some_and(|st| st.kind == Station::SparkGenerator) {
                continue;
            }
            let Some(from) = self.map.remove(&o) else { continue };
            for set in [&mut self.active, &mut self.burning, &mut self.retry] {
                set.remove(&o);
            }
            self.dirty.insert(o);
            let fresh = self.fresh(Station::SparkGenerator);
            let st = self.map.entry(key).or_insert(fresh);
            st.spark = st.spark.saturating_add(from.spark);
            // One item burns at a time: the other's spark still to come goes
            // straight into the store.
            if st.burn_total == 0 {
                st.burn_total = from.burn_total;
                st.burn_elapsed = from.burn_elapsed;
            } else {
                st.spark = st.spark.saturating_add(from.burn_left());
            }
            match (&mut st.fuel, from.fuel) {
                (_, None) => {}
                (None, f) => st.fuel = f,
                (Some(f), Some(g)) => f.count = f.count.saturating_add(g.count),
            }
        }
        self.set_generator_blocks(key, blocks);
        if self.map.get(&key).is_some_and(StationData::burns) {
            self.burning.insert(key);
        }
    }

    /// A block of the generator group keyed at `key`, of `blocks` blocks,
    /// has been broken, and what is left stands as `groups` (each key and
    /// its blocks). The broken block takes its share of the store, returned
    /// for the refund; the rest is shared by the groups by size, and the
    /// item burning burns on in the first.
    pub fn break_generator(&mut self, key: BlockPos, blocks: usize, groups: &[(BlockPos, usize)]) -> u32 {
        let Some(st) = self.map.remove(&key).filter(|st| st.kind == Station::SparkGenerator) else {
            return 0;
        };
        for set in [&mut self.active, &mut self.burning, &mut self.retry] {
            set.remove(&key);
        }
        self.dirty.insert(key);
        let share = st.spark / blocks.max(1) as u32;
        let rest = (st.spark - share) as u64;
        let total: u64 = groups.iter().map(|g| g.1 as u64).sum();
        let mut given = 0u64;
        for (i, &(g, n)) in groups.iter().enumerate() {
            let part = if i + 1 == groups.len() {
                rest - given
            } else {
                rest * n as u64 / total.max(1)
            };
            given += part;
            let mut new = self.fresh(Station::SparkGenerator);
            new.blocks = n.max(1) as u32;
            new.spark = part as u32;
            if i == 0 {
                new.burn_total = st.burn_total;
                new.burn_elapsed = st.burn_elapsed;
                new.fuel = st.fuel;
            }
            self.dirty.insert(g);
            if new.is_empty() {
                continue;
            }
            if new.burns() {
                self.burning.insert(g);
            }
            self.map.insert(g, new);
        }
        share
    }

    /// Finishes every craft due by `tick`, burns generators' fuel and tries
    /// crafts waiting for spark again. Returns the stations that changed
    /// enough to show at once.
    pub fn step(
        &mut self,
        tick: u64,
        items: &ItemRegistry,
        recipes: &RecipeBook,
        env: &dyn Surroundings,
    ) -> Vec<BlockPos> {
        let mut changed = Vec::new();
        let mut out = Vec::new();
        for &pos in &self.burning {
            let Some(st) = self.map.get_mut(&pos).filter(|st| st.burns()) else {
                out.push(pos);
                continue;
            };
            let before = st.spark;
            if st.burn(items) {
                changed.push(pos);
            }
            if st.spark != before {
                self.dirty.insert(pos);
            }
            if !st.burns() {
                out.push(pos);
                changed.push(pos);
            }
        }
        for pos in out {
            self.burning.remove(&pos);
        }
        if tick % RETRY_EVERY == 0 {
            let waiting: Vec<BlockPos> = self.retry.iter().copied().collect();
            for pos in waiting {
                let before = self.map.get(&pos).and_then(|st| st.wait);
                if self.try_start(pos, tick, items, recipes, env) || self.map.get(&pos).and_then(|st| st.wait) != before
                {
                    changed.push(pos);
                }
            }
        }
        let active: Vec<BlockPos> = self.active.iter().copied().collect();
        for pos in active {
            let Some(st) = self.map.get_mut(&pos) else {
                self.active.remove(&pos);
                continue;
            };
            if st.wait.is_some() {
                self.active.remove(&pos);
                continue;
            }
            let Some(job) = st.jobs.front_mut() else {
                self.active.remove(&pos);
                continue;
            };
            let Some(recipe) = recipes.get(job.recipe) else {
                // Unreachable: indices are checked on queue and load.
                st.jobs.pop_front();
                changed.push(pos);
                continue;
            };
            if tick < st.started + scaled_ticks(recipe, st.scale) {
                continue;
            }
            // The output and any containers given back, all or nothing.
            let mut containers = job.containers.clone();
            let craft = job.taken.front().cloned().unwrap_or_default();
            let back = returned(items, &craft, &mut containers);
            let mut output = st.output.clone();
            if output
                .add_all_coloured(items, recipe.output, job.colour, recipe.output_count)
                .is_none()
                || back.iter().any(|b| add_stack(&mut output, items, *b) > 0)
            {
                // Full: wait, finished, until someone collects.
                self.active.remove(&pos);
                continue;
            }
            st.output = output;
            job.containers = containers;
            job.taken.pop_front();
            job.remaining -= 1;
            st.xp = st.xp.saturating_add(recipe.xp as u32);
            if job.remaining == 0 {
                st.jobs.pop_front();
            }
            if st.jobs.is_empty() {
                self.active.remove(&pos);
            } else {
                self.try_start(pos, tick, items, recipes, env);
            }
            changed.push(pos);
        }
        let forging: Vec<BlockPos> = self.forging.iter().copied().collect();
        for pos in forging {
            if self.step_forge(pos, tick, items, env, false) {
                changed.push(pos);
            }
        }
        changed.sort_by_key(order);
        changed.dedup();
        self.dirty.extend(changed.iter().copied());
        changed
    }

    /// One tick of the forge at `pos`'s queue: a sealed item waiting to be
    /// paid for tries again every [`RETRY_EVERY`] ticks (or at once, with
    /// `now`); one whose time is up goes into the output, its XP with it,
    /// when it fits. Returns whether anything changed.
    fn step_forge(
        &mut self,
        pos: BlockPos,
        tick: u64,
        items: &ItemRegistry,
        env: &dyn Surroundings,
        now: bool,
    ) -> bool {
        let Some(mut st) = self.map.remove(&pos) else {
            self.forging.remove(&pos);
            return false;
        };
        let mut changed = false;
        match st.sealed().map(|q| (q.started, q.ticks(), q.band, q.item)) {
            None => {
                self.forging.remove(&pos);
            }
            Some((None, _, band, _)) => {
                if now || tick % RETRY_EVERY == 0 {
                    let before = st.wait;
                    let wait = self.pay_forge(&mut st, pos, band, env);
                    st.wait = wait;
                    if wait.is_none()
                        && let Some(q) = st.forge.as_mut().and_then(|f| f.queued.as_mut())
                    {
                        q.started = Some(tick);
                    }
                    changed = wait != before || wait.is_none();
                }
            }
            Some((Some(started), ticks, band, item)) => {
                if tick >= started + ticks && add_stack(&mut st.output, items, item) == 0 {
                    st.xp = st.xp.saturating_add(band.xp);
                    if let Some(f) = &mut st.forge {
                        f.queued = None;
                    }
                    self.forging.remove(&pos);
                    changed = true;
                }
            }
        }
        if changed {
            self.dirty.insert(pos);
        }
        self.map.insert(pos, st);
        if changed {
            self.tidy(pos);
        }
        changed
    }

    /// The costs of a sealed item's time: wear left, then spark from the
    /// forge's links; its wear is taken once paid. Why it waits, if it does.
    fn pay_forge(
        &mut self,
        st: &mut StationData,
        pos: BlockPos,
        band: QueueBand,
        env: &dyn Surroundings,
    ) -> Option<Wait> {
        if st.wear == 0 {
            return Some(Wait::Worn);
        }
        if band.spark > 0 {
            if let Some(grid) = env.generators(pos) {
                st.grid.0 = Some(grid);
            }
            let grid = st.grid.0.clone().unwrap_or_default();
            if grid.is_empty() {
                return Some(Wait::NoGrid);
            }
            if !self.draw_spark(&grid, band.spark) {
                return Some(Wait::Spark(band.spark));
            }
        }
        st.wear = st.wear.saturating_sub(band.wear);
        None
    }

    /// The forge's session and queue at `pos`, if it has any.
    pub fn forge(&self, pos: BlockPos) -> Option<&ForgeData> {
        self.map.get(&pos)?.forge.as_deref()
    }

    /// Starts a session in the forge at `pos` (it has none: the caller
    /// checked).
    pub fn begin_forge(&mut self, pos: BlockPos, forging: Forging) {
        let fresh = self.fresh(Station::Forge);
        let st = self.map.entry(pos).or_insert(fresh);
        st.forge.get_or_insert_with(Default::default).session = Some(forging);
        self.dirty.insert(pos);
    }

    /// The session in the forge at `pos`, to play a round (marked changed).
    pub fn forging_mut(&mut self, pos: BlockPos) -> Option<&mut Forging> {
        let f = self.map.get_mut(&pos)?.forge.as_mut()?.session.as_mut()?;
        self.dirty.insert(pos);
        Some(f)
    }

    /// Ends the session in the forge at `pos`, returning it.
    pub fn end_forge(&mut self, pos: BlockPos) -> Option<Forging> {
        let st = self.map.get_mut(&pos)?;
        let f = st.forge.as_mut()?.session.take();
        self.tidy(pos);
        f
    }

    /// Puts a sealed item into the forge at `pos`'s queue for its band's
    /// time (or the test's [`Stations::forge_seconds`]), paying at once if
    /// it can.
    pub fn seal(
        &mut self,
        pos: BlockPos,
        item: ItemStack,
        mut band: QueueBand,
        tick: u64,
        items: &ItemRegistry,
        env: &dyn Surroundings,
    ) {
        if let Some(s) = self.forge_seconds {
            band.seconds = s;
        }
        let fresh = self.fresh(Station::Forge);
        let st = self.map.entry(pos).or_insert(fresh);
        st.forge.get_or_insert_with(Default::default).queued = Some(Sealed {
            item,
            band,
            started: None,
        });
        self.forging.insert(pos);
        self.dirty.insert(pos);
        self.step_forge(pos, tick, items, env, true);
    }

    /// Adds to a station's output (what a forge gives back that its forger
    /// has no room for). Returns what did not fit.
    pub fn add_output(&mut self, pos: BlockPos, kind: Station, stack: ItemStack, items: &ItemRegistry) -> u16 {
        let fresh = self.fresh(kind);
        let st = self.map.entry(pos).or_insert(fresh);
        let left = add_stack(&mut st.output, items, stack);
        self.dirty.insert(pos);
        self.tidy(pos);
        left
    }

    /// Drops a record left with nothing worth keeping.
    fn tidy(&mut self, pos: BlockPos) {
        let Some(st) = self.map.get_mut(&pos) else { return };
        if st.forge.as_ref().is_some_and(|f| f.is_empty()) {
            st.forge = None;
        }
        if st.is_empty() {
            self.map.remove(&pos);
            for set in [&mut self.active, &mut self.burning, &mut self.retry, &mut self.forging] {
                set.remove(&pos);
            }
        }
        self.dirty.insert(pos);
    }

    /// Moves as much output as fits into `inv`. Returns whether anything moved.
    pub fn collect(&mut self, pos: BlockPos, inv: &mut Inventory, items: &ItemRegistry) -> bool {
        self.collect_xp(pos, inv, items).is_some()
    }

    /// [`Stations::collect`], returning the craft XP the collector earns
    /// (all the station holds, once anything moves), or None if nothing
    /// moved.
    pub fn collect_xp(&mut self, pos: BlockPos, inv: &mut Inventory, items: &ItemRegistry) -> Option<u32> {
        let st = self.map.get_mut(&pos)?;
        let mut moved = false;
        for slot in st.output.slots.iter_mut() {
            let Some(s) = slot else { continue };
            let left = add_stack(inv, items, *s);
            if left != s.count {
                moved = true;
            }
            if left == 0 {
                *slot = None;
            } else {
                s.count = left;
            }
        }
        if moved {
            self.dirty.insert(pos);
            // There may be room now for a craft that was waiting.
            if !st.jobs.is_empty() && st.wait.is_none() {
                self.active.insert(pos);
            }
            let xp = std::mem::take(&mut st.xp);
            if st.is_empty() {
                self.map.remove(&pos);
            }
            return Some(xp);
        }
        None
    }

    /// Removes a station's data, returning everything in it: its output, its
    /// fuel, and the inputs of every craft not yet finished (the one under
    /// way too). Banked heat and spark are lost. Used when its claim lapses.
    pub fn take(&mut self, pos: BlockPos, items: &ItemRegistry, recipes: &RecipeBook) -> Vec<ItemStack> {
        let Some(st) = self.map.remove(&pos) else {
            return Vec::new();
        };
        for set in [&mut self.active, &mut self.burning, &mut self.retry, &mut self.forging] {
            set.remove(&pos);
        }
        self.dirty.insert(pos);
        let mut out: Vec<ItemStack> = st.output.slots.iter().flatten().copied().collect();
        out.extend(st.fuel);
        out.extend(st.forge.iter().flat_map(|f| f.items()));
        for job in &st.jobs {
            // Full containers come back as they went in; the rest exactly
            // as the crafts took them, in the order first taken.
            out.extend(job.containers.iter().copied());
            let mut totals: Vec<(ItemId, u32)> = Vec::new();
            for &(item, n) in job.taken.iter().flatten() {
                if is_container(items, item) {
                    continue;
                }
                match totals.iter_mut().find(|(i, _)| *i == item) {
                    Some((_, t)) => *t += n as u32,
                    None => totals.push((item, n as u32)),
                }
            }
            // The tinted input comes back in the job's colour (milestone 22).
            let tinted = recipes.get(job.recipe).and_then(|r| r.tint.map(|t| (r, t)));
            for (item, mut total) in totals {
                let colour = match tinted {
                    Some((r, t)) if r.input_of(item) == Some(t as usize) => job.colour,
                    _ => Colour::NATURAL,
                };
                while total > 0 {
                    let c = total.min(u16::MAX as u32) as u16;
                    out.push(ItemStack::coloured(item, colour, c));
                    total -= c as u32;
                }
            }
        }
        out
    }

    /// Stations whose current craft is under way, and generators burning
    /// (for progress updates).
    pub fn running(&self) -> impl Iterator<Item = BlockPos> + '_ {
        self.active
            .iter()
            .chain(self.burning.iter())
            .chain(self.forging.iter())
            .copied()
    }

    /// What a player sees. `kind` is the machine's station kind, used for an
    /// empty station; `idle` why the machine does not work, if it does not
    /// (else why its craft waits, if it does). Spark is the caller's to
    /// fill in: it needs the world.
    pub fn view(
        &self,
        pos: BlockPos,
        kind: Station,
        idle: Option<String>,
        tick: u64,
        recipes: &RecipeBook,
    ) -> StationView {
        let wear =
            spark::wears(kind).then(|| (self.wear(pos).min(spark::machine_wear(kind)), spark::machine_wear(kind)));
        let Some(st) = self.map.get(&pos) else {
            return StationView {
                pos,
                station: kind,
                jobs: Vec::new(),
                output: Vec::new(),
                idle,
                fuel: None,
                heat: 0,
                wear,
                spark: None,
                forge: None,
                power: None,
            };
        };
        let jobs = st
            .jobs
            .iter()
            .enumerate()
            .map(|(i, j)| {
                let progress = match (i, recipes.get(j.recipe)) {
                    (0, Some(r)) if st.wait.is_none() => {
                        (tick.saturating_sub(st.started) as f32 / scaled_ticks(r, st.scale) as f32).clamp(0.0, 1.0)
                    }
                    _ => 0.0,
                };
                JobView {
                    recipe: j.recipe,
                    remaining: j.remaining,
                    progress,
                }
            })
            .collect();
        StationView {
            pos,
            station: st.kind,
            jobs,
            output: st.output.slots.iter().flatten().copied().collect(),
            idle: idle.or_else(|| st.wait.and_then(Wait::text)),
            fuel: st.fuel,
            heat: st.heat,
            wear,
            spark: None,
            forge: None,
            power: None,
        }
    }

    /// Changed stations for the save; None for one now empty.
    pub fn take_dirty(&mut self, items: &ItemRegistry, recipes: &RecipeBook) -> Vec<(BlockPos, Option<SavedStation>)> {
        std::mem::take(&mut self.dirty)
            .into_iter()
            .map(|pos| (pos, self.map.get(&pos).map(|st| save_station(st, items, recipes))))
            .collect()
    }
}

/// The order that picks a group's first block (as `machines` does).
fn order(p: &BlockPos) -> (i64, i64, i64) {
    (p.y, p.z, p.x)
}

impl Default for Stations {
    fn default() -> Self {
        Self::new()
    }
}

/// Adds a stack, keeping its wear (a tool is never merged). Returns what did
/// not fit.
fn add_stack(inv: &mut Inventory, items: &ItemRegistry, s: ItemStack) -> u16 {
    inv.add_stack(items, s)
}

/// A job's saved containers, checked against what its crafts took: any
/// missing come back fresh (a save from before milestone 16 holds none).
fn load_containers(items: &ItemRegistry, crafts: &[Craft], saved: Option<&Vec<SavedStack>>) -> Vec<ItemStack> {
    let mut have: Vec<ItemStack> = saved
        .map(|v| v.iter().filter_map(|s| s.resolve(items)).collect())
        .unwrap_or_default();
    let mut out = Vec::new();
    for (item, n) in crafts.iter().flatten() {
        if !is_container(items, *item) {
            continue;
        }
        for _ in 0..*n {
            out.push(match have.iter().position(|c| c.item == *item) {
                Some(i) => ItemStack {
                    count: 1,
                    ..have.remove(i)
                },
                None => ItemStack::new(*item, 1),
            });
        }
    }
    out
}

/// A job as runs of crafts that took the same items, each saved as a job of
/// its own with its items spelled out (and its share of the containers):
/// the save keeps the real items taken, and loading joins them again.
fn job_runs(items: &ItemRegistry, job: &Job) -> Vec<(u16, Craft, Vec<ItemStack>)> {
    let mut runs: Vec<(u16, Craft, Vec<ItemStack>)> = Vec::new();
    let mut held = job.containers.iter().copied();
    for craft in &job.taken {
        let mine: Vec<ItemStack> = held
            .by_ref()
            .take(
                craft
                    .iter()
                    .filter(|(i, _)| is_container(items, *i))
                    .map(|(_, n)| *n as usize)
                    .sum(),
            )
            .collect();
        match runs.last_mut().filter(|(_, c, _)| c == craft) {
            Some(run) => {
                run.0 += 1;
                run.2.extend(mine);
            }
            None => runs.push((1, craft.clone(), mine)),
        }
    }
    runs
}

fn save_station(st: &StationData, items: &ItemRegistry, recipes: &RecipeBook) -> SavedStation {
    let name = |id| items.get(id).map(|d| d.name.clone()).unwrap_or_default();
    let mut jobs = Vec::new();
    let mut containers = Vec::new();
    let mut job_colours = Vec::new();
    for j in &st.jobs {
        let Some(r) = recipes.get(j.recipe) else { continue };
        for (remaining, craft, held) in job_runs(items, j) {
            job_colours.push(j.colour);
            jobs.push(SavedJob {
                output: name(r.output),
                output_count: r.output_count,
                inputs: craft.iter().map(|(i, n)| (name(*i), *n)).collect(),
                remaining,
            });
            containers.push(held.iter().filter_map(|c| SavedStack::new(items, c)).collect());
        }
    }
    SavedStation {
        kind: st.kind,
        jobs,
        started: st.started,
        output: save_slots(items, &st.output),
        fuel: st.fuel.as_ref().and_then(|f| SavedStack::new(items, f)),
        heat: st.heat,
        waiting: st.wait.is_some(),
        xp: st.xp,
        wear: st.wear,
        spark: st.spark,
        burn_total: st.burn_total,
        burn_elapsed: st.burn_elapsed,
        blocks: st.blocks,
        containers,
        forge: st.forge.as_ref().map(|f| forging::save_forge(f, items)),
        coils: st.coils.iter().map(|(p, w)| (*p, *w)).collect(),
        scale: st.scale,
        job_colours,
    }
}

/// The recipe a saved job was (same station and output, and its inputs
/// serve exactly as the items its crafts took: a recipe that became a group
/// still takes its old item), with those items.
fn find_recipe(job: &SavedJob, kind: Station, items: &ItemRegistry, recipes: &RecipeBook) -> Option<(u16, Craft)> {
    let output = items.id(&job.output).ok()?;
    let craft = job
        .inputs
        .iter()
        .map(|(n, c)| Some((items.id(n).ok()?, *c)))
        .collect::<Option<Craft>>()?;
    recipes
        .at(kind)
        .find(|(_, r)| r.output == output && r.output_count == job.output_count && r.matches(&craft))
        .map(|(i, _)| (i, craft))
}

fn refund(st: &mut StationData, job: &SavedJob, items: &ItemRegistry, what: &str) {
    tracing::warn!(
        "{what}: the recipe for `{}` changed; refunding {} crafts' inputs",
        job.output,
        job.remaining
    );
    for (name, n) in &job.inputs {
        let total = (*n as u32 * job.remaining as u32).min(u16::MAX as u32) as u16;
        let left = SavedStack {
            item: name.clone(),
            count: total,
            wear: 0,
        }
        .resolve(items)
        .map_or(total, |s| st.output.add(items, s.item, s.count));
        if left > 0 {
            tracing::error!("{what}: no room to refund {left} × `{name}`; lost");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bm_core::BlockRegistry;

    fn plain(recipes: &RecipeBook, index: u16, times: u16) -> Taken {
        Taken::plain(recipes.get(index).unwrap(), times)
    }

    fn setup() -> (ItemRegistry, RecipeBook) {
        let items = ItemRegistry::builtin(&BlockRegistry::builtin());
        let recipes = RecipeBook::builtin(&items);
        (items, recipes)
    }

    fn recipe_for(items: &ItemRegistry, recipes: &RecipeBook, name: &str) -> u16 {
        let id = items.id(name).unwrap();
        recipes.recipes.iter().position(|r| r.output == id).unwrap() as u16
    }

    fn coal(items: &ItemRegistry, n: u16) -> ItemStack {
        ItemStack::new(items.id("coal").unwrap(), n)
    }

    #[test]
    fn hand_crafts_are_all_or_nothing() {
        let (items, recipes) = setup();
        let timber = recipe_for(&items, &recipes, "timber");
        let r = recipes.get(timber).unwrap();
        let mut inv = Inventory::default();
        inv.add(&items, items.id("trunk").unwrap(), 2);
        assert_eq!(craft_by_hand(&mut inv, &items, r, 3), Err(CraftError::Materials));
        assert_eq!(inv.count(items.id("trunk").unwrap()), 2);
        craft_by_hand(&mut inv, &items, r, 2).unwrap();
        assert_eq!(
            inv.count(items.id("timber").unwrap()),
            4,
            "2 timber a trunk, as in Boundless"
        );
        assert_eq!(inv.count(items.id("trunk").unwrap()), 0);

        // Outputs that cannot fit refuse the craft and keep the inputs.
        let mut full = Inventory::default();
        full.add(&items, items.id("stone").unwrap(), 100 * 39);
        full.slots[39] = Some(ItemStack::new(items.id("trunk").unwrap(), 2));
        // One trunk out leaves the slot holding the other: no room for timber.
        assert_eq!(craft_by_hand(&mut full, &items, r, 1), Err(CraftError::NoRoom));
        assert_eq!(full.count(items.id("trunk").unwrap()), 2);
        // Both trunks out free the slot for the timber.
        craft_by_hand(&mut full, &items, r, 2).unwrap();
        assert_eq!(full.count(items.id("timber").unwrap()), 4);
    }

    #[test]
    fn jobs_finish_over_time_and_pause_when_the_output_is_full() {
        let (items, recipes) = setup();
        let stones = recipe_for(&items, &recipes, "stones");
        let r = recipes.get(stones).unwrap();
        assert_eq!((r.station, r.output_count, r.heat), (Station::CraftingTable, 2, 0));
        let ticks = craft_ticks(r);
        let pos = BlockPos::new(1, 2, 3);
        let mut s = Stations::new();
        s.queue(
            pos,
            Station::CraftingTable,
            stones,
            plain(&recipes, stones, 3),
            100,
            &items,
            &recipes,
            &Nowhere,
        )
        .unwrap();
        assert!(s.step(100 + ticks - 1, &items, &recipes, &Nowhere).is_empty());
        let v = s.view(pos, Station::CraftingTable, None, 100 + ticks / 2, &recipes);
        assert!((v.jobs[0].progress - 0.5).abs() < 0.02);
        assert_eq!((v.fuel, v.heat, v.idle), (None, 0, None));
        assert_eq!(s.step(100 + ticks, &items, &recipes, &Nowhere), vec![pos]);
        let v = s.view(pos, Station::CraftingTable, None, 100 + ticks, &recipes);
        assert_eq!((v.jobs[0].remaining, v.output.len()), (2, 1));
        assert_eq!(v.output[0].count, 2);

        // Fill the output with other things: the next craft waits.
        let st = s.map.get_mut(&pos).unwrap();
        for slot in st.output.slots.iter_mut().skip(1) {
            *slot = Some(ItemStack::new(items.id("glass").unwrap(), 100));
        }
        st.output.slots[0] = Some(ItemStack::new(items.id("stones").unwrap(), 100));
        assert!(s.step(100 + 2 * ticks, &items, &recipes, &Nowhere).is_empty());
        assert_eq!(s.running().count(), 0, "paused");
        assert_eq!(s.get(pos).unwrap().jobs[0].remaining, 2);

        // Collecting makes room and the waiting craft finishes at once.
        let mut inv = Inventory::default();
        assert!(s.collect(pos, &mut inv, &items));
        assert_eq!(inv.count(items.id("stones").unwrap()), 100);
        assert_eq!(s.step(100 + 5 * ticks, &items, &recipes, &Nowhere), vec![pos]);
        assert_eq!(s.get(pos).unwrap().jobs[0].remaining, 1);
        assert_eq!(s.step(100 + 6 * ticks, &items, &recipes, &Nowhere), vec![pos]);
        assert!(s.get(pos).unwrap().jobs.is_empty());
        assert!(s.has_contents(pos), "output waits");
        let mut inv = Inventory::default();
        assert!(s.collect(pos, &mut inv, &items));
        assert!(!s.has_contents(pos) && !s.exists(pos), "an empty station has no data");
    }

    /// Milestone 16: a soup takes its bucket of water with its fills used,
    /// gives it back empty and one fill more worn as each craft finishes, and
    /// never on its last fill; a station taken gives the full buckets back
    /// as they went in; a save keeps them.
    #[test]
    fn buckets_come_back_from_soup_one_fill_more_worn() {
        let (items, recipes) = setup();
        let soup = recipe_for(&items, &recipes, "tuber_soup");
        let r = recipes.get(soup).unwrap().clone();
        let water = items.id("water_bucket").unwrap();
        let bucket = items.id("bucket").unwrap();
        let tuber = items.id("tuber").unwrap();
        let mut inv = Inventory::default();
        inv.add(&items, tuber, 6);
        inv.slots[5] = Some(ItemStack {
            wear: 3,
            ..ItemStack::new(water, 1)
        });
        inv.slots[6] = Some(ItemStack {
            wear: 9,
            ..ItemStack::new(water, 1)
        });
        assert!(
            take_inputs_keeping(&mut inv.clone(), &items, &r, 3).is_none(),
            "two buckets, three soups"
        );
        let taken = take_inputs_keeping(&mut inv, &items, &r, 2).unwrap();
        let containers = taken.containers.clone();
        assert_eq!(containers.iter().map(|c| c.wear).collect::<Vec<_>>(), vec![3, 9]);
        assert_eq!((inv.count(tuber), inv.count(water)), (0, 0));

        let pos = BlockPos::new(0, 30, 0);
        let mut s = Stations::new();
        s.queue(pos, Station::Furnace, soup, taken, 10, &items, &recipes, &Nowhere)
            .unwrap();
        // Saved and loaded, the buckets stay with the job.
        let saved: Vec<_> = s
            .take_dirty(&items, &recipes)
            .into_iter()
            .filter_map(|(p, st)| Some((p, st?)))
            .collect();
        let mut loaded = Stations::load(saved, &items, &recipes);
        assert_eq!(loaded.get(pos).unwrap().jobs[0].containers, containers);
        // A station taken gives back what went in.
        let back = loaded.take(pos, &items, &recipes);
        assert!(back.contains(&ItemStack {
            wear: 3,
            ..ItemStack::new(water, 1)
        }));
        assert!(back.contains(&ItemStack {
            wear: 9,
            ..ItemStack::new(water, 1)
        }));
        assert_eq!(back.iter().filter(|b| b.item == tuber).map(|b| b.count).sum::<u16>(), 6);

        // Cooked: the first bucket back with 4 fills used, the second (its
        // tenth fill) worn out.
        s.add_fuel(pos, Station::Furnace, coal(&items, 5), 10, &items, &recipes, &Nowhere);
        run(&mut s, 10, 10 + 3 * craft_ticks(&r), &items, &recipes, &Nowhere);
        let st = s.get(pos).unwrap();
        assert!(st.jobs.is_empty());
        assert_eq!(st.output.count(items.id("tuber_soup").unwrap()), 2);
        let empties: Vec<_> = st.output.slots.iter().flatten().filter(|o| o.item == bucket).collect();
        assert_eq!(empties.len(), 1);
        assert_eq!((empties[0].count, empties[0].wear), (1, 4));
    }

    /// A group input (milestone 19) takes mixed members in the group's
    /// order; the job keeps what each craft took, through a save, and a
    /// station taken gives back exactly the items its unfinished crafts took.
    #[test]
    fn group_inputs_take_mixed_items_and_refund_exactly_those() {
        // (Milestone 22 split stones by rock, so the group is Any Stones in
        // the stone hammer.)
        let (items, recipes) = setup();
        let id = |n: &str| items.id(n).unwrap();
        let (stones, basalt, limestone) = (id("stones"), id("basalt_stones"), id("limestone_stones"));
        let (stick, leaves) = (id("stick"), id("foliage"));
        let index = recipes
            .at(Station::CraftingTable)
            .find(|(_, r)| r.output == id("stone_hammer"))
            .unwrap()
            .0;
        let r = recipes.get(index).unwrap().clone();
        let mut inv = Inventory::default();
        inv.add(&items, limestone, 6);
        inv.add(&items, basalt, 2);
        inv.add(&items, stones, 1);
        inv.add(&items, stick, 8);
        inv.add(&items, leaves, 8);
        let taken = take_inputs_keeping(&mut inv, &items, &r, 2).unwrap();
        assert_eq!(
            taken.crafts,
            vec![
                vec![(stones, 1), (basalt, 2), (limestone, 1), (stick, 4), (leaves, 4)],
                vec![(limestone, 4), (stick, 4), (leaves, 4)],
            ]
        );
        assert_eq!((inv.count(stones), inv.count(basalt), inv.count(limestone)), (0, 0, 1));

        let pos = BlockPos::new(4, 30, 4);
        let mut s = Stations::new();
        s.queue(pos, Station::CraftingTable, index, taken, 0, &items, &recipes, &Nowhere)
            .unwrap();
        // Saved as runs of the real items taken, loaded as one job again.
        let saved: Vec<_> = s
            .take_dirty(&items, &recipes)
            .into_iter()
            .filter_map(|(p, st)| Some((p, st?)))
            .collect();
        let jobs = &saved[0].1.jobs;
        assert_eq!(jobs.iter().map(|j| j.remaining).collect::<Vec<_>>(), vec![1, 1]);
        assert_eq!(jobs[1].inputs[0], ("limestone_stones".to_string(), 4));
        let mut loaded = Stations::load(saved, &items, &recipes);
        assert_eq!(loaded.get(pos), s.get(pos));
        assert_eq!(loaded.get(pos).unwrap().jobs.len(), 1);

        // The first craft done: the second's items come back as taken.
        run(&mut loaded, 0, craft_ticks(&r), &items, &recipes, &Nowhere);
        assert_eq!(loaded.get(pos).unwrap().jobs[0].remaining, 1);
        let mut back = loaded.take(pos, &items, &recipes);
        back.sort_by_key(|b| b.item);
        let mut want = vec![
            ItemStack::new(id("stone_hammer"), 1),
            ItemStack::new(limestone, 4),
            ItemStack::new(stick, 4),
            ItemStack::new(leaves, 4),
        ];
        want.sort_by_key(|b| b.item);
        assert_eq!(back, want);
    }

    /// Milestone 22: a tinted recipe takes its input in the chosen colour
    /// and makes its output in it, by hand and at a station, through a save
    /// and a reload; a station taken gives the input back in that colour.
    #[test]
    fn tinted_recipes_pass_the_chosen_colour_on() {
        let (items, recipes) = setup();
        let id = |n: &str| items.id(n).unwrap();
        let (red, blue) = (Colour(5), Colour(180));
        // By hand: timber from a red trunk, never from the blue one.
        let timber = recipes.recipes.iter().position(|r| r.output == id("timber")).unwrap() as u16;
        let r = recipes.get(timber).unwrap().clone();
        let mut inv = Inventory::default();
        inv.add_coloured(&items, id("trunk"), red, 1);
        inv.add_coloured(&items, id("trunk"), blue, 3);
        assert_eq!(r.colour_choices(&inv), vec![(blue, 3), (red, 1)]);
        assert_eq!(
            craft_by_hand_in(&mut inv, &items, &r, 2, Some(red)),
            Err(CraftError::Materials),
            "only one red trunk"
        );
        craft_by_hand_in(&mut inv, &items, &r, 1, Some(red)).unwrap();
        assert_eq!(inv.count_coloured(id("timber"), red), 2);
        assert_eq!(inv.count_coloured(id("trunk"), blue), 3, "blue untouched");
        // With no colour asked, the most plentiful.
        craft_by_hand(&mut inv, &items, &r, 1).unwrap();
        assert_eq!(inv.count_coloured(id("timber"), blue), 2);

        // At a crafting table: red stones from red stone.
        let index = recipes
            .at(Station::CraftingTable)
            .find(|(_, r)| r.output == id("stones") && r.inputs[0].accepts(id("stone")))
            .unwrap()
            .0;
        let r = recipes.get(index).unwrap().clone();
        let mut inv = Inventory::default();
        inv.add_coloured(&items, id("stone"), red, 3);
        inv.add_coloured(&items, id("stone"), blue, 3);
        let taken = take_inputs_keeping_in(&mut inv, &items, &r, 3, Some(red)).unwrap();
        assert_eq!(taken.colour, red);
        assert_eq!(
            (
                inv.count_coloured(id("stone"), red),
                inv.count_coloured(id("stone"), blue)
            ),
            (0, 3)
        );
        let pos = BlockPos::new(4, 30, 4);
        let mut s = Stations::new();
        s.queue(pos, Station::CraftingTable, index, taken, 0, &items, &recipes, &Nowhere)
            .unwrap();
        let saved: Vec<_> = s
            .take_dirty(&items, &recipes)
            .into_iter()
            .filter_map(|(p, st)| Some((p, st?)))
            .collect();
        assert_eq!(saved[0].1.job_colours, vec![red]);
        let mut loaded = Stations::load(saved, &items, &recipes);
        assert_eq!(loaded.get(pos), s.get(pos));
        run(&mut loaded, 0, craft_ticks(&r), &items, &recipes, &Nowhere);
        let out = loaded.get(pos).unwrap().output.clone();
        assert_eq!(out.count_coloured(id("stones"), red), 2, "made red");
        // The two crafts left come back red.
        let back = loaded.take(pos, &items, &recipes);
        assert!(back.contains(&ItemStack::coloured(id("stone"), red, 2)), "{back:?}");
    }

    /// Each craft draws its heat as it starts, burning fuel as needed; with
    /// none, it waits without progress until fuel arrives (soft coal 100,
    /// a copper bar 100, an iron bar 150: tuning 4.7).
    #[test]
    fn furnace_crafts_burn_fuel_for_heat_and_wait_without_it() {
        let (items, recipes) = setup();
        let copper = recipe_for(&items, &recipes, "copper_ingot");
        let iron = recipe_for(&items, &recipes, "iron_ingot");
        assert_eq!(recipes.get(copper).unwrap().heat, 100);
        assert_eq!(recipes.get(iron).unwrap().heat, 150);
        let ticks = craft_ticks(recipes.get(copper).unwrap());
        let pos = BlockPos::new(0, 30, 0);
        let mut s = Stations::new();
        s.queue(
            pos,
            Station::Furnace,
            copper,
            plain(&recipes, copper, 2),
            10,
            &items,
            &recipes,
            &Nowhere,
        )
        .unwrap();
        assert_eq!(s.running().count(), 0, "no heat: it waits");
        assert!(s.step(10 + 2 * ticks, &items, &recipes, &Nowhere).is_empty());
        let v = s.view(pos, Station::Furnace, None, 10 + ticks, &recipes);
        assert_eq!(v.jobs[0].progress, 0.0, "no progress while it waits");

        // One coal: the first craft starts now, the bank spent.
        let t = 1000;
        assert_eq!(
            s.add_fuel(pos, Station::Furnace, coal(&items, 1), t, &items, &recipes, &Nowhere),
            1
        );
        let st = s.get(pos).unwrap();
        assert_eq!((st.fuel, st.heat, st.wait.is_some(), st.started), (None, 0, false, t));
        assert_eq!(s.running().collect::<Vec<_>>(), vec![pos]);
        let v = s.view(pos, Station::Furnace, None, t + ticks / 2, &recipes);
        assert!((v.jobs[0].progress - 0.5).abs() < 0.02);
        // It finishes; the second has nothing to burn and waits.
        assert_eq!(s.step(t + ticks, &items, &recipes, &Nowhere), vec![pos]);
        let st = s.get(pos).unwrap();
        assert!(st.wait.is_some() && st.jobs[0].remaining == 1);
        assert_eq!(st.output.count(items.id("copper_ingot").unwrap()), 1);
        assert_eq!(s.running().count(), 0);
        // Five more: one burns, four wait in the slot.
        let t = 5000;
        assert_eq!(
            s.add_fuel(pos, Station::Furnace, coal(&items, 5), t, &items, &recipes, &Nowhere),
            5
        );
        let st = s.get(pos).unwrap();
        assert_eq!(
            (st.fuel.map(|f| f.count), st.heat, st.wait.is_some()),
            (Some(4), 0, false)
        );
        assert_eq!(s.step(t + ticks, &items, &recipes, &Nowhere), vec![pos]);
        assert!(s.get(pos).unwrap().jobs.is_empty());

        // An iron bar burns two coal and banks the 50 left over.
        s.queue(
            pos,
            Station::Furnace,
            iron,
            plain(&recipes, iron, 1),
            9000,
            &items,
            &recipes,
            &Nowhere,
        )
        .unwrap();
        let st = s.get(pos).unwrap();
        assert_eq!(
            (st.fuel.map(|f| f.count), st.heat, st.wait.is_some()),
            (Some(2), 50, false)
        );
        let v = s.view(pos, Station::Furnace, None, 9000, &recipes);
        assert_eq!((v.fuel.map(|f| f.count), v.heat), (Some(2), 50));
    }

    #[test]
    fn the_fuel_slot_takes_one_stack_of_something_that_burns() {
        let (items, recipes) = setup();
        let pos = BlockPos::new(5, 30, 5);
        let mut s = Stations::new();
        let stone = ItemStack::new(items.id("stone").unwrap(), 3);
        assert_eq!(
            s.add_fuel(pos, Station::Furnace, stone, 0, &items, &recipes, &Nowhere),
            0
        );
        assert!(!s.exists(pos), "nothing kept for a refusal");
        let cap = items.get(items.id("coal").unwrap()).unwrap().stack;
        assert_eq!(
            s.add_fuel(
                pos,
                Station::Furnace,
                coal(&items, cap - 2),
                0,
                &items,
                &recipes,
                &Nowhere
            ),
            cap - 2
        );
        assert_eq!(
            s.add_fuel(pos, Station::Furnace, coal(&items, 5), 0, &items, &recipes, &Nowhere),
            2,
            "up to a stack"
        );
        let stick = ItemStack::new(items.id("stick").unwrap(), 4);
        assert_eq!(
            s.add_fuel(pos, Station::Furnace, stick, 0, &items, &recipes, &Nowhere),
            0,
            "one kind at a time"
        );
        assert!(s.has_contents(pos), "fuel stops it being broken");
        assert_eq!(s.take_fuel(pos, 3), Some(coal(&items, 3)));
        assert_eq!(s.take_fuel(pos, u16::MAX), Some(coal(&items, cap - 3)));
        assert_eq!(s.take_fuel(pos, 1), None);
        assert!(!s.exists(pos));
        s.return_fuel(pos, Station::Furnace, coal(&items, 2));
        assert_eq!(s.get(pos).unwrap().fuel, Some(coal(&items, 2)));

        // Banked heat alone is kept, but does not stop a break.
        let iron = recipe_for(&items, &recipes, "iron_ingot");
        s.queue(
            pos,
            Station::Furnace,
            iron,
            plain(&recipes, iron, 1),
            0,
            &items,
            &recipes,
            &Nowhere,
        )
        .unwrap();
        s.step(10_000, &items, &recipes, &Nowhere);
        let mut inv = Inventory::default();
        assert!(s.collect(pos, &mut inv, &items));
        assert_eq!(s.get(pos).unwrap().heat, 50);
        assert!(s.exists(pos) && !s.has_contents(pos));
    }

    #[test]
    fn taking_a_station_returns_its_output_fuel_and_unfinished_inputs() {
        let (items, recipes) = setup();
        let ingot = recipe_for(&items, &recipes, "copper_ingot");
        let r = recipes.get(ingot).unwrap().clone();
        let ticks = craft_ticks(&r);
        let pos = BlockPos::new(4, 5, 6);
        let mut s = Stations::new();
        s.add_fuel(pos, Station::Furnace, coal(&items, 4), 0, &items, &recipes, &Nowhere);
        s.queue(
            pos,
            Station::Furnace,
            ingot,
            plain(&recipes, ingot, 3),
            0,
            &items,
            &recipes,
            &Nowhere,
        )
        .unwrap();
        s.step(ticks, &items, &recipes, &Nowhere);
        s.take_dirty(&items, &recipes);
        let mut got = s.take(pos, &items, &recipes);
        got.sort_by_key(|st| st.item);
        let mut want = vec![ItemStack::new(r.output, r.output_count), coal(&items, 2)];
        want.extend(r.inputs.iter().map(|i| ItemStack::new(i.items[0], i.count * 2)));
        want.sort_by_key(|st| st.item);
        assert_eq!(got, want, "one ingot made, two coal burnt, two crafts' inputs back");
        assert!(!s.has_contents(pos) && s.running().count() == 0);
        assert_eq!(s.take_dirty(&items, &recipes), vec![(pos, None)]);
        assert!(s.take(pos, &items, &recipes).is_empty());
    }

    #[test]
    fn stations_survive_a_save_with_fuel_and_heat_and_refund_vanished_recipes() {
        let (items, recipes) = setup();
        let glass = recipe_for(&items, &recipes, "glass");
        let iron = recipe_for(&items, &recipes, "iron_ingot");
        let pos = BlockPos::new(-7, 30, 9);
        let waiting = BlockPos::new(1, 30, 9);
        let mut s = Stations::new();
        s.add_fuel(pos, Station::Furnace, coal(&items, 3), 50, &items, &recipes, &Nowhere);
        s.queue(
            pos,
            Station::Furnace,
            iron,
            plain(&recipes, iron, 1),
            50,
            &items,
            &recipes,
            &Nowhere,
        )
        .unwrap();
        s.queue(
            pos,
            Station::Furnace,
            glass,
            plain(&recipes, glass, 4),
            50,
            &items,
            &recipes,
            &Nowhere,
        )
        .unwrap();
        s.queue(
            waiting,
            Station::Furnace,
            glass,
            plain(&recipes, glass, 1),
            50,
            &items,
            &recipes,
            &Nowhere,
        )
        .unwrap();
        let saved: Vec<_> = s
            .take_dirty(&items, &recipes)
            .into_iter()
            .map(|(p, st)| (p, st.unwrap()))
            .collect();
        let st = &saved.iter().find(|(p, _)| *p == pos).unwrap().1;
        assert_eq!((st.heat, st.waiting), (50, false));
        assert_eq!(st.fuel.as_ref().map(|f| (f.item.as_str(), f.count)), Some(("coal", 1)));
        let t = Stations::load(saved.clone(), &items, &recipes);
        assert_eq!(t.get(pos), s.get(pos));
        assert_eq!(t.get(waiting), s.get(waiting));
        assert!(t.get(waiting).unwrap().wait.is_some());
        assert_eq!(
            t.running().collect::<Vec<_>>(),
            vec![pos],
            "a waiting craft stays waiting"
        );

        // A recipe that changed: its inputs come back as output.
        let mut changed = saved;
        let i = changed.iter().position(|(p, _)| *p == pos).unwrap();
        changed[i].1.jobs[1].inputs = vec![("sand".into(), 3)];
        let t = Stations::load(changed, &items, &recipes);
        let st = t.get(pos).unwrap();
        assert_eq!(st.jobs.len(), 1);
        assert_eq!(st.output.count(items.id("sand").unwrap()), 12);
        assert_eq!(st.heat, 50);
    }

    #[test]
    fn queues_are_bounded() {
        let (items, recipes) = setup();
        assert!(Stations::new().has_room(BlockPos::new(0, 0, 0)));
        let glass = recipe_for(&items, &recipes, "glass");
        let pos = BlockPos::new(0, 0, 0);
        let mut s = Stations::new();
        for _ in 0..MAX_JOBS {
            s.queue(
                pos,
                Station::Furnace,
                glass,
                plain(&recipes, glass, 1),
                0,
                &items,
                &recipes,
                &Nowhere,
            )
            .unwrap();
        }
        assert!(!s.has_room(pos));
        assert_eq!(
            s.queue(
                pos,
                Station::Furnace,
                glass,
                plain(&recipes, glass, 1),
                0,
                &items,
                &recipes,
                &Nowhere
            ),
            Err(CraftError::QueueFull)
        );
    }

    #[test]
    fn a_record_moves_to_its_machines_first_block() {
        let (items, recipes) = setup();
        let stones = recipe_for(&items, &recipes, "stones");
        let (from, to) = (BlockPos::new(3, 20, 3), BlockPos::new(2, 20, 3));
        let mut s = Stations::new();
        s.queue(
            from,
            Station::Workbench,
            stones,
            plain(&recipes, stones, 1),
            0,
            &items,
            &recipes,
            &Nowhere,
        )
        .unwrap();
        s.take_dirty(&items, &recipes);
        s.relocate(from, to);
        assert!(!s.exists(from) && s.has_contents(to));
        assert_eq!(s.running().collect::<Vec<_>>(), vec![to]);
        let dirty: Vec<_> = s
            .take_dirty(&items, &recipes)
            .into_iter()
            .map(|(p, st)| (p, st.is_some()))
            .collect();
        assert_eq!(dirty, vec![(to, true), (from, false)]);
        // Never over another record.
        s.queue(
            from,
            Station::Workbench,
            stones,
            plain(&recipes, stones, 1),
            0,
            &items,
            &recipes,
            &Nowhere,
        )
        .unwrap();
        s.relocate(from, to);
        assert!(s.exists(from) && s.exists(to));
    }

    /// Surroundings for tests: every machine's links reach `grid`; furnaces
    /// burn `share` of the heat.
    struct Around {
        grid: Option<Vec<BlockPos>>,
        share: Option<f32>,
    }

    impl Surroundings for Around {
        fn generators(&self, _: BlockPos) -> Option<Vec<BlockPos>> {
            self.grid.clone()
        }
        fn heat_share(&self, _: BlockPos) -> Option<f32> {
            self.share
        }
    }

    fn linked(grid: &[BlockPos]) -> Around {
        Around {
            grid: Some(grid.to_vec()),
            share: Some(1.0),
        }
    }

    /// Runs `s` from tick `from` to `to` (inclusive).
    fn run(s: &mut Stations, from: u64, to: u64, items: &ItemRegistry, recipes: &RecipeBook, env: &dyn Surroundings) {
        for t in from..=to {
            s.step(t, items, recipes, env);
        }
    }

    /// Soft coal gives 100 spark over 10 s (tuning 4.7), one item at a time;
    /// a full store pauses the burn and loses nothing.
    #[test]
    fn generators_burn_fuel_into_spark_over_its_burn_time() {
        let (items, recipes) = setup();
        let g = BlockPos::new(0, 30, 0);
        let mut s = Stations::new();
        assert_eq!(
            s.add_fuel(
                g,
                Station::SparkGenerator,
                coal(&items, 3),
                0,
                &items,
                &recipes,
                &Nowhere
            ),
            3
        );
        s.set_generator_blocks(g, 1);
        assert!(s.running().any(|p| p == g), "burning");
        // Half the burn time: about half the spark of the first coal.
        run(&mut s, 1, 300, &items, &recipes, &Nowhere);
        let st = s.get(g).unwrap();
        assert_eq!((st.spark, st.burn_left()), (50, 50));
        assert_eq!(st.fuel.map(|f| f.count), Some(2), "one item burns at a time");
        run(&mut s, 301, 600, &items, &recipes, &Nowhere);
        assert_eq!((s.get(g).unwrap().spark, s.get(g).unwrap().burn_left()), (100, 0));
        run(&mut s, 601, 1800, &items, &recipes, &Nowhere);
        let st = s.get(g).unwrap();
        assert_eq!((st.spark, st.fuel), (300, None));
        assert!(!s.running().any(|p| p == g), "nothing left to burn");
        let v = s.view(g, Station::SparkGenerator, None, 1800, &recipes);
        assert_eq!(v.wear, None, "generators do not wear");

        // Full: the burn waits with its spark still to come.
        let mut s = Stations::new();
        s.add_fuel(
            g,
            Station::SparkGenerator,
            coal(&items, 2),
            0,
            &items,
            &recipes,
            &Nowhere,
        );
        s.map.get_mut(&g).unwrap().spark = spark::GENERATOR_CAPACITY - 30;
        run(&mut s, 1, 1200, &items, &recipes, &Nowhere);
        let st = s.get(g).unwrap();
        assert_eq!(st.spark, spark::GENERATOR_CAPACITY);
        assert_eq!(
            (st.burn_left(), st.fuel.map(|f| f.count)),
            (70, Some(1)),
            "paused, not lost"
        );
        // A second block doubles the store and the burn goes on.
        s.set_generator_blocks(g, 2);
        run(&mut s, 1201, 3000, &items, &recipes, &Nowhere);
        assert_eq!(s.get(g).unwrap().spark, spark::GENERATOR_CAPACITY + 170);
    }

    /// A spark recipe starts only when the grid pays it all, fullest first,
    /// and says why it waits; it starts by itself once spark arrives.
    #[test]
    fn spark_recipes_wait_for_the_grid_and_draw_from_the_fullest_first() {
        let (items, recipes) = setup();
        let alloy = recipe_for(&items, &recipes, "silver_alloy");
        assert_eq!(recipes.get(alloy).unwrap().spark, 100);
        let mixer = BlockPos::new(0, 30, 0);
        let (a, b) = (BlockPos::new(10, 30, 0), BlockPos::new(20, 30, 0));
        let mut s = Stations::new();
        s.queue(
            mixer,
            Station::Mixer,
            alloy,
            plain(&recipes, alloy, 3),
            0,
            &items,
            &recipes,
            &linked(&[]),
        )
        .unwrap();
        let v = s.view(mixer, Station::Mixer, None, 0, &recipes);
        assert_eq!(v.idle.as_deref(), Some(NO_SPARK));
        assert_eq!(v.jobs[0].progress, 0.0);

        // Linked to two generators holding 60 and 70: 130 in all.
        for (g, n) in [(a, 60), (b, 70)] {
            let mut st = StationData::new(Station::SparkGenerator, 0);
            st.spark = n;
            s.map.insert(g, st);
        }
        let env = linked(&[a, b]);
        run(&mut s, 1, RETRY_EVERY, &items, &recipes, &env);
        assert!(s.running().any(|p| p == mixer), "started on its retry");
        assert_eq!(
            (s.get(a).unwrap().spark, s.get(b).map_or(0, |st| st.spark)),
            (30, 0),
            "fullest first"
        );
        let ticks = craft_ticks(recipes.get(alloy).unwrap());
        // The next craft finds 30 spark: not enough.
        run(&mut s, RETRY_EVERY + 1, RETRY_EVERY + ticks, &items, &recipes, &env);
        let v = s.view(mixer, Station::Mixer, None, RETRY_EVERY + ticks, &recipes);
        assert_eq!(v.idle.as_deref(), Some("Not enough spark (needs 100)"));
        assert_eq!(v.output.len(), 1);
        assert_eq!(s.get(a).unwrap().spark, 30, "nothing taken from a grid that cannot pay");
        assert_eq!(
            v.wear,
            Some((4_000 - 12, 4_000)),
            "one craft's wear (silver alloy: 12) of a mixer's 4,000"
        );
    }

    /// Wear goes by each craft's wear; at 0 the queue stops until a spanner
    /// repairs it, which starts it again.
    #[test]
    fn machines_wear_out_and_spanners_repair_them() {
        let (items, recipes) = setup();
        let cog = recipe_for(&items, &recipes, "cog");
        let wear = recipes.get(cog).unwrap().wear as u32;
        assert!(wear > 0);
        let bench = BlockPos::new(0, 30, 0);
        let mut s = Stations::new();
        s.start_wear = wear + 1;
        assert_eq!(s.wear(bench), wear + 1, "a machine nobody used");
        s.queue(
            bench,
            Station::Workbench,
            cog,
            plain(&recipes, cog, 3),
            0,
            &items,
            &recipes,
            &Nowhere,
        )
        .unwrap();
        assert_eq!(s.wear(bench), 1);
        let ticks = craft_ticks(recipes.get(cog).unwrap());
        run(&mut s, 1, ticks, &items, &recipes, &Nowhere);
        assert_eq!(s.wear(bench), 0, "worn as the second craft starts");
        run(&mut s, ticks + 1, 2 * ticks, &items, &recipes, &Nowhere);
        let v = s.view(bench, Station::Workbench, None, 2 * ticks, &recipes);
        assert_eq!(v.idle.as_deref(), Some(WORN_OUT));
        assert_eq!((v.jobs[0].remaining, v.output[0].count), (1, 2));
        assert_eq!(v.wear, Some((0, 2_000)), "a workbench holds 2,000");
        assert_eq!(s.running().count(), 0);
        // A copper spanner's hit: 400, and the last craft starts.
        let t = 5000;
        assert_eq!(
            s.repair(bench, Station::Workbench, 400, t, &items, &recipes, &Nowhere),
            400 - wear
        );
        assert!(s.running().any(|p| p == bench));
        assert_eq!(s.get(bench).unwrap().started, t);
        // Never past full; a full machine with nothing in it keeps no record.
        let mut inv = Inventory::default();
        run(&mut s, t + 1, t + ticks, &items, &recipes, &Nowhere);
        s.collect(bench, &mut inv, &items);
        assert_eq!(
            s.repair(
                bench,
                Station::Workbench,
                2 * MACHINE_WEAR,
                t,
                &items,
                &recipes,
                &Nowhere
            ),
            2_000
        );
        assert!(!s.exists(bench));
        // A crafting table and furnaces do not wear.
        let stones = recipe_for(&items, &recipes, "stones");
        assert_eq!(recipes.get(stones).unwrap().wear, 0);
        assert_eq!(s.view(bench, Station::CraftingTable, None, 0, &recipes).wear, None);
    }

    /// A spanner hit on a machine mends its coils by the same share of
    /// their durability (Boundless since Release 235).
    #[test]
    fn a_spanner_on_the_machine_mends_its_coils_alike() {
        let (items, recipes) = setup();
        let bench = BlockPos::new(0, 30, 0);
        let coil = BlockPos::new(1, 30, 0);
        let mut s = Stations::new();
        let mut st = StationData::new(Station::Workbench, 1_000);
        st.coils.insert(coil, 5_000);
        s.map.insert(bench, st);
        // 400 is a fifth of a workbench's 2,000: a fifth of 20,000 to each coil.
        assert_eq!(
            s.repair(bench, Station::Workbench, 400, 0, &items, &recipes, &Nowhere),
            1_400
        );
        assert_eq!(s.get(bench).unwrap().coils[&coil], 9_000);
    }

    #[test]
    fn an_iron_furnace_burns_half_the_heat() {
        let (items, recipes) = setup();
        let copper = recipe_for(&items, &recipes, "copper_ingot");
        let pos = BlockPos::new(0, 30, 0);
        let mut s = Stations::new();
        let iron = Around {
            grid: None,
            share: Some(0.5),
        };
        s.add_fuel(pos, Station::Furnace, coal(&items, 1), 0, &items, &recipes, &iron);
        s.queue(
            pos,
            Station::Furnace,
            copper,
            plain(&recipes, copper, 2),
            0,
            &items,
            &recipes,
            &iron,
        )
        .unwrap();
        assert_eq!(s.get(pos).unwrap().heat, 50, "100 heat for 50");
        let ticks = craft_ticks(recipes.get(copper).unwrap());
        // Unloaded now: it remembers its base.
        let unknown = Around {
            grid: None,
            share: None,
        };
        s.step(ticks, &items, &recipes, &unknown);
        let st = s.get(pos).unwrap();
        assert_eq!((st.heat, st.wait), (0, None), "the second craft paid 50 too");
    }

    /// Joined generators add their stores up; a broken one takes its share
    /// and the rest is split by size.
    #[test]
    fn generator_groups_join_and_split_their_stores() {
        let (items, recipes) = setup();
        let (a, b, c) = (
            BlockPos::new(0, 30, 0),
            BlockPos::new(2, 30, 0),
            BlockPos::new(1, 30, 0),
        );
        let mut s = Stations::new();
        for (g, n) in [(a, 1000), (b, 2500)] {
            let mut st = StationData::new(Station::SparkGenerator, 0);
            st.spark = n;
            s.map.insert(g, st);
        }
        s.add_fuel(
            b,
            Station::SparkGenerator,
            coal(&items, 4),
            0,
            &items,
            &recipes,
            &Nowhere,
        );
        // c joins a and b into one group keyed at a.
        s.join_generators(a, &[a, c, b], 3);
        assert!(!s.exists(b));
        let st = s.get(a).unwrap();
        assert_eq!((st.spark, st.blocks, st.fuel.map(|f| f.count)), (3500, 3, Some(4)));
        assert!(s.running().any(|p| p == a), "its fuel burns on at the key");
        s.take_fuel(a, 4);
        s.step(1, &items, &recipes, &Nowhere);
        // Breaking c: it takes a third; a and b each keep half the rest.
        let spark = s.get(a).unwrap().spark;
        let share = s.break_generator(a, 3, &[(a, 1), (b, 1)]);
        assert_eq!(share, spark / 3);
        let (sa, sb) = (s.get(a).unwrap().spark, s.get(b).unwrap().spark);
        assert_eq!(sa + sb, spark - share);
        assert!(sa.abs_diff(sb) <= 1);
        assert_eq!(s.get(b).unwrap().blocks, 1);
        // The refund is 1,000 spark an item, rounded down.
        assert_eq!(share / spark::SPARK_PER_ITEM, 1);
    }

    #[test]
    fn wear_and_spark_survive_a_save() {
        let (items, recipes) = setup();
        let cog = recipe_for(&items, &recipes, "cog");
        let (bench, g) = (BlockPos::new(0, 30, 0), BlockPos::new(5, 30, 0));
        let mut s = Stations::new();
        s.start_wear = 3;
        s.queue(
            bench,
            Station::Workbench,
            cog,
            plain(&recipes, cog, 2),
            0,
            &items,
            &recipes,
            &Nowhere,
        )
        .unwrap();
        s.add_fuel(
            g,
            Station::SparkGenerator,
            coal(&items, 2),
            0,
            &items,
            &recipes,
            &Nowhere,
        );
        s.set_generator_blocks(g, 2);
        run(&mut s, 1, 10_000, &items, &recipes, &Nowhere);
        let saved: Vec<_> = s
            .take_dirty(&items, &recipes)
            .into_iter()
            .filter_map(|(p, st)| Some((p, st?)))
            .collect();
        let t = Stations::load(saved, &items, &recipes);
        assert_eq!(t.get(bench), s.get(bench));
        assert_eq!(t.get(bench).unwrap().wait, Some(Wait::Worn), "still worn out");
        assert_eq!(t.get(g), s.get(g));
        assert_eq!((t.get(g).unwrap().spark, t.get(g).unwrap().blocks), (200, 2));
    }

    fn session(items: &ItemRegistry) -> crate::forging::Forging {
        use bm_core::forge::{Forger, Gear, Session};
        let hammer = ItemStack::new(items.id("iron_hammer").unwrap(), 1);
        let deck = ["pure_tincture_1", "sealing_wax"]
            .iter()
            .map(|n| crate::forging::deck_slot(items, ItemStack::new(items.id(n).unwrap(), 5)).unwrap())
            .collect();
        let forger = Forger::default();
        let session = Session::new(hammer, Gear::Tool, forger.stats(110, 0.0), deck, 42);
        crate::forging::Forging {
            account: "alice".into(),
            name: "Alice".into(),
            session,
        }
    }

    fn reload(s: &mut Stations, items: &ItemRegistry, recipes: &RecipeBook) -> Stations {
        let saved: Vec<_> = s
            .take_dirty(items, recipes)
            .into_iter()
            .filter_map(|(p, st)| Some((p, st?)))
            .collect();
        Stations::load(saved, items, recipes)
    }

    #[test]
    fn a_forge_session_holds_the_forge_and_survives_a_save_with_its_dice() {
        let (items, recipes) = setup();
        let forge = BlockPos::new(0, 30, 0);
        let mut s = Stations::new();
        assert!(!s.has_contents(forge));
        s.begin_forge(forge, session(&items));
        assert!(s.has_contents(forge), "a session stops the forge being broken");
        // A few rounds in, so pools, traits, buffs, the dice and the last
        // report all hold something.
        for _ in 0..3 {
            let f = s.forging_mut(forge).unwrap();
            assert_eq!(f.session.use_ingredient(0), Ok(None));
        }
        let mut t = reload(&mut s, &items, &recipes);
        assert_eq!(t.get(forge), s.get(forge), "the same session, dice and all");
        // The same next round on both.
        let a = s.forging_mut(forge).unwrap().session.use_ingredient(0);
        let b = t.forging_mut(forge).unwrap().session.use_ingredient(0);
        assert_eq!(a, b);
        assert_eq!(t.forge(forge), s.forge(forge));
        assert_eq!(t.forge(forge).unwrap().session.as_ref().unwrap().session.rounds, 4);
        // Ended, the record goes (nothing else in it).
        let f = t.end_forge(forge).unwrap();
        assert_eq!(f.account, "alice");
        assert!(!t.has_contents(forge));
        assert!(t.get(forge).is_none());
    }

    #[test]
    fn a_sealed_item_waits_for_spark_then_its_time_then_comes_out_with_its_xp() {
        use bm_core::forge::{Forged, TraitId, TraitLevel, queue_band};
        let (items, recipes) = setup();
        let (forge, g) = (BlockPos::new(0, 30, 0), BlockPos::new(5, 30, 0));
        let mut s = Stations::new();
        s.forge_seconds = Some(2);
        let mut hammer = ItemStack::new(items.id("iron_hammer").unwrap(), 1);
        hammer.forge = Forged {
            done: true,
            ..Default::default()
        };
        hammer.forge.traits[0] = TraitLevel {
            id: TraitId::Quick as u8,
            level: 2,
        };
        let band = queue_band(hammer.forge.rank(30));
        assert_eq!(band.spark, 90);
        let env = linked(&[g]);
        s.seal(forge, hammer, band, 0, &items, &env);
        assert!(s.has_contents(forge), "a queued item stops the forge being broken");
        assert_eq!(s.get(forge).unwrap().wait, Some(Wait::Spark(90)), "no spark yet");
        // Saved while it waits.
        let mut s = reload(&mut s, &items, &recipes);
        s.forge_seconds = Some(2);
        s.add_fuel(g, Station::SparkGenerator, coal(&items, 1), 1, &items, &recipes, &env);
        s.set_generator_blocks(g, 4);
        run(&mut s, 1, 2000, &items, &recipes, &env);
        let st = s.get(forge).unwrap();
        assert_eq!(st.output.slots[0], Some(hammer), "out with its traits");
        assert_eq!(st.xp, band.xp);
        assert_eq!(st.wear, MACHINE_WEAR - band.wear);
        assert!(st.forge.is_none());
        let mut inv = Inventory::default();
        assert_eq!(s.collect_xp(forge, &mut inv, &items), Some(band.xp));
        assert_eq!(inv.slots[0], Some(hammer));
    }

    #[test]
    fn a_sealed_item_mid_queue_survives_a_save() {
        let (items, recipes) = setup();
        let forge = BlockPos::new(0, 30, 0);
        let mut s = Stations::new();
        let hammer = ItemStack::new(items.id("iron_hammer").unwrap(), 1);
        let band = bm_core::forge::QueueBand {
            seconds: 120,
            spark: 0,
            wear: 4,
            xp: 5,
        };
        s.seal(forge, hammer, band, 10, &items, &Nowhere);
        run(&mut s, 11, 600, &items, &recipes, &Nowhere);
        let t = reload(&mut s, &items, &recipes);
        assert_eq!(t.get(forge), s.get(forge));
        let q = t.forge(forge).unwrap().queued.as_ref().unwrap();
        assert_eq!(q.started, Some(10));
        assert_eq!(q.progress(610), (600.0 / 7200.0, 110));
        // Lapsing gives the item back.
        let mut t = t;
        assert_eq!(t.take(forge, &items, &recipes), vec![hammer]);
    }
}
