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

use crate::store::{SavedJob, SavedStack, SavedStation, load_slots, save_slots};
use bm_core::spark::{self, MACHINE_WEAR};
use bm_core::{BlockPos, Inventory, ItemRegistry, ItemStack, Recipe, RecipeBook, Station, TICK_HZ};
use bm_protocol::{JobView, StationView};
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
        }
    }
}

/// Ticks one craft of `recipe` takes (at least one).
pub fn craft_ticks(recipe: &Recipe) -> u64 {
    ((recipe.seconds as f64 * TICK_HZ as f64).round() as u64).max(1)
}

/// Takes `times` crafts' inputs from `inv`, all or nothing.
pub fn take_inputs(inv: &mut Inventory, recipe: &Recipe, times: u16) -> bool {
    let mut c = inv.clone();
    for (item, n) in &recipe.inputs {
        let Ok(total) = u16::try_from(*n as u32 * times as u32) else {
            return false;
        };
        if c.remove(*item, total).is_none() {
            return false;
        }
    }
    *inv = c;
    true
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
    if times == 0 || times > MAX_CRAFT_COUNT {
        return Err(CraftError::TooMany);
    }
    let out = u16::try_from(recipe.output_count as u32 * times as u32).map_err(|_| CraftError::TooMany)?;
    let mut c = inv.clone();
    if !take_inputs(&mut c, recipe, times) {
        return Err(CraftError::Materials);
    }
    c.add_all(items, recipe.output, out).ok_or(CraftError::NoRoom)?;
    *inv = c;
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Job {
    pub recipe: u16,
    /// Crafts still to finish, including the one in progress.
    pub remaining: u16,
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
    /// A generator: spark still to come from the item burning, and the ticks
    /// it takes to give it.
    pub burn_left: u32,
    pub burn_ticks: u32,
    /// A generator group's blocks, for its capacity.
    pub blocks: u32,
    /// What the surroundings last said: the generators a machine's links
    /// reach, a furnace's heat share (not saved; asked again).
    grid: Heard<Vec<BlockPos>>,
    heat_share: Heard<f32>,
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
            wear: if spark::wears(kind) { wear.min(MACHINE_WEAR) } else { 0 },
            spark: 0,
            burn_left: 0,
            burn_ticks: 0,
            blocks: 1,
            grid: Heard(None),
            heat_share: Heard(None),
        }
    }

    /// Nothing worth keeping a record for.
    fn is_empty(&self) -> bool {
        !self.holds_anything()
            && self.heat == 0
            && self.spark == 0
            && self.burn_left == 0
            && (!spark::wears(self.kind) || self.wear >= MACHINE_WEAR)
    }

    /// A generator group's store: the most it holds.
    pub fn capacity(&self) -> u32 {
        spark::capacity(self.blocks.max(1) as usize)
    }

    /// A generator: one tick of burning. Takes the next fuel item when the
    /// last is spent; gives nothing while the store is full. Returns whether
    /// it took an item (a change worth showing at once).
    fn burn(&mut self, items: &ItemRegistry) -> bool {
        let cap = self.capacity();
        if self.spark >= cap {
            return false;
        }
        let mut took = false;
        if self.burn_left == 0 {
            let Some(f) = &mut self.fuel else { return false };
            let value = items.get(f.item).map_or(0, |d| d.fuel);
            if value == 0 {
                return false;
            }
            f.count -= 1;
            if f.count == 0 {
                self.fuel = None;
            }
            self.burn_left = value as u32;
            self.burn_ticks = ((spark::burn_seconds(value) as f64 * TICK_HZ as f64).round() as u32).max(1);
            took = true;
        }
        // Spread evenly: what is left over the ticks left, the last tick
        // giving the rest.
        let due = self.burn_left.div_ceil(self.burn_ticks.max(1));
        let give = due.min(cap - self.spark);
        self.spark += give;
        self.burn_left -= give;
        if give == due {
            self.burn_ticks = self.burn_ticks.saturating_sub(1).max(1);
        }
        if self.burn_left == 0 {
            self.burn_ticks = 0;
        }
        took
    }

    /// Whether a generator has anything to burn.
    fn burns(&self) -> bool {
        self.kind == Station::SparkGenerator && (self.burn_left > 0 || self.fuel.is_some())
    }

    /// Jobs, output or fuel: what stops it being broken. Banked heat alone
    /// does not (a fire going out loses it): a furnace's leftover heat is
    /// rarely exactly what any recipe takes, so it would never come free.
    fn holds_anything(&self) -> bool {
        !self.jobs.is_empty() || self.output.slots.iter().any(Option::is_some) || self.fuel.is_some()
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
    /// The wear a machine without a record has (`MACHINE_WEAR`; tests set
    /// less).
    pub start_wear: u32,
}

impl Stations {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            active: BTreeSet::new(),
            burning: BTreeSet::new(),
            retry: BTreeSet::new(),
            dirty: BTreeSet::new(),
            start_wear: MACHINE_WEAR,
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
            st.heat = saved.heat;
            st.xp = saved.xp;
            if saved.kind == Station::SparkGenerator {
                st.blocks = saved.blocks.max(1);
                st.spark = saved.spark;
                st.burn_left = saved.burn_left;
                st.burn_ticks = if saved.burn_left > 0 {
                    saved.burn_ticks.max(1)
                } else {
                    0
                };
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
            for job in saved.jobs {
                match find_recipe(&job, saved.kind, items, recipes) {
                    Some(recipe) if job.remaining > 0 => st.jobs.push_back(Job {
                        recipe,
                        remaining: job.remaining,
                    }),
                    Some(_) => {}
                    None => refund(&mut st, &job, items, &what),
                }
            }
            // Why it waited is asked again: heat in a furnace, a repair when
            // worn out, else spark (tried again soon).
            st.wait = match st.jobs.front().and_then(|j| recipes.get(j.recipe)) {
                Some(_) if !saved.waiting => None,
                None => None,
                Some(_) if st.kind == Station::Furnace => Some(Wait::Heat),
                Some(_) if spark::wears(st.kind) && st.wear == 0 => Some(Wait::Worn),
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
        for set in [&mut self.active, &mut self.burning, &mut self.retry] {
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

    /// Queues `times` crafts of recipe `index` (inputs already taken). The
    /// first craft of an idle station starts at once, drawing its heat,
    /// spark and wear.
    #[allow(clippy::too_many_arguments)]
    pub fn queue(
        &mut self,
        pos: BlockPos,
        kind: Station,
        index: u16,
        times: u16,
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
        st.jobs.push_back(Job {
            recipe: index,
            remaining: times,
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
        if matches!(wait, Some(Wait::NoGrid | Wait::Spark(_))) {
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
        if recipe.spark > 0 {
            if let Some(grid) = env.generators(pos) {
                st.grid.0 = Some(grid);
            }
            let grid = st.grid.0.clone().unwrap_or_default();
            if grid.is_empty() {
                return Some(Wait::NoGrid);
            }
            if !self.draw_spark(&grid, recipe.spark) {
                return Some(Wait::Spark(recipe.spark));
            }
        }
        st.wear = st.wear.saturating_sub(recipe.wear as u32);
        None
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
        st.wear = st.wear.saturating_add(amount).min(MACHINE_WEAR);
        let wear = st.wear;
        let worn = st.wait == Some(Wait::Worn);
        let empty = st.is_empty();
        self.dirty.insert(pos);
        if worn {
            self.try_start(pos, tick, items, recipes, env);
        } else if empty {
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
            st.burn_left = st.burn_left.saturating_add(from.burn_left);
            st.burn_ticks = st.burn_ticks.max(from.burn_ticks);
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
                new.burn_left = st.burn_left;
                new.burn_ticks = st.burn_ticks;
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
            if tick < st.started + craft_ticks(recipe) {
                continue;
            }
            if st.output.add_all(items, recipe.output, recipe.output_count).is_none() {
                // Full: wait, finished, until someone collects.
                self.active.remove(&pos);
                continue;
            }
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
        changed.sort_by_key(order);
        changed.dedup();
        self.dirty.extend(changed.iter().copied());
        changed
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
    pub fn take(&mut self, pos: BlockPos, recipes: &RecipeBook) -> Vec<ItemStack> {
        let Some(st) = self.map.remove(&pos) else {
            return Vec::new();
        };
        for set in [&mut self.active, &mut self.burning, &mut self.retry] {
            set.remove(&pos);
        }
        self.dirty.insert(pos);
        let mut out: Vec<ItemStack> = st.output.slots.iter().flatten().copied().collect();
        out.extend(st.fuel);
        for job in &st.jobs {
            let Some(r) = recipes.get(job.recipe) else { continue };
            for (item, n) in &r.inputs {
                let mut total = *n as u32 * job.remaining as u32;
                while total > 0 {
                    let c = total.min(u16::MAX as u32) as u16;
                    out.push(ItemStack::new(*item, c));
                    total -= c as u32;
                }
            }
        }
        out
    }

    /// Stations whose current craft is under way, and generators burning
    /// (for progress updates).
    pub fn running(&self) -> impl Iterator<Item = BlockPos> + '_ {
        self.active.iter().chain(self.burning.iter()).copied()
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
        let wear = spark::wears(kind).then(|| (self.wear(pos), MACHINE_WEAR));
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
            };
        };
        let jobs = st
            .jobs
            .iter()
            .enumerate()
            .map(|(i, j)| {
                let progress = match (i, recipes.get(j.recipe)) {
                    (0, Some(r)) if st.wait.is_none() => {
                        (tick.saturating_sub(st.started) as f32 / craft_ticks(r) as f32).clamp(0.0, 1.0)
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
    if s.wear == 0 {
        return inv.add(items, s.item, s.count);
    }
    match inv.slots.iter_mut().find(|slot| slot.is_none()) {
        Some(slot) => {
            *slot = Some(s);
            0
        }
        None => s.count,
    }
}

fn save_station(st: &StationData, items: &ItemRegistry, recipes: &RecipeBook) -> SavedStation {
    let name = |id| items.get(id).map(|d| d.name.clone()).unwrap_or_default();
    SavedStation {
        kind: st.kind,
        jobs: st
            .jobs
            .iter()
            .filter_map(|j| {
                let r = recipes.get(j.recipe)?;
                Some(SavedJob {
                    output: name(r.output),
                    output_count: r.output_count,
                    inputs: r.inputs.iter().map(|(i, n)| (name(*i), *n)).collect(),
                    remaining: j.remaining,
                })
            })
            .collect(),
        started: st.started,
        output: save_slots(items, &st.output),
        fuel: st.fuel.as_ref().and_then(|f| SavedStack::new(items, f)),
        heat: st.heat,
        waiting: st.wait.is_some(),
        xp: st.xp,
        wear: st.wear,
        spark: st.spark,
        burn_left: st.burn_left,
        burn_ticks: st.burn_ticks,
        blocks: st.blocks,
    }
}

/// The recipe a saved job was: same station, output and inputs.
fn find_recipe(job: &SavedJob, kind: Station, items: &ItemRegistry, recipes: &RecipeBook) -> Option<u16> {
    let output = items.id(&job.output).ok()?;
    let inputs = job
        .inputs
        .iter()
        .map(|(n, c)| Some((items.id(n).ok()?, *c)))
        .collect::<Option<Vec<_>>>()?;
    recipes
        .at(kind)
        .find(|(_, r)| r.output == output && r.output_count == job.output_count && r.inputs == inputs)
        .map(|(i, _)| i)
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
        s.queue(pos, Station::CraftingTable, stones, 3, 100, &items, &recipes, &Nowhere)
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
        s.queue(pos, Station::Furnace, copper, 2, 10, &items, &recipes, &Nowhere)
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
        s.queue(pos, Station::Furnace, iron, 1, 9000, &items, &recipes, &Nowhere)
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
        s.queue(pos, Station::Furnace, iron, 1, 0, &items, &recipes, &Nowhere)
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
        s.queue(pos, Station::Furnace, ingot, 3, 0, &items, &recipes, &Nowhere)
            .unwrap();
        s.step(ticks, &items, &recipes, &Nowhere);
        s.take_dirty(&items, &recipes);
        let mut got = s.take(pos, &recipes);
        got.sort_by_key(|st| st.item);
        let mut want = vec![ItemStack::new(r.output, r.output_count), coal(&items, 2)];
        want.extend(r.inputs.iter().map(|(i, n)| ItemStack::new(*i, n * 2)));
        want.sort_by_key(|st| st.item);
        assert_eq!(got, want, "one ingot made, two coal burnt, two crafts' inputs back");
        assert!(!s.has_contents(pos) && s.running().count() == 0);
        assert_eq!(s.take_dirty(&items, &recipes), vec![(pos, None)]);
        assert!(s.take(pos, &recipes).is_empty());
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
        s.queue(pos, Station::Furnace, iron, 1, 50, &items, &recipes, &Nowhere)
            .unwrap();
        s.queue(pos, Station::Furnace, glass, 4, 50, &items, &recipes, &Nowhere)
            .unwrap();
        s.queue(waiting, Station::Furnace, glass, 1, 50, &items, &recipes, &Nowhere)
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
            s.queue(pos, Station::Furnace, glass, 1, 0, &items, &recipes, &Nowhere)
                .unwrap();
        }
        assert!(!s.has_room(pos));
        assert_eq!(
            s.queue(pos, Station::Furnace, glass, 1, 0, &items, &recipes, &Nowhere),
            Err(CraftError::QueueFull)
        );
    }

    #[test]
    fn a_record_moves_to_its_machines_first_block() {
        let (items, recipes) = setup();
        let stones = recipe_for(&items, &recipes, "stones");
        let (from, to) = (BlockPos::new(3, 20, 3), BlockPos::new(2, 20, 3));
        let mut s = Stations::new();
        s.queue(from, Station::Workbench, stones, 1, 0, &items, &recipes, &Nowhere)
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
        s.queue(from, Station::Workbench, stones, 1, 0, &items, &recipes, &Nowhere)
            .unwrap();
        s.relocate(from, to);
        assert!(s.exists(from) && s.exists(to));
    }
}
