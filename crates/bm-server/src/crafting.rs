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
//! A station has data only while it holds jobs, output, fuel or banked heat;
//! jobs, output and fuel stop it being broken.
//!
//! Hand recipes are instant wherever they are asked for, a crafting table
//! included (Boundless queued them at a table; instant keeps the hand and the
//! table the same and costs nothing).

use crate::store::{SavedJob, SavedStack, SavedStation, load_slots, save_slots};
use bm_core::{BlockPos, Inventory, ItemRegistry, ItemStack, Recipe, RecipeBook, Station, TICK_HZ};
use bm_protocol::{JobView, StationView};
use std::collections::{BTreeSet, HashMap, VecDeque};

/// Stacks of finished items a station holds.
pub const OUTPUT_STACKS: usize = 10;
/// Jobs one station queues at most.
pub const MAX_JOBS: usize = 8;
/// Crafts one request may ask for.
pub const MAX_CRAFT_COUNT: u16 = 100;

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
    /// The head job's current craft is waiting for heat: it has not started
    /// (no progress) and starts when fuel arrives.
    pub waiting: bool,
    /// Craft XP of the finished output (milestone 12): each finished craft
    /// adds its recipe's `xp`, and whoever next collects output gets it all.
    pub xp: u32,
}

impl StationData {
    fn new(kind: Station) -> Self {
        Self {
            kind,
            jobs: VecDeque::new(),
            started: 0,
            output: Inventory {
                slots: vec![None; OUTPUT_STACKS],
            },
            fuel: None,
            heat: 0,
            waiting: false,
            xp: 0,
        }
    }

    /// Nothing worth keeping a record for.
    fn is_empty(&self) -> bool {
        !self.holds_anything() && self.heat == 0
    }

    /// Jobs, output or fuel: what stops it being broken. Banked heat alone
    /// does not (a fire going out loses it): a furnace's leftover heat is
    /// rarely exactly what any recipe takes, so it would never come free.
    fn holds_anything(&self) -> bool {
        !self.jobs.is_empty() || self.output.slots.iter().any(Option::is_some) || self.fuel.is_some()
    }

    /// Starts the head job's next craft at `tick`: it draws its recipe's
    /// heat from the bank, burning fuel from the slot one item at a time as
    /// needed. With not enough heat it waits (and keeps what it burnt
    /// banked). Returns whether the craft is running.
    fn start_craft(&mut self, tick: u64, items: &ItemRegistry, recipes: &RecipeBook) -> bool {
        self.started = tick;
        let need = self
            .jobs
            .front()
            .and_then(|j| recipes.get(j.recipe))
            .map_or(0, |r| r.heat as u32);
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
        self.waiting = self.heat < need;
        if !self.waiting {
            self.heat -= need;
        }
        !self.waiting
    }
}

pub struct Stations {
    map: HashMap<BlockPos, StationData>,
    /// Stations whose head job is running (not finished-and-waiting for
    /// room, nor waiting for heat).
    active: BTreeSet<BlockPos>,
    /// Changed since the last save.
    dirty: BTreeSet<BlockPos>,
}

impl Stations {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            active: BTreeSet::new(),
            dirty: BTreeSet::new(),
        }
    }

    /// Rebuilds from the save. A job whose recipe no longer exists is
    /// refunded into the station's output, so a recipe change never eats
    /// materials.
    pub fn load(saved: Vec<(BlockPos, SavedStation)>, items: &ItemRegistry, recipes: &RecipeBook) -> Self {
        let mut s = Self::new();
        for (pos, saved) in saved {
            let mut st = StationData::new(saved.kind);
            st.started = saved.started;
            st.heat = saved.heat;
            st.xp = saved.xp;
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
            st.waiting = saved.waiting && !st.jobs.is_empty();
            if st.is_empty() {
                s.dirty.insert(pos);
                continue;
            }
            if !st.jobs.is_empty() && !st.waiting {
                s.active.insert(pos);
            }
            s.map.insert(pos, st);
        }
        s
    }

    pub fn get(&self, pos: BlockPos) -> Option<&StationData> {
        self.map.get(&pos)
    }

    /// Whether a record is kept at `pos` (contents, or banked heat).
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
        if self.active.remove(&from) {
            self.active.insert(to);
        }
        self.map.insert(to, st);
        self.dirty.insert(from);
        self.dirty.insert(to);
    }

    /// Queues `times` crafts of recipe `index` (inputs already taken). The
    /// first craft of an idle station starts at once, drawing its heat.
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
    ) -> Result<(), CraftError> {
        let st = self.map.entry(pos).or_insert_with(|| StationData::new(kind));
        if st.jobs.len() >= MAX_JOBS {
            return Err(CraftError::QueueFull);
        }
        st.jobs.push_back(Job {
            recipe: index,
            remaining: times,
        });
        if st.jobs.len() == 1 && st.start_craft(tick, items, recipes) {
            self.active.insert(pos);
        }
        self.dirty.insert(pos);
        Ok(())
    }

    /// Whether a queue of this station has room for one more job.
    pub fn has_room(&self, pos: BlockPos) -> bool {
        self.map.get(&pos).is_none_or(|s| s.jobs.len() < MAX_JOBS)
    }

    /// Puts up to `stack.count` of `stack` into the fuel slot of the station
    /// at `pos` (merging with what is there, up to a stack). Returns how many
    /// went in. A craft waiting for heat starts if it now has enough.
    pub fn add_fuel(
        &mut self,
        pos: BlockPos,
        kind: Station,
        stack: ItemStack,
        tick: u64,
        items: &ItemRegistry,
        recipes: &RecipeBook,
    ) -> u16 {
        let Some(def) = items.get(stack.item).filter(|d| d.fuel > 0) else {
            return 0;
        };
        let st = self.map.entry(pos).or_insert_with(|| StationData::new(kind));
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
            if st.waiting && st.start_craft(tick, items, recipes) {
                self.active.insert(pos);
            }
        } else if st.is_empty() {
            self.map.remove(&pos);
        }
        put
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
        let st = self.map.entry(pos).or_insert_with(|| StationData::new(kind));
        match &mut st.fuel {
            Some(f) => f.count = f.count.saturating_add(stack.count),
            None => st.fuel = Some(stack),
        }
        self.dirty.insert(pos);
    }

    /// Finishes every craft due by `tick`. Returns the stations that changed.
    pub fn step(&mut self, tick: u64, items: &ItemRegistry, recipes: &RecipeBook) -> Vec<BlockPos> {
        let mut changed = Vec::new();
        let mut idle = Vec::new();
        for &pos in &self.active {
            let Some(st) = self.map.get_mut(&pos) else {
                idle.push(pos);
                continue;
            };
            if st.waiting {
                idle.push(pos);
                continue;
            }
            let Some(job) = st.jobs.front_mut() else {
                idle.push(pos);
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
                idle.push(pos);
                continue;
            }
            job.remaining -= 1;
            st.xp = st.xp.saturating_add(recipe.xp as u32);
            if job.remaining == 0 {
                st.jobs.pop_front();
            }
            if st.jobs.is_empty() || !st.start_craft(tick, items, recipes) {
                idle.push(pos);
            }
            changed.push(pos);
        }
        for pos in idle {
            self.active.remove(&pos);
        }
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
            if !st.jobs.is_empty() && !st.waiting {
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
    /// way too). Banked heat is lost. Used when its claim lapses.
    pub fn take(&mut self, pos: BlockPos, recipes: &RecipeBook) -> Vec<ItemStack> {
        let Some(st) = self.map.remove(&pos) else {
            return Vec::new();
        };
        self.active.remove(&pos);
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

    /// Stations whose current craft is under way (for progress updates).
    pub fn running(&self) -> impl Iterator<Item = BlockPos> + '_ {
        self.active.iter().copied()
    }

    /// What a player sees. `kind` is the machine's station kind, used for an
    /// empty station; `idle` why the machine does not work, if it does not.
    pub fn view(
        &self,
        pos: BlockPos,
        kind: Station,
        idle: Option<String>,
        tick: u64,
        recipes: &RecipeBook,
    ) -> StationView {
        let Some(st) = self.map.get(&pos) else {
            return StationView {
                pos,
                station: kind,
                jobs: Vec::new(),
                output: Vec::new(),
                idle,
                fuel: None,
                heat: 0,
            };
        };
        let jobs = st
            .jobs
            .iter()
            .enumerate()
            .map(|(i, j)| {
                let progress = match (i, recipes.get(j.recipe)) {
                    (0, Some(r)) if !st.waiting => {
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
            idle,
            fuel: st.fuel,
            heat: st.heat,
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
        waiting: st.waiting,
        xp: st.xp,
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
        s.queue(pos, Station::CraftingTable, stones, 3, 100, &items, &recipes)
            .unwrap();
        assert!(s.step(100 + ticks - 1, &items, &recipes).is_empty());
        let v = s.view(pos, Station::CraftingTable, None, 100 + ticks / 2, &recipes);
        assert!((v.jobs[0].progress - 0.5).abs() < 0.02);
        assert_eq!((v.fuel, v.heat, v.idle), (None, 0, None));
        assert_eq!(s.step(100 + ticks, &items, &recipes), vec![pos]);
        let v = s.view(pos, Station::CraftingTable, None, 100 + ticks, &recipes);
        assert_eq!((v.jobs[0].remaining, v.output.len()), (2, 1));
        assert_eq!(v.output[0].count, 2);

        // Fill the output with other things: the next craft waits.
        let st = s.map.get_mut(&pos).unwrap();
        for slot in st.output.slots.iter_mut().skip(1) {
            *slot = Some(ItemStack::new(items.id("glass").unwrap(), 100));
        }
        st.output.slots[0] = Some(ItemStack::new(items.id("stones").unwrap(), 100));
        assert!(s.step(100 + 2 * ticks, &items, &recipes).is_empty());
        assert_eq!(s.running().count(), 0, "paused");
        assert_eq!(s.get(pos).unwrap().jobs[0].remaining, 2);

        // Collecting makes room and the waiting craft finishes at once.
        let mut inv = Inventory::default();
        assert!(s.collect(pos, &mut inv, &items));
        assert_eq!(inv.count(items.id("stones").unwrap()), 100);
        assert_eq!(s.step(100 + 5 * ticks, &items, &recipes), vec![pos]);
        assert_eq!(s.get(pos).unwrap().jobs[0].remaining, 1);
        assert_eq!(s.step(100 + 6 * ticks, &items, &recipes), vec![pos]);
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
        s.queue(pos, Station::Furnace, copper, 2, 10, &items, &recipes).unwrap();
        assert_eq!(s.running().count(), 0, "no heat: it waits");
        assert!(s.step(10 + 2 * ticks, &items, &recipes).is_empty());
        let v = s.view(pos, Station::Furnace, None, 10 + ticks, &recipes);
        assert_eq!(v.jobs[0].progress, 0.0, "no progress while it waits");

        // One coal: the first craft starts now, the bank spent.
        let t = 1000;
        assert_eq!(
            s.add_fuel(pos, Station::Furnace, coal(&items, 1), t, &items, &recipes),
            1
        );
        let st = s.get(pos).unwrap();
        assert_eq!((st.fuel, st.heat, st.waiting, st.started), (None, 0, false, t));
        assert_eq!(s.running().collect::<Vec<_>>(), vec![pos]);
        let v = s.view(pos, Station::Furnace, None, t + ticks / 2, &recipes);
        assert!((v.jobs[0].progress - 0.5).abs() < 0.02);
        // It finishes; the second has nothing to burn and waits.
        assert_eq!(s.step(t + ticks, &items, &recipes), vec![pos]);
        let st = s.get(pos).unwrap();
        assert!(st.waiting && st.jobs[0].remaining == 1);
        assert_eq!(st.output.count(items.id("copper_ingot").unwrap()), 1);
        assert_eq!(s.running().count(), 0);
        // Five more: one burns, four wait in the slot.
        let t = 5000;
        assert_eq!(
            s.add_fuel(pos, Station::Furnace, coal(&items, 5), t, &items, &recipes),
            5
        );
        let st = s.get(pos).unwrap();
        assert_eq!((st.fuel.map(|f| f.count), st.heat, st.waiting), (Some(4), 0, false));
        assert_eq!(s.step(t + ticks, &items, &recipes), vec![pos]);
        assert!(s.get(pos).unwrap().jobs.is_empty());

        // An iron bar burns two coal and banks the 50 left over.
        s.queue(pos, Station::Furnace, iron, 1, 9000, &items, &recipes).unwrap();
        let st = s.get(pos).unwrap();
        assert_eq!((st.fuel.map(|f| f.count), st.heat, st.waiting), (Some(2), 50, false));
        let v = s.view(pos, Station::Furnace, None, 9000, &recipes);
        assert_eq!((v.fuel.map(|f| f.count), v.heat), (Some(2), 50));
    }

    #[test]
    fn the_fuel_slot_takes_one_stack_of_something_that_burns() {
        let (items, recipes) = setup();
        let pos = BlockPos::new(5, 30, 5);
        let mut s = Stations::new();
        let stone = ItemStack::new(items.id("stone").unwrap(), 3);
        assert_eq!(s.add_fuel(pos, Station::Furnace, stone, 0, &items, &recipes), 0);
        assert!(!s.exists(pos), "nothing kept for a refusal");
        let cap = items.get(items.id("coal").unwrap()).unwrap().stack;
        assert_eq!(
            s.add_fuel(pos, Station::Furnace, coal(&items, cap - 2), 0, &items, &recipes),
            cap - 2
        );
        assert_eq!(
            s.add_fuel(pos, Station::Furnace, coal(&items, 5), 0, &items, &recipes),
            2,
            "up to a stack"
        );
        let stick = ItemStack::new(items.id("stick").unwrap(), 4);
        assert_eq!(
            s.add_fuel(pos, Station::Furnace, stick, 0, &items, &recipes),
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
        s.queue(pos, Station::Furnace, iron, 1, 0, &items, &recipes).unwrap();
        s.step(10_000, &items, &recipes);
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
        s.add_fuel(pos, Station::Furnace, coal(&items, 4), 0, &items, &recipes);
        s.queue(pos, Station::Furnace, ingot, 3, 0, &items, &recipes).unwrap();
        s.step(ticks, &items, &recipes);
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
        s.add_fuel(pos, Station::Furnace, coal(&items, 3), 50, &items, &recipes);
        s.queue(pos, Station::Furnace, iron, 1, 50, &items, &recipes).unwrap();
        s.queue(pos, Station::Furnace, glass, 4, 50, &items, &recipes).unwrap();
        s.queue(waiting, Station::Furnace, glass, 1, 50, &items, &recipes)
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
        assert!(t.get(waiting).unwrap().waiting);
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
            s.queue(pos, Station::Furnace, glass, 1, 0, &items, &recipes).unwrap();
        }
        assert!(!s.has_room(pos));
        assert_eq!(
            s.queue(pos, Station::Furnace, glass, 1, 0, &items, &recipes),
            Err(CraftError::QueueFull)
        );
    }

    #[test]
    fn a_record_moves_to_its_machines_first_block() {
        let (items, recipes) = setup();
        let stones = recipe_for(&items, &recipes, "stones");
        let (from, to) = (BlockPos::new(3, 20, 3), BlockPos::new(2, 20, 3));
        let mut s = Stations::new();
        s.queue(from, Station::Workbench, stones, 1, 0, &items, &recipes)
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
        s.queue(from, Station::Workbench, stones, 1, 0, &items, &recipes)
            .unwrap();
        s.relocate(from, to);
        assert!(s.exists(from) && s.exists(to));
    }
}
