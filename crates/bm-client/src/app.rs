//! The client application: window events, the fixed-rate simulation, and the
//! glue between network, world, prediction, menus and renderer.

use crate::chat::{ChatAction, ChatBox};
use crate::claims::{Claims, outline};
use crate::combat::{Anchor, Defeat, FireMode, Floaters, Fuse, Losses, Shots, Trigger};
use crate::creatures::{Creatures, Seen};
use crate::forge_screen::{ForgeUi, ForgeWindow};
use crate::icons::icon_for;
use crate::items::Items;
use crate::menus::{
    self, Arrival, BlockBar, CraftingView, Drag, Hud, ItemIcons, LiveStation, LoginForm, Menu, Notices, PortalWindow,
    ShrineTab, StorageWindow, TravelView, UiAction,
};
use crate::mining::{BlockHit, DamageMap, SwingInput, Swinger, predict_swing, skilled_tool};
use crate::net::{self, ConnectOptions, Link, NetEvent};
use crate::predict::{Predictor, RemoteSample, Remotes};
use crate::render::{BoxInstance, Camera, EguiFrame, LineVertex, Renderer, Scene};
use crate::settings::Settings;
use crate::skill_screen::{SkillUi, SkillView};
use crate::skills::Progress;
use crate::sky::Sky;
use crate::textures::TextureSet;
use crate::world::ClientWorld;
use bm_core::forge::Held;
use bm_core::mining::swing_ticks;
use bm_core::movement::{MoveConfig, PlayerInput};
use bm_core::{
    BlockId, BlockPos, BlockRegistry, BlockState, Chunk, Colour, DayCycle, HOTBAR_SLOTS, ItemId, ItemStack, Mount,
    PlanetLook, PlanetShape, PlotPos, RayHit, Shape, TICK_DT, raycast, raycast_shaped,
};
use bm_protocol::{
    ClientDatagram, ClientMsg, EditAction, INPUT_REDUNDANCY, MIN_PASSWORD_LEN, PlanetInfo, PlanetSummary, PortalView,
    ServerDatagram, ServerMsg, StorageView, Target, Travel,
};
use glam::{DVec3, Vec3};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Fullscreen, Window, WindowId};

/// Said when the break button is pressed with no tool in hand (the server
/// says the same if a swing reaches it).
/// How often the crop under the crosshair is asked about again (its time to
/// ripe counts down between answers).
const CROP_ASK_SECONDS: f64 = 5.0;

/// A crop as the server described it (milestone 16, `ServerMsg::CropInfo`).
#[derive(Clone, Debug, PartialEq)]
pub struct CropView {
    pub pos: BlockPos,
    pub name: String,
    pub stage: u8,
    pub stages: u8,
    pub ripe_in: u32,
    pub wild: bool,
    pub tilled: bool,
    pub dry: bool,
}

impl CropView {
    /// One line under the crosshair, `elapsed` seconds after the answer:
    /// "Oats: ripe in 1 h 20 min, tilled".
    pub fn line(&self, elapsed: f64) -> String {
        let left = (self.ripe_in as f64 - elapsed).max(0.0) as u32;
        let mut text = if self.wild {
            format!("Wild {}", self.name)
        } else {
            self.name.clone()
        };
        if self.stage >= self.stages || (!self.wild && left == 0) {
            text += ": ripe, harvest it";
        } else if self.wild {
            text += &format!(": growing ({} of {})", self.stage, self.stages);
        } else if left >= 3600 {
            text += &format!(": ripe in {} h {} min", left / 3600, left % 3600 / 60);
        } else {
            text += &format!(": ripe in {} min", left.div_ceil(60));
        }
        if self.tilled {
            text += ", tilled";
        }
        if self.dry {
            text += ", needs water beside it to grow well";
        }
        text
    }
}

/// Creative mode swings this often while the button is held (ticks).
const CREATIVE_SWING_TICKS: u32 = 8;
const NEED_A_TOOL: &str = "You need a tool to break blocks. A hand stone needs nothing to craft";

/// Said when attacking a creature with nothing to fight with.
const NEED_A_WEAPON: &str = "Hold a tool or a sling to fight";
/// `--demo-ui bombs` holds the attack button this long out of every
/// `DEMO_BOMB_CYCLE` seconds: a bomb lit, held, thrown, and the blast.
const DEMO_BOMB_HOLD: f64 = 2.4;
const DEMO_BOMB_CYCLE: f64 = 4.2;
/// Creatures are aimed at this far (when nearer than the terrain behind).
const CREATURE_AIM: f64 = 40.0;
/// Where the eye sits above the feet while down (defeated).
const DOWN_EYE: f64 = 0.35;
/// A held sling is drawn at this scale.
const HELD_SLING_SCALE: f32 = 0.7;

const MOUSE_SENSITIVITY: f64 = 0.0022;
/// Seconds between repeated places while the button is held.
const PLACE_REPEAT: f64 = 0.22;
const MAX_TICKS_PER_FRAME: u32 = 8;
/// How long a machine's wear shows under the crosshair after a spanner hit.
const WEAR_BAR_SECONDS: f64 = 2.5;
const PLAYER_VISUAL_HEIGHT: f32 = 2.0;
/// The camera's vertical field of view.
const FOV_Y: f32 = 75.0 * std::f32::consts::PI / 180.0;
/// Claim mode aims this far at terrain, and this far into open air.
const CLAIM_AIM: f64 = 48.0;
const CLAIM_AIM_AIR: f64 = 10.0;
/// Claims with a plot this close (horizontally, blocks) are outlined.
const CLAIM_DRAW_RADIUS: f64 = 128.0;
/// Outline colours, linear RGBA.
const OWN_LINE: [f32; 4] = [1.0, 0.55, 0.10, 0.95];
const OTHER_LINE: [f32; 4] = [0.95, 0.10, 0.06, 0.95];
const TARGET_LINE: [f32; 4] = [0.95, 0.95, 0.88, 1.0];
/// Chisel previews: what a left click cuts, what a right click fills back.
const CUT_LINE: [f32; 4] = [1.0, 0.82, 0.18, 1.0];
const FILL_LINE: [f32; 4] = [0.45, 0.85, 1.0, 0.9];
/// Grapple rope and hook (linear RGBA).
const ROPE_LINE: [f32; 4] = [0.30, 0.20, 0.10, 1.0];
const HOOK_COLOUR: [f32; 4] = [0.22, 0.22, 0.25, 1.0];
const XRAY_ALPHA: f32 = 0.22;
/// The travelling screen stays at least this long (seconds), so a quick
/// arrival reads as a journey rather than a flicker, then fades out.
const ARRIVAL_MIN: f64 = 0.8;
const ARRIVAL_FADE: f64 = 0.5;
/// The chat box's bottom edge sits this far up, clear of the hotbar's row.
const CHAT_BOTTOM: f32 = 96.0;

pub struct Options {
    /// Server address, name and password from the command line; with a
    /// password the login screen is skipped.
    pub server: String,
    pub name: Option<String>,
    pub password: Option<String>,
    pub register: bool,
    pub cert: PathBuf,
    pub vsync: bool,
    /// The folder holding `textures/` (normally `assets`).
    pub assets: PathBuf,
    pub settings_path: PathBuf,
    /// `--demo-ui SCREEN`: run offline against `demo.rs`, opening SCREEN
    /// (`hud`, `inventory`, `crafting`, `station`, `claims`, `shrine`,
    /// `travel`, `travelling`, `sky`, `storage`, `chat`, `colours`,
    /// `coloured-icons`, `reclaim`, `shapes`, `chisel`, `grapple`, `portal`,
    /// `mining`, `torches`, `table`, `furnace`, `extractor`, `flora`, `textures`, `meteor`,
    /// `creatures`, `defeat`, `bombs`, `spark`, `skills` or `login`).
    pub demo: Option<String>,
}

/// The journey to another planet, shown until its ground has arrived.
struct Journey {
    to: Arrival,
    started: f64,
    /// When the chunks around us had arrived; the screen fades from here.
    landed: Option<f64>,
}

/// Everything that exists once the server has welcomed us.
/// The feats (milestone 24), the same file the server reads.
static FEAT_BOOK: std::sync::LazyLock<bm_core::feats::FeatBook> = std::sync::LazyLock::new(|| {
    bm_core::feats::FeatBook::parse(include_str!("../../../assets/feats.toml")).expect("assets/feats.toml")
});

struct Session {
    /// The planet we are on: everything in `world`, `claims` and `remotes`
    /// belongs to it.
    planet: PlanetInfo,
    /// Every planet on the server, for the travel menu and the sky.
    universe: Vec<PlanetSummary>,
    /// The colour chosen for each tinted recipe (milestone 22).
    craft_colours: HashMap<u16, bm_core::Colour>,
    /// Coin and feats, once the server has sent them (milestone 24).
    ledger: Option<bm_core::feats::Ledger>,
    /// Friends and requests (milestone 25).
    friends: bm_protocol::FriendsView,
    /// Our guilds (milestone 25 part 2).
    guilds: bm_protocol::GuildsView,
    account: String,
    /// Ours, from the welcome: critical hits and random areas hash it.
    player_id: u32,
    /// Server tick in the welcome and our clock when it arrived, for the time
    /// of day before the first snapshot.
    welcome_tick: u64,
    welcome_local: f64,
    movement: MoveConfig,
    /// The server's reach for an untrained character; Grasp adds to it
    /// (`Session::reach`).
    base_reach: f64,
    view_radius: u8,
    /// Far views (milestone 20): chunks out to here are drawn coarser.
    far_radius: u8,
    far: crate::far::FarWorld,
    /// Level, XP, marks and skill pages: the server's word and our
    /// prediction (milestone 12).
    progress: Progress,
    /// Written signs by anchor (milestone 22 part 4).
    signs: crate::signs::Signs,
    world: ClientWorld,
    me: Predictor,
    remotes: Remotes,
    /// Each remote player's held light (`RemotePlayer::light`), from the
    /// latest snapshot.
    remote_lights: HashMap<u32, [u8; 3]>,
    items: Items,
    icons: ItemIcons,
    claims: Claims,
    plot_allowance: u32,
    /// Plots bought with marks when the allowance above was said: plots
    /// bought since add to it (milestone 12).
    plots_bought_then: u32,
    /// Hotbar slot, 0 to 9.
    /// The hotbar slot in the left hand (the left button's).
    selected: usize,
    /// The hotbar slot in the right hand (milestone 28; the right button's,
    /// chosen with Alt).
    selected_off: usize,
    edit_seq: u32,
    place_cooldown: f64,
    /// Builder's Buff (a guild buff): percent faster block placing.
    place_percent: u16,
    target: Option<RayHit>,
    /// What E acts on: as `target`, but portal surfaces (not solid) stop the
    /// ray too. Never used for breaking or placing.
    use_target: Option<RayHit>,
    /// With a chisel in hand: the aimed block and what each click would do.
    chisel: Option<(BlockPos, crate::chisel::Plan)>,
    /// The plot claim mode points at.
    claim_target: Option<PlotPos>,
    /// When our next swing may land (milestone 9, `crate::mining`).
    swinger: Swinger,
    /// Told "you need a tool" for the current press of the break button.
    told_no_tool: bool,
    /// Damage blocks have taken, ours predicted and the server's word.
    damage: DamageMap,
    /// The last spanner hit's answer (milestone 15): the machine's wear
    /// left, the most it holds, and when it came (`App::now`), for the bar
    /// under the crosshair.
    machine_wear: Option<((u32, u32), f64)>,
    /// The crop last asked about and when (milestone 16), and what the
    /// server said of it, with when.
    crop_asked: Option<(BlockPos, f64)>,
    crop_info: Option<(CropView, f64)>,
    warp_shrine: BlockId,
    portal_conduit: BlockId,
    portal_surface: BlockId,
    /// Kept to repaint item icons in a new planet's tints.
    textures: TextureSet,
    journey: Option<Journey>,
    /// What waits in this account's reclaim (`ServerMsg::Reclaim`).
    reclaim: Vec<ItemStack>,
    /// Creatures the server told us of (milestone 13), and where each is
    /// drawn this frame.
    creatures: Creatures,
    seen: Vec<Seen>,
    /// The creature (or body) aimed at and how far: within `CREATURE_AIM`,
    /// nearer than the first solid block.
    aim_creature: Option<(u32, f64)>,
    /// The sling's trigger, on the swing cooldown (`swinger`).
    trigger: Trigger,
    /// A bag of bombs' trigger, on the same cooldown.
    fuse: Fuse,
    /// Blasts and beams being drawn.
    fx: crate::effects::Effects,
    /// Regions and meteors on this planet (milestone 14).
    sky_view: crate::meteors::Sky,
    /// Shots in the air, ours and others'.
    shots: Shots,
    /// Damage numbers floating up.
    floaters: Floaters,
    /// Matches health lost with the server's `Hit`s (falls get a number of
    /// their own).
    losses: Losses,
    /// When we last lost health (seconds), for the red flash.
    hurt_at: f64,
    last_hurt: u32,
    defeat: Defeat,
    /// A body's loot was asked for on this press of the attack button.
    looted_this_press: bool,
    /// Told "hold a tool or a sling" for this press.
    told_no_weapon: bool,
    /// The server tick the remote lights were last said for: parts of one
    /// tick add to them, a newer tick replaces them.
    lights_tick: u64,
}

impl Session {
    #[allow(clippy::too_many_arguments)]
    fn new(
        w: bm_protocol::Welcome,
        registry: BlockRegistry,
        items: Items,
        textures: TextureSet,
        icons: ItemIcons,
        local_ticks: f64,
    ) -> Result<Self, String> {
        let warp_shrine = registry
            .id("warp_shrine")
            .map_err(|e| format!("the server sent a block catalogue without a warp shrine: {e}"))?;
        let portal = |name: &str| {
            registry
                .id(name)
                .map_err(|e| format!("the server sent a block catalogue without portals: {e}"))
        };
        let (portal_conduit, portal_surface) = (portal("portal_conduit")?, portal("portal_surface")?);
        let tree = bm_core::skills::SkillTree::from_defs(w.skills)
            .map_err(|e| format!("the server sent a bad skill tree: {e}"))?;
        let shape = w.planet.shape;
        let bought = w.character.plots_bought;
        let hurt = w.spawn.hurt;
        let mut losses = Losses::default();
        losses.reset(hurt);
        Ok(Self {
            creatures: Creatures::new(w.creatures),
            seen: Vec::new(),
            aim_creature: None,
            trigger: Trigger::default(),
            fuse: Fuse::default(),
            fx: crate::effects::Effects::default(),
            sky_view: crate::meteors::Sky::default(),
            shots: Shots::default(),
            floaters: Floaters::default(),
            losses,
            hurt_at: f64::NEG_INFINITY,
            last_hurt: hurt,
            defeat: Defeat::default(),
            looted_this_press: false,
            told_no_weapon: false,
            lights_tick: 0,
            account: w.account,
            player_id: w.player_id,
            welcome_tick: w.tick,
            welcome_local: local_ticks,
            movement: w.movement,
            base_reach: w.reach,
            view_radius: w.view_radius,
            far_radius: w.far_radius,
            far: crate::far::FarWorld::new(shape, registry.flags()).with_colours(&w.colours),
            progress: Progress::new(tree, w.character),
            claims: Claims::new(shape, w.claims),
            signs: crate::signs::Signs::default(),
            world: ClientWorld::new(shape, registry, &w.colours),
            me: Predictor::new(w.spawn),
            remotes: Remotes::default(),
            remote_lights: HashMap::new(),
            items,
            icons,
            plot_allowance: w.plot_allowance,
            plots_bought_then: bought,
            selected: 0,
            selected_off: 1,
            edit_seq: 0,
            place_cooldown: 0.0,
            place_percent: 0,
            target: None,
            use_target: None,
            chisel: None,
            claim_target: None,
            swinger: Swinger::default(),
            told_no_tool: false,
            damage: DamageMap::default(),
            machine_wear: None,
            crop_asked: None,
            crop_info: None,
            warp_shrine,
            portal_conduit,
            portal_surface,
            textures,
            journey: None,
            reclaim: Vec::new(),
            planet: w.planet,
            universe: w.universe,
            craft_colours: HashMap::new(),
            ledger: None,
            friends: Default::default(),
            guilds: Default::default(),
        })
    }

    /// Moves us to another planet. Everything of the old one goes: chunks,
    /// meshes in flight (the new world has its own mesher), light, edits
    /// waiting for an answer, remote players, claims, aim and block damage.
    /// Inventory, hotbar choice and input sequences carry on.
    fn arrive(&mut self, t: Travel, now: f64, local_ticks: f64) {
        let shape = t.planet.shape;
        let registry = self.world.registry.clone();
        self.far = crate::far::FarWorld::new(shape, registry.flags()).with_colours(&self.items.colours);
        self.world = ClientWorld::new(shape, registry, &self.items.colours);
        self.signs.clear();
        self.claims = Claims::new(shape, t.claims);
        self.remotes = Remotes::default();
        self.remote_lights.clear();
        self.lights_tick = 0;
        self.creatures.clear();
        self.seen.clear();
        self.aim_creature = None;
        self.shots.clear();
        self.fuse.reset();
        self.fx.clear();
        self.losses.reset(t.spawn.hurt);
        self.last_hurt = t.spawn.hurt;
        self.me.teleport(t.spawn);
        self.welcome_tick = t.tick;
        self.welcome_local = local_ticks;
        self.target = None;
        self.use_target = None;
        self.claim_target = None;
        self.damage.reset();
        self.place_cooldown = 0.0;
        self.journey = Some(Journey {
            to: Arrival {
                name: t.planet.name.clone(),
                kind: t.planet.kind.clone(),
                tier: t.planet.tier,
                color: t.planet.look.color,
            },
            started: now,
            landed: None,
        });
        self.planet = t.planet;
    }

    /// Moved on this planet (`ServerMsg::Teleport`, through a portal): a
    /// jump, not a correction. Chunks, claims, remote players and edits in
    /// flight carry on; what we aimed at and were breaking does not.
    fn teleport(&mut self, spawn: bm_core::PlayerState, tick: u64) {
        self.me.jump(spawn, tick);
        // A respawn (Return) comes this way too: health starts afresh.
        self.losses.reset(spawn.hurt);
        self.last_hurt = spawn.hurt;
        self.target = None;
        self.use_target = None;
        self.chisel = None;
    }

    /// Aims from `eye` along `dir`, within reach: `target` for breaking and
    /// placing (solid blocks, cut ones by their kept octants, as the server
    /// does), `use_target` for E, which open portal surfaces stop too.
    fn aim(&mut self, eye: DVec3, dir: DVec3) {
        let world = &self.world;
        let surface = self.portal_surface;
        let reach = self.reach();
        let aim = |surfaces: bool| {
            raycast_shaped(eye, dir, reach, |p| {
                let st = world.state(p)?;
                if surfaces && st.id == surface {
                    return Some(Shape::FULL);
                }
                // A prop (a torch) is not solid, but is aimed at by its
                // whole cell so it can be broken (and a block placed by it).
                if world.registry.get(st.id).is_prop() {
                    return Some(Shape::FULL);
                }
                // An open door (milestone 22) is not solid, but is aimed at
                // by its strip, to shut it or break it.
                if world.registry.get(st.id).swing.is_some() {
                    return Some(world.block_shape(p));
                }
                world.flags.solid(st.id).then(|| world.block_shape(p))
            })
        };
        self.target = aim(false);
        self.use_target = aim(true);
    }

    /// Plots we may claim: the allowance the server said, plus plots bought
    /// with marks since (milestone 12).
    fn allowance(&self) -> u32 {
        let bought = self.progress.character().plots_bought;
        self.plot_allowance + bought.saturating_sub(self.plots_bought_then)
    }

    /// How far we reach: the server's base plus Grasp (milestone 12).
    fn reach(&self) -> f64 {
        crate::skills::reach(self.base_reach, self.progress.effects())
    }

    /// The conditions some quirks need (night, health, hunger), judged as
    /// the server judges them when it applies an action: our state now, at
    /// the server tick we map now to (milestone 17).
    fn held_now(&self, local: f64) -> Held {
        Held::of(&self.me.state, &self.planet.day, self.server_tick(local) as f64)
    }

    /// Our best estimate of the server's tick now.
    fn server_tick(&self, local: f64) -> u64 {
        self.remotes
            .server_tick(local)
            .unwrap_or(self.welcome_tick as f64 + (local - self.welcome_local))
            .max(0.0) as u64
    }

    /// Standing inside one of our own claims ("at home": switching skill
    /// pages is free there).
    fn at_home(&self) -> bool {
        let feet = self.me.state.pos;
        let plot = PlotPos::of(BlockPos::containing(feet.x, feet.y, feet.z), self.world.shape);
        self.claims.at(plot).is_some_and(|c| c.owner == self.account)
    }

    /// Sets the bodies a skill change implies (`Progress::server`, `act`,
    /// `expire`) in prediction, where they differ from what we predicted.
    fn set_bodies(&mut self, bodies: Vec<(u32, bm_core::movement::Body)>) {
        for (at, mut body) in bodies {
            // Creative mode and guild buffs are not the skills'.
            body = body.keeping_switches(&self.me.body_at(at).unwrap_or(self.me.state.body));
            if self.me.body_at(at) != Some(body) {
                self.me
                    .set_body(at, body, &self.movement, self.world.shape, &self.world);
            }
        }
    }

    /// A skill change from the skill screen, following our latest input:
    /// predicted (body included, so moving and energy change at once and
    /// match the server) and returned to send, or refused with why.
    fn skill_action(&mut self, action: bm_protocol::SkillAction, local: f64) -> Result<ClientMsg, String> {
        let at = self.me.seq;
        let tick = self.server_tick(local);
        let home = self.at_home();
        self.progress
            .act(at, action.clone(), tick, home)
            .map_err(|r| r.to_string())?;
        let body = self.progress.effects().body();
        self.set_bodies(vec![(at, body)]);
        Ok(ClientMsg::Skill { at, action })
    }

    /// `ServerMsg::Character`: the server's word on the character, after
    /// the change that followed input `at` (0: XP or marks alone).
    fn character(&mut self, character: bm_core::skills::Character, at: u32) {
        let mut bodies = self.progress.server(character, at);
        if at == 0 && !self.progress.waiting() {
            // Nothing in flight: what we move with is the page's.
            bodies.push((self.me.seq, self.progress.effects().body()));
        }
        self.set_bodies(bodies);
    }

    /// Creative mode on or off right after our latest input: predicted, and
    /// the message that asks the server.
    fn toggle_creative(&mut self) -> ClientMsg {
        let at = self.me.seq;
        let on = !self.me.state.body.creative;
        let body = bm_core::movement::Body {
            creative: on,
            ..self.me.body_at(at).unwrap_or(self.me.state.body)
        };
        self.me
            .set_body(at, body, &self.movement, self.world.shape, &self.world);
        ClientMsg::Creative { at, on }
    }

    /// `ServerMsg::GuildBody`: what our guilds' buffs add from right after
    /// input `at` (milestone 25 part 3); prediction follows.
    fn guild_body(&mut self, at: u32, run: f64, grapple: f64, lava: u16, place: u16) {
        self.place_percent = place;
        let body = self.me.body_at(at).unwrap_or(self.me.state.body);
        let want = bm_core::movement::Body {
            guild_run_bonus: run,
            guild_grapple_bonus: grapple,
            guild_lava_percent: lava,
            ..body
        };
        if want != body {
            self.me
                .set_body(at, want, &self.movement, self.world.shape, &self.world);
        }
    }

    /// `ServerMsg::Creative`: whether creative mode holds from right after
    /// input `at`; where we predicted otherwise, prediction follows.
    fn creative(&mut self, at: u32, on: bool) {
        let body = self.me.body_at(at).unwrap_or(self.me.state.body);
        if body.creative != on {
            let body = bm_core::movement::Body { creative: on, ..body };
            self.me
                .set_body(at, body, &self.movement, self.world.shape, &self.world);
        }
    }

    /// What a right click holding `item` would place against the target: its
    /// block and, for a prop, the mount from the face aimed at (on top, or
    /// hanging on a side of a solid block; never underneath). None when it
    /// cannot be placed there.
    fn place_choice(&self, item: ItemId) -> Option<(BlockId, Option<Mount>)> {
        let registry = &self.items.registry;
        let block = registry.get(item)?.block()?;
        if registry.prop_block(&self.world.registry, item, Mount::Floor).is_none() {
            // Furnishings turn by the face clicked (`bm_core::forms`).
            let mount = self.target.and_then(|h| crate::mining::mount_for_face(h.normal));
            return Some((block, mount));
        }
        let hit = self.target?;
        let support = self.world.block(hit.block)?;
        if !self.world.flags.solid(support) {
            return None;
        }
        let mount = crate::mining::mount_for_face(hit.normal)?;
        Some((registry.prop_block(&self.world.registry, item, mount)?, Some(mount)))
    }

    /// One swing at `pos`, following input `at`, with the item in hotbar
    /// `slot` and looking along `look`: starts the cooldown and predicts the
    /// damage and breaks the server will rule (`crate::mining`). Returns the
    /// message to send, or None when there is nothing breakable to swing at.
    fn swing(&mut self, pos: BlockPos, at: u32, slot: usize, look: DVec3, held: &Held) -> Option<ClientMsg> {
        let block = self.world.block(pos)?;
        if block.is_air() || !self.world.registry.get(block).breakable {
            return None;
        }
        // Creative mode: one swing of anything breaks it, as the server rules.
        if self.me.state.body.creative {
            self.swinger.swung(at, CREATIVE_SWING_TICKS);
            self.edit_seq += 1;
            let seq = self.edit_seq;
            let fits = self
                .items
                .registry
                .drop_of(block)
                .is_none_or(|d| self.items.inventory.can_fit(&self.items.registry, d, 1));
            if fits {
                self.world.predict_edit(seq, pos, BlockState::AIR);
            }
            return Some(ClientMsg::Swing {
                seq,
                at,
                pos: self.world.shape.wrap_block(pos),
                slot: slot as u8,
            });
        }
        // A bare hand breaks nothing (Nathan, 2026-10-20: as in Boundless).
        // The tool through its traits (milestone 17), as the server reads it.
        let tool = self.items.tool_of(self.items.slot(slot).as_ref(), held)?;
        // With the brew in effect (milestone 23 part 2), as the server reads it.
        let effects = self.progress.effects().brewed(&self.me.state);
        // Skills quicken the cadence (Deftness, mastery, the tool's epic).
        self.swinger
            .swung(at, swing_ticks(skilled_tool(&tool, &effects).action_speed));
        self.edit_seq += 1;
        let seq = self.edit_seq;
        // Energy (milestone 10): spent right after input `at`, as the server
        // will. Too tired, the swing does nothing but restart the pause; it
        // is still sent so the server restarts it too.
        if !self.me.spend(
            at,
            tool.energy,
            bm_core::food::Drain::Tool,
            &self.movement,
            self.world.shape,
            &self.world,
        ) {
            return Some(ClientMsg::Swing {
                seq,
                at,
                pos: self.world.shape.wrap_block(pos),
                slot: slot as u8,
            });
        }
        let world = &self.world;
        let input = SwingInput {
            keen: self.me.state.crit_multiplier(),
            tool: &tool,
            effects: &effects,
            hit: pos,
            look,
            player: self.player_id,
            at,
            tier: self.planet.tier,
            shape: world.shape,
            registry: &world.registry,
        };
        let hits = predict_swing(&input, &self.damage, |p| world.block(p));
        for h in hits {
            // With no room for what it yields the server refuses the break;
            // predicting it would only flash the block away.
            let fits = self
                .items
                .registry
                .drop_of(h.block)
                .is_none_or(|d| self.items.inventory.can_fit(&self.items.registry, d, 1));
            // A full pack leaves it one point short, as the server does.
            let h = match h.breaks && !fits {
                true => {
                    let health = bm_core::mining::block_health(self.world.registry.get(h.block), self.planet.tier);
                    BlockHit {
                        breaks: false,
                        damage: (health - 1).saturating_sub(self.damage.damage(h.pos)),
                        ..h
                    }
                }
                false => h,
            };
            self.damage.hit(seq, at, &h);
            if h.breaks {
                self.world.predict_edit(seq, h.pos, BlockState::AIR);
            }
        }
        Some(ClientMsg::Swing {
            seq,
            at,
            pos: self.world.shape.wrap_block(pos),
            slot: slot as u8,
        })
    }

    /// `ServerMsg::BlockHealth`: the server's word on a block's damage. If
    /// it answers a swing we predicted would break the block, the block
    /// still stands.
    fn block_health(&mut self, pos: BlockPos, damage: u32) {
        let pos = self.world.shape.wrap_block(pos);
        if let Some(seq) = self.damage.server_health(pos, damage, self.me.seq) {
            self.world.revert_prediction(seq, pos);
        }
    }

    /// `ServerMsg::EditResult`: settles our edit (a placed block, a cut, a
    /// broken one). Accepted, the block starts again at full health;
    /// refused, the server's word on its damage follows (`BlockHealth`), so
    /// our guesses about it go.
    fn edit_result(&mut self, seq: u32, pos: BlockPos, block: BlockState) {
        self.world.apply_edit_result(seq, pos, block);
        self.damage.clear(self.world.shape.wrap_block(pos));
    }

    /// `ServerMsg::BlockChanged`: someone else changed a block.
    fn block_changed(&mut self, pos: BlockPos, block: BlockState) {
        self.world.set_block(pos, block);
        self.damage.clear(self.world.shape.wrap_block(pos));
    }

    /// Whether the selected hotbar item is a spanner, which repairs
    /// machines and breaks nothing: no block health bar for it.
    fn holding_spanner(&self) -> bool {
        [self.selected, self.selected_off].into_iter().any(|i| {
            self.items
                .tool_of(self.items.slot(i).as_ref(), &Held::default())
                .is_some_and(|t| t.class == Some(bm_core::items::ToolClass::Spanner))
        })
    }

    /// The aimed block's health for the HUD and the cracks: where it is,
    /// what is left (0 to 1) and the planet's shields (one per 100 armour).
    /// None when aiming at nothing breakable.
    fn target_health(&self) -> Option<(BlockPos, f32, u32)> {
        let hit = self.target?;
        let block = self.world.block(hit.block)?;
        let def = self.world.registry.get(block);
        if block.is_air() || !def.breakable {
            return None;
        }
        let pos = self.world.shape.wrap_block(hit.block);
        let tier = self.planet.tier;
        let left = crate::mining::health_left(def, tier, self.damage.damage(pos));
        Some((hit.block, left, bm_core::mining::armour(tier) / 100))
    }

    /// Whether a block is part of a portal (a conduit, or an open surface).
    fn is_portal_block(&self, p: BlockPos) -> bool {
        self.world
            .block(p)
            .is_some_and(|b| b == self.portal_conduit || b == self.portal_surface)
    }

    /// The portal block aimed at for E, if any.
    fn aiming_at_portal(&self) -> Option<BlockPos> {
        let hit = self.use_target?;
        self.is_portal_block(hit.block).then_some(hit.block)
    }

    /// The sign aimed at for E, if any (milestone 22 part 4).
    /// The helix machine aimed at for E (milestone 25 part 3).
    fn aiming_at_helix(&self) -> Option<BlockPos> {
        let hit = self.target?;
        let block = self.world.block(hit.block)?;
        bm_core::guilds::BuffKind::of_helix(&self.world.registry.get(block).name).map(|_| hit.block)
    }

    /// The guild control aimed at for E (milestone 25 part 2).
    fn aiming_at_guild_control(&self) -> Option<BlockPos> {
        let hit = self.target?;
        let block = self.world.block(hit.block)?;
        (self.world.registry.get(block).name == "guild_control").then_some(hit.block)
    }

    fn aiming_at_sign(&self) -> Option<BlockPos> {
        let hit = self.use_target?;
        let s = self.world.state(hit.block)?;
        bm_core::signs::is_sign(s.shape).then_some(hit.block)
    }

    /// The door or trapdoor aimed at for E, if any (milestone 22).
    fn aiming_at_door(&self) -> Option<BlockPos> {
        let hit = self.use_target?;
        let b = self.world.block(hit.block)?;
        self.world.registry.get(b).swing.map(|_| hit.block)
    }

    /// The open meteor heart aimed at, whose share we have not taken.
    fn aiming_at_heart(&self) -> Option<BlockPos> {
        let hit = self.use_target?;
        self.sky_view
            .heart_open(self.world.shape, hit.block)
            .then_some(hit.block)
    }

    /// Applies a snapshot; one about another planet (sent before we
    /// travelled, overtaking the stream) is dropped. Returns whether it was used.
    fn snapshot(&mut self, d: ServerDatagram, local: f64, reconcile: bool) -> bool {
        let ServerDatagram::Snapshot {
            planet,
            tick,
            ack,
            you,
            others,
            creatures,
        } = d;
        if planet != self.planet.id {
            return false;
        }
        let shape = self.world.shape;
        if reconcile {
            self.me.reconcile(tick, ack, you, &self.movement, shape, &self.world);
        }
        // One tick may come in several parts, each with other players: their
        // lights add up; a newer tick starts afresh.
        if tick > self.lights_tick {
            self.lights_tick = tick;
            self.remote_lights.clear();
        }
        if tick == self.lights_tick {
            self.remote_lights.extend(others.iter().map(|o| (o.id, o.light)));
        }
        self.creatures.record(tick, &creatures);
        self.remotes.record(
            tick,
            local,
            others.into_iter().map(|o| {
                (
                    o.id,
                    RemoteSample {
                        tick,
                        pos: DVec3::new(o.pos[0] as f64, o.pos[1] as f64, o.pos[2] as f64),
                        yaw: o.yaw,
                        pitch: o.pitch,
                        hook: o.hook_offset(),
                    },
                )
            }),
        );
        true
    }

    /// Still on the way: the new planet's ground has not arrived yet.
    fn travelling(&self) -> bool {
        self.journey.as_ref().is_some_and(|j| j.landed.is_none())
    }

    /// Lands the journey once the ground is here, and ends its fade.
    fn update_journey(&mut self, now: f64) {
        let ready = self.world.has_chunks_around(self.me.state.pos);
        let Some(j) = &mut self.journey else { return };
        if j.landed.is_none() && ready && now - j.started >= ARRIVAL_MIN {
            j.landed = Some(now);
        }
        if j.landed.is_some_and(|l| now - l >= ARRIVAL_FADE) {
            self.journey = None;
        }
    }

    /// The server tick the sky shows now.
    fn sky_tick(&self, local: f64) -> f64 {
        // A development override for judging the sky: BOUNDMORE_TIME=0.75 is
        // sunset. Visual only; the server's clock is untouched.
        if let Some(tod) = std::env::var("BOUNDMORE_TIME").ok().and_then(|v| v.parse::<f64>().ok()) {
            return tod * self.planet.day.day_ticks as f64 - self.planet.day.start_offset as f64;
        }
        self.remotes
            .server_tick(local)
            .unwrap_or(self.welcome_tick as f64 + (local - self.welcome_local))
    }

    /// Held torches as point lights (`Renderer::set_point_lights`): ours at
    /// the hand (where the rope starts), everyone else's at their hands.
    fn held_lights(&self, remotes: &[(u32, RemoteSample)], camera: &Camera) -> Vec<(DVec3, [u8; 3])> {
        // A torch in its colour, or a Glowing tool (milestone 17).
        // Either hand (milestone 28): the brighter of the two, per channel.
        let held = bm_core::light::brighter(
            self.items.held_rgb_of(self.items.slot(self.selected).as_ref()),
            self.items.held_rgb_of(self.items.slot(self.selected_off).as_ref()),
        );
        // The light-source epic lights around us like a held torch.
        // A Glowing brew lights around us too (milestone 23 part 2).
        let mine = bm_core::light::brighter(
            crate::skills::carried_light(held, self.progress.effects()),
            [self.me.state.brew_light(); 3],
        );
        let forward = camera.forward();
        let right = forward.cross(DVec3::Y).normalize_or_zero();
        let hand = camera.eye + right * 0.3 - DVec3::Y * 0.35 + forward * 0.4;
        let others = remotes.iter().filter_map(|(id, smp)| {
            let light = self.remote_lights.get(id).copied().unwrap_or([0; 3]);
            (light != [0; 3]).then(|| (smp.pos + DVec3::new(0.0, bm_core::movement::ROPE_HEIGHT, 0.0), light))
        });
        std::iter::once((hand, mine))
            .filter(|l| l.1 != [0; 3])
            .chain(others)
            .collect()
    }

    /// The storage block aimed at (within reach, as aiming is), if any.
    fn aiming_at_storage(&self) -> Option<BlockPos> {
        let hit = self.target?;
        let block = self.world.block(hit.block)?;
        (self.world.registry.get(block).storage > 0).then_some(hit.block)
    }

    /// `ServerMsg::Hit`: damage we dealt (a number over the creature, which
    /// flashes) or took (a number by the crosshair, and the red flash).
    fn hit(&mut self, target: Target, amount: u32, crit: bool, now: f64) {
        match target {
            Target::Creature(id) => {
                self.creatures.flash(id);
                if let Some(c) = self.seen.iter().find(|c| c.id == id)
                    && let Some(k) = self.creatures.kind(c.kind)
                {
                    let over = c.pos + DVec3::new(0.0, k.height as f64 + 0.3, 0.0);
                    self.floaters.add(Anchor::World(over), amount, crit, false, now);
                }
            }
            Target::Player(id) if id == self.player_id => {
                self.losses.hit(amount, now);
                self.floaters.add(Anchor::Me, amount, crit, true, now);
                self.hurt_at = now;
            }
            Target::Player(_) => {}
        }
    }

    /// Name plates to show: the creature aimed at, and hurt living ones
    /// within `fight_hud::PLATE_RANGE`.
    fn plates(&self, eye: DVec3, forward: Vec3, screen: egui::Rect) -> Vec<crate::fight_hud::Plate> {
        let shape = self.world.shape;
        let aimed = self.aim_creature.map(|a| a.0);
        let mut out = Vec::new();
        for c in &self.seen {
            let Some(k) = self.creatures.kind(c.kind) else { continue };
            let pos = shape.nearest_lap(eye, c.pos);
            let near = (pos - eye).length() <= crate::fight_hud::PLATE_RANGE;
            if aimed != Some(c.id) && !(c.damage > 0 && !c.dead && near) {
                continue;
            }
            let over = pos + DVec3::new(0.0, k.height as f64 + 0.55, 0.0);
            let Some(at) = crate::fight_hud::to_screen((over - eye).as_vec3(), forward, FOV_Y, screen) else {
                continue;
            };
            let t = k.tier(c.tier);
            let element = bm_core::creatures::Essence::from_code(c.element);
            let max = bm_core::creatures::max_health(t.health, bm_core::creatures::Exotic(c.exotic), element);
            out.push(crate::fight_hud::Plate {
                at,
                title: plate_title(&k.title(c.tier), c.exotic != 0, element),
                angry: c.angry,
                left: 1.0 - c.damage.min(max) as f32 / max.max(1) as f32,
                shields: t.armour / 100,
                dead: c.dead,
            });
        }
        out
    }

    /// The damage numbers on screen now.
    fn numbers(&self, eye: DVec3, forward: Vec3, screen: egui::Rect, now: f64) -> Vec<crate::fight_hud::Number> {
        let shape = self.world.shape;
        self.floaters
            .visible(now)
            .into_iter()
            .filter_map(|(f, rise, alpha)| {
                let at = match f.at {
                    Anchor::World(p) => {
                        let p = shape.nearest_lap(eye, p) + DVec3::new(0.0, rise as f64 * 0.9, 0.0);
                        crate::fight_hud::to_screen((p - eye).as_vec3(), forward, FOV_Y, screen)?
                            + egui::vec2(f.drift * 36.0, 0.0)
                    }
                    Anchor::Me => screen.center() + egui::vec2(28.0 + f.drift * 30.0, -36.0 - rise * 60.0),
                };
                Some(crate::fight_hud::Number {
                    at,
                    amount: f.amount,
                    crit: f.crit,
                    taken: f.taken,
                    alpha,
                })
            })
            .collect()
    }

    /// Where the camera is, `alpha` of the way through the tick: the eye,
    /// or low by the ground while down.
    fn render_eye(&self, alpha: f64) -> DVec3 {
        let shape = self.world.shape;
        let feet = self.me.render_pos(alpha, shape);
        let height = if self.me.state.down {
            DOWN_EYE
        } else {
            self.movement.eye_height
        };
        shape.wrap_point(feet + DVec3::new(0.0, height, 0.0))
    }

    /// Whether the block aimed at is a warp shrine.
    fn aiming_at_shrine(&self) -> Option<BlockPos> {
        let hit = self.target?;
        (self.world.block(hit.block) == Some(self.warp_shrine)).then_some(hit.block)
    }
}

struct EguiState {
    ctx: egui::Context,
    winit: egui_winit::State,
}

pub struct App {
    opts: Options,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    egui: Option<EguiState>,
    link: Option<Link>,
    session: Option<Session>,
    settings: Settings,
    login: LoginForm,
    menu: Option<Menu>,
    drag: Option<Drag>,
    live_station: Option<LiveStation>,
    /// The open storage block's contents as the server last sent them.
    storage_view: Option<StorageView>,
    /// The open sign window's text (milestone 22 part 4).
    sign_ui: Option<crate::signs::SignUi>,
    /// The open portal window's portal as the server last sent it.
    portal_view: Option<PortalView>,
    /// The open shop stand or basket (milestone 24 part 2).
    shop_view: Option<bm_protocol::ShopView>,
    /// The open atlas's map (milestone 27).
    atlas: Option<crate::atlas::AtlasMap>,
    /// The atlas in this slot was used this frame: its window opens.
    atlas_request: Option<u8>,
    /// Prices being typed in the owner's price window, by slot.
    shop_prices: HashMap<u8, String>,
    /// The name being typed in the friends window.
    friend_name: String,
    /// The open guild control, and what is typed in the guild window.
    guild_control_view: Option<bm_protocol::GuildControlView>,
    /// The open helix machine (milestone 25 part 3).
    helix_view: Option<bm_protocol::HelixView>,
    guild_form: menus::GuildForm,
    /// The warp shrine window's open tab.
    shrine_tab: ShrineTab,
    /// The skill screen's chosen category and reset question.
    skill_ui: SkillUi,
    /// The forge window's chosen gear and deck (milestone 17).
    forge_ui: ForgeUi,
    /// Storage blocks we closed whose `closed` answer has not come back yet:
    /// that answer must not close the same block opened again meanwhile.
    storage_closing: Vec<BlockPos>,
    chat: ChatBox,
    claim_mode: bool,
    notices: Notices,
    status: String,
    keys: HashSet<KeyCode>,
    mouse: [bool; 2],
    /// A left click not yet sent as a grapple shot (clicks between ticks
    /// wait for the next one).
    fire_pending: bool,
    /// The right button's press, as `fire_pending` and `attack_pending`
    /// are the left's (milestone 28).
    fire_pending_off: bool,
    attack_pending_off: bool,
    /// The hand that acted last (0 left, 1 right): held both, the button
    /// pressed last acts (one hand at a time).
    last_hand: usize,
    /// A left click not yet seen by the attack code (a sling's click must
    /// count however short).
    attack_pending: bool,
    /// `--demo-ui bombs`: the attack button is held on a cycle, and whether
    /// it was last frame.
    demo_bombs: bool,
    demo_was_held: bool,
    /// `--demo-ui`: where to put us once the block its hook holds exists.
    demo_pose: Option<bm_core::PlayerState>,
    grabbed: bool,
    yaw: f64,
    pitch: f64,
    start: Instant,
    last_frame: Instant,
    accumulator: f64,
    frames: u32,
    fps: f64,
    last_title: Instant,
    missing_textures: usize,
}

impl App {
    pub fn new(opts: Options) -> Self {
        let now = Instant::now();
        let settings = Settings::load(&opts.settings_path);
        let login = LoginForm {
            name: opts.name.clone().unwrap_or_else(|| settings.name.clone()),
            password: String::new(),
            server: if settings.server.is_empty() || opts.name.is_some() {
                opts.server.clone()
            } else {
                settings.server.clone()
            },
            error: None,
            busy: false,
        };
        Self {
            opts,
            window: None,
            renderer: None,
            egui: None,
            link: None,
            session: None,
            settings,
            login,
            menu: None,
            drag: None,
            live_station: None,
            storage_view: None,
            sign_ui: None,
            portal_view: None,
            shop_view: None,
            atlas: None,
            atlas_request: None,
            shop_prices: HashMap::new(),
            friend_name: String::new(),
            guild_control_view: None,
            helix_view: None,
            guild_form: Default::default(),
            shrine_tab: ShrineTab::Worlds,
            skill_ui: SkillUi::default(),
            forge_ui: ForgeUi::default(),
            storage_closing: Vec::new(),
            chat: ChatBox::default(),
            claim_mode: false,
            notices: Notices::default(),
            status: "logging in".into(),
            keys: HashSet::new(),
            mouse: [false; 2],
            fire_pending: false,
            fire_pending_off: false,
            attack_pending_off: false,
            last_hand: 0,
            attack_pending: false,
            demo_bombs: false,
            demo_was_held: false,
            demo_pose: None,
            grabbed: false,
            yaw: 0.0,
            pitch: 0.0,
            start: now,
            last_frame: now,
            accumulator: 0.0,
            frames: 0,
            fps: 0.0,
            last_title: now,
            missing_textures: 0,
        }
    }

    fn local_ticks(&self) -> f64 {
        self.start.elapsed().as_secs_f64() / TICK_DT
    }

    fn now(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }

    fn set_grab(&mut self, grab: bool) {
        let Some(w) = &self.window else { return };
        if grab {
            // Playing does not depend on the platform granting a grab: raw
            // mouse motion still turns the camera without one.
            let locked = w
                .set_cursor_grab(CursorGrabMode::Locked)
                .or_else(|_| w.set_cursor_grab(CursorGrabMode::Confined))
                .is_ok();
            if !locked {
                tracing::warn!("this platform refused to grab the cursor; playing without it");
            }
            w.set_cursor_visible(false);
            self.grabbed = true;
        } else {
            let _ = w.set_cursor_grab(CursorGrabMode::None);
            w.set_cursor_visible(true);
            self.grabbed = false;
        }
    }

    fn send(&mut self, msg: ClientMsg) {
        if let Some(l) = &mut self.link {
            l.send(msg);
        }
    }

    /// F8 or `/creative`: creative mode on or off, predicted at once (the
    /// server answers whether it holds).
    fn toggle_creative(&mut self) {
        if let Some(msg) = self.session.as_mut().map(|s| s.toggle_creative()) {
            self.send(msg);
        }
    }

    // ---- logging in ----

    fn start_login(&mut self, register: bool) {
        let name = self.login.name.trim().to_string();
        if bm_protocol::clean_name(&name).as_deref() != Some(name.as_str()) {
            self.login.error = Some(format!(
                "Names are 2 to {} letters, digits, spaces, - or _.",
                bm_protocol::MAX_NAME_LEN
            ));
            return;
        }
        if self.login.password.chars().count() < MIN_PASSWORD_LEN {
            self.login.error = Some(format!("Passwords are at least {MIN_PASSWORD_LEN} characters."));
            return;
        }
        if self.login.server.trim().is_empty() {
            self.login.error = Some("Which server? For example 127.0.0.1:7777".into());
            return;
        }
        self.login.error = None;
        self.login.busy = true;
        self.status = format!("connecting to {}", self.login.server.trim());
        // A new attempt is a new network session.
        self.link = Some(Link::Net(net::spawn(ConnectOptions {
            server: self.login.server.trim().to_string(),
            name,
            password: self.login.password.clone(),
            register,
            cert: self.opts.cert.clone(),
        })));
    }

    fn cancel_login(&mut self) {
        self.link = None;
        self.login.busy = false;
        self.status = "logging in".into();
    }

    // ---- menus ----

    fn open_menu(&mut self, menu: Menu) {
        if self.menu == Some(menu) {
            return;
        }
        self.close_menu(false);
        if let Menu::Crafting(Some(pos)) = menu {
            self.live_station = None;
            self.forge_ui = ForgeUi::default();
            self.send(ClientMsg::WatchStation { pos: Some(pos) });
        }
        if let Menu::Storage(pos) = menu {
            self.storage_view = None;
            self.send(ClientMsg::OpenStorage { pos: Some(pos) });
        }
        if let Menu::Travel(_) = menu {
            self.shrine_tab = ShrineTab::Worlds;
        }
        if let Menu::Skills = menu {
            self.skill_ui.confirm_reset = false;
        }
        if let Menu::Portal(pos) = menu {
            self.portal_view = None;
            self.send(ClientMsg::OpenPortal { pos: Some(pos) });
        }
        if let Menu::Shop(pos) = menu {
            self.shop_view = None;
            self.send(ClientMsg::OpenShop { pos: Some(pos) });
        }
        if let Menu::Atlas(slot) = menu {
            self.send(ClientMsg::OpenAtlas { slot: Some(slot) });
        }
        self.menu = Some(menu);
        self.keys.clear();
        self.mouse = [false; 2];
        self.set_grab(false);
    }

    fn close_menu(&mut self, regrab: bool) {
        if let Some(Menu::Crafting(Some(_))) = self.menu {
            self.send(ClientMsg::WatchStation { pos: None });
            self.live_station = None;
        }
        if let Some(Menu::Storage(pos)) = self.menu {
            self.send(ClientMsg::OpenStorage { pos: None });
            self.storage_closing.push(pos);
            self.storage_view = None;
        }
        if matches!(self.menu, Some(Menu::Shop(_))) || self.shop_view.is_some() {
            self.send(ClientMsg::OpenShop { pos: None });
            self.shop_view = None;
            self.shop_prices.clear();
        }
        if let Some(Menu::Portal(_)) = self.menu {
            self.send(ClientMsg::OpenPortal { pos: None });
            self.portal_view = None;
        }
        if let Some(Menu::Atlas(_)) = self.menu {
            self.send(ClientMsg::OpenAtlas { slot: None });
            self.atlas = None;
        }
        if let Some(Menu::Helix(_)) = self.menu {
            self.send(ClientMsg::OpenHelix { pos: None });
            self.helix_view = None;
        }
        if let Some(Menu::Guild(Some(_))) = self.menu {
            self.send(ClientMsg::OpenGuildControl { pos: None });
            self.guild_control_view = None;
        }
        let was_open = self.menu.take().is_some();
        self.drag = None;
        if was_open && regrab && self.session.is_some() && !self.chat.open {
            self.set_grab(true);
        }
    }

    /// The crafting screen: the station looked at if there is one in reach,
    /// otherwise the hand recipes.
    fn crafting_menu(&self) -> Menu {
        let station = self.session.as_ref().and_then(|s| {
            let hit = s.target?;
            let block = s.world.block(hit.block)?;
            menus::station_of(s.world.registry.get(block)).map(|_| hit.block)
        });
        Menu::Crafting(station)
    }

    fn toggle(&mut self, menu: Menu) {
        if self
            .menu
            .is_some_and(|m| std::mem::discriminant(&m) == std::mem::discriminant(&menu))
        {
            self.close_menu(true);
        } else {
            self.claim_mode = false;
            self.open_menu(menu);
        }
    }

    /// Opens the chat line: the mouse is let go and game keys stop. Opened
    /// by a key, that key's press is kept out of the line.
    fn open_chat(&mut self, by_key: bool) {
        self.chat.open(by_key);
        self.keys.clear();
        self.mouse = [false; 2];
        self.set_grab(false);
    }

    fn close_chat(&mut self) {
        self.chat.close();
        if self.menu.is_none() && self.session.is_some() {
            self.set_grab(true);
        }
    }

    /// E on a storage block: open it beside the inventory, unless it stands
    /// on someone else's claim (the server refuses that too).
    fn open_storage(&mut self, pos: BlockPos) {
        let now = self.now();
        let Some(s) = &self.session else { return };
        // A shop stand or basket (milestone 24 part 2): its owner stocks it
        // as storage beside its prices; anyone else gets its shop window.
        if s.world
            .block(pos)
            .is_some_and(|b| crate::shop::is_shop(&s.world.registry.get(b).name))
        {
            let pos = s.world.shape.wrap_block(pos);
            let mine = s
                .claims
                .at(PlotPos::of(pos, s.world.shape))
                .is_some_and(|c| c.owner == s.account);
            self.shop_prices.clear();
            if mine {
                self.open_menu(Menu::Storage(pos));
                self.send(ClientMsg::OpenShop { pos: Some(pos) });
            } else {
                self.open_menu(Menu::Shop(pos));
            }
            return;
        }
        if let Some(c) = s.claims.at(PlotPos::of(pos, s.world.shape))
            && !crate::claims::may(c, &s.account, &s.guilds, bm_core::rights::Act::Work)
        {
            let text = format!("This land belongs to {}", c.owner);
            self.notices.push(text, now);
            return;
        }
        // The server names storage by canonical position.
        let pos = s.world.shape.wrap_block(pos);
        self.open_menu(Menu::Storage(pos));
    }

    /// E on a sign: its window, to write where we may (no claim, or ours;
    /// the server rules it too) and to read elsewhere.
    fn open_sign(&mut self, pos: BlockPos) {
        let Some(s) = &self.session else { return };
        let pos = s.world.shape.wrap_block(pos);
        let writable = s
            .claims
            .at(PlotPos::of(pos, s.world.shape))
            .is_none_or(|c| crate::claims::may(c, &s.account, &s.guilds, bm_core::rights::Act::Build));
        let anchor = s.signs.sign_at(&s.world, pos);
        let text = anchor
            .and_then(|a| s.signs.get(a))
            .map(|v| v.text.clone())
            .unwrap_or_default();
        if !writable && anchor.is_none() {
            return;
        }
        self.sign_ui = Some(crate::signs::SignUi { text, writable });
        self.open_menu(Menu::Sign(pos));
    }

    /// E on a portal block: open its window, unless it stands on someone
    /// else's claim or is no portal yet (the server refuses both too; the
    /// shape rule is told here at once).
    fn open_portal(&mut self, pos: BlockPos) {
        let now = self.now();
        let Some(s) = &self.session else { return };
        if let Some(c) = s.claims.at(PlotPos::of(pos, s.world.shape))
            && c.owner != s.account
        {
            let text = format!("This land belongs to {}", c.owner);
            self.notices.push(text, now);
            return;
        }
        if let Err(bm_core::portal::FrameError::Shape) = bm_core::portal::find_frame(pos, |p| s.is_portal_block(p)) {
            self.notices.push(bm_core::portal::SHAPE_RULE.into(), now);
            return;
        }
        let pos = s.world.shape.wrap_block(pos);
        self.open_menu(Menu::Portal(pos));
    }

    /// A `Portal` from the server: the open window's news, or (None) that
    /// it closed on the server's side: the portal is gone, out of reach or
    /// not ours. Nothing is asked of the server for that close.
    /// A `Shop` from the server (milestone 24 part 2): kept while its window
    /// (or, for its owner, its storage window) is open.
    fn shop_update(&mut self, view: bm_protocol::ShopView) {
        let showing = match self.menu {
            Some(Menu::Shop(p)) | Some(Menu::Storage(p)) => p == view.pos,
            _ => false,
        };
        if !showing {
            return;
        }
        if view.closed {
            self.shop_view = None;
            if matches!(self.menu, Some(Menu::Shop(_))) {
                self.menu = None;
                if self.session.is_some() && !self.chat.open {
                    self.set_grab(true);
                }
            }
            return;
        }
        self.shop_view = Some(view);
    }

    /// A `PortalClosed`: shuts the window if it is still that portal's (or
    /// one whose open was refused); a late close of a portal left behind,
    /// while another opens, is ignored (M8 follow-up, 2026-10-27).
    fn portal_closed(&mut self, id: Option<u32>) {
        let ours = match id {
            None => true,
            Some(id) => self.portal_view.as_ref().is_some_and(|v| v.id == id),
        };
        if ours {
            self.portal_update(None);
        }
    }

    fn portal_update(&mut self, view: Option<PortalView>) {
        if !matches!(self.menu, Some(Menu::Portal(_))) {
            return;
        }
        match view {
            Some(v) => self.portal_view = Some(v),
            None => {
                self.portal_view = None;
                self.menu = None;
                if self.session.is_some() && !self.chat.open {
                    self.set_grab(true);
                }
            }
        }
    }

    /// A `Storage` from the server: shown if it is the one open, and closing
    /// the window when it is gone or out of reach.
    fn storage_update(&mut self, view: StorageView) {
        if view.closed
            && let Some(i) = self.storage_closing.iter().position(|p| *p == view.pos)
        {
            // The answer to our own close, not news about the block.
            self.storage_closing.remove(i);
            return;
        }
        if self.menu != Some(Menu::Storage(view.pos)) {
            return;
        }
        if view.closed {
            self.storage_view = None;
            // Already closed on the server: closing here asks for nothing.
            self.menu = None;
            self.drag = None;
            if self.session.is_some() && !self.chat.open {
                self.set_grab(true);
            }
            return;
        }
        let Some(s) = &self.session else { return };
        let slots = crate::storage::sanitize(&s.items.registry, view.slots);
        self.storage_view = Some(StorageView { slots, ..view });
    }

    fn apply_actions(&mut self, actions: Vec<UiAction>) {
        for a in actions {
            match a {
                UiAction::Send(m) => self.send(m),
                UiAction::LogIn { register } => self.start_login(register),
                UiAction::CancelLogin => self.cancel_login(),
                UiAction::CloseMenu => self.close_menu(true),
                UiAction::CraftColour { recipe, colour } => {
                    if let Some(s) = &mut self.session {
                        s.craft_colours.insert(recipe, colour);
                    }
                }
                UiAction::Skill(action) => {
                    let (now, local) = (self.now(), self.local_ticks());
                    let Some(s) = &mut self.session else { continue };
                    match s.skill_action(action, local) {
                        Ok(m) => self.send(m),
                        Err(why) => self.notices.push(why, now),
                    }
                }
            }
        }
    }

    // ---- the network ----

    fn handle_net(&mut self) {
        let Some(link) = &mut self.link else { return };
        for ev in link.poll() {
            match ev {
                NetEvent::Connected(w) => self.welcome(*w),
                NetEvent::Msg(m) => self.handle_msg(m),
                NetEvent::Snapshot(d) => self.handle_snapshot(d, true),
                NetEvent::Observed(d) => self.handle_snapshot(d, false),
                NetEvent::Disconnected(reason) => {
                    tracing::warn!("disconnected: {reason}");
                    let was_playing = self.session.is_some();
                    self.close_menu(false);
                    self.chat.close();
                    self.claim_mode = false;
                    self.session = None;
                    self.link = None;
                    if let Some(r) = &mut self.renderer {
                        r.clear_chunks();
                    }
                    self.login.busy = false;
                    self.login.error = Some(if was_playing {
                        format!("Disconnected: {reason}")
                    } else {
                        reason.clone()
                    });
                    self.status = format!("disconnected: {reason}");
                    self.set_grab(false);
                    return;
                }
            }
        }
    }

    fn welcome(&mut self, w: bm_protocol::Welcome) {
        let registry = match BlockRegistry::from_defs(w.blocks.clone()) {
            Ok(r) => r,
            Err(e) => return self.refuse_catalogue(format!("the server sent a bad block catalogue: {e}")),
        };
        let items = match Items::new(
            w.items.clone(),
            w.recipes.clone(),
            w.inventory.clone(),
            w.colours.clone(),
            &registry,
        ) {
            Ok(i) => i,
            Err(e) => return self.refuse_catalogue(format!("the server sent a bad item catalogue: {e}")),
        };
        let set = TextureSet::load(&registry, &self.opts.assets.join("textures"));
        self.missing_textures = set.missing.len();
        if let Some(r) = &mut self.renderer {
            r.clear_chunks();
            r.write_palette(&registry);
            r.set_colours(&w.colours);
            r.set_textures(&set);
        }
        if let Some(r) = &mut self.renderer {
            r.set_tints(&w.planet.tints);
        }
        let icons = self.build_icons(&items, &registry, &set, &w.planet.tints);
        tracing::info!("welcomed to {} as {} (#{})", w.planet.name, w.account, w.player_id);
        self.chat.me = w.account.clone();
        self.storage_closing.clear();
        self.status = format!("on {}", w.planet.name);
        // Remember who and where, never the password; and forget it here too.
        self.login.busy = false;
        self.login.error = None;
        self.login.password.clear();
        if self.opts.demo.is_none() {
            self.settings.name = w.account.clone();
            self.settings.server = self.login.server.trim().to_string();
            if let Err(e) = self.settings.save(&self.opts.settings_path) {
                tracing::warn!("cannot save settings to {}: {e}", self.opts.settings_path.display());
            }
        }
        match Session::new(w, registry, items, set, icons, self.local_ticks()) {
            Ok(s) => self.session = Some(s),
            Err(e) => return self.refuse_catalogue(e),
        }
        self.set_grab(true);
        self.open_demo_screen();
    }

    /// Item icons, block items painted in this planet's tints.
    fn build_icons(&self, items: &Items, registry: &BlockRegistry, set: &TextureSet, tints: &[[u8; 3]]) -> ItemIcons {
        let items_dir = self.opts.assets.join("textures").join("items");
        let sources = items
            .registry
            .defs()
            .iter()
            .map(|d| icon_for(d, registry, set, &items_dir, tints, None))
            .collect();
        match &self.egui {
            Some(e) => ItemIcons::build(&e.ctx, sources),
            None => ItemIcons::default(),
        }
    }

    /// Makes icons for coloured stacks shown anywhere (inventory, the open
    /// chest, a station's output, the reclaim) the first time each colour of
    /// each item appears. Painted like the terrain shader paints the block.
    fn ensure_coloured_icons(&mut self) {
        let (Some(s), Some(e)) = (self.session.as_mut(), self.egui.as_ref()) else {
            return;
        };
        let chest = self.storage_view.iter().flat_map(|v| v.slots.iter().flatten());
        let output = self.live_station.iter().flat_map(|l| l.view.output.iter());
        let missing = s.icons.missing_colours(
            s.items
                .inventory
                .slots
                .iter()
                .flatten()
                .chain(chest)
                .chain(output)
                .chain(s.reclaim.iter()),
        );
        if missing.is_empty() {
            return;
        }
        let items_dir = self.opts.assets.join("textures").join("items");
        for (item, colour) in missing {
            let Some(def) = s.items.registry.get(item) else {
                continue;
            };
            let source = icon_for(
                def,
                &s.world.registry,
                &s.textures,
                &items_dir,
                &s.planet.tints,
                crate::colour::linear(&s.items.colours, colour),
            );
            s.icons.add_coloured(&e.ctx, (item, colour), source);
        }
    }

    /// The server moved us to another planet (`ServerMsg::Travel`).
    fn travel(&mut self, t: Travel) {
        let (now, local) = (self.now(), self.local_ticks());
        self.close_menu(false);
        // The server closed every storage view before the journey.
        self.storage_closing.clear();
        self.live_station = None;
        self.claim_mode = false;
        self.mouse = [false; 2];
        let Some(s) = &mut self.session else { return };
        tracing::info!("travelling to {} ({})", t.planet.name, t.planet.kind);
        if let Some(yaw) = t.yaw {
            self.yaw = yaw;
        }
        s.arrive(t, now, local);
        self.status = format!("on {}", s.planet.name);
        if let Some(r) = &mut self.renderer {
            r.clear_chunks();
            r.set_tints(&s.planet.tints);
        }
        let Some(s) = self.session.take() else { return };
        let icons = self.build_icons(&s.items, &s.world.registry, &s.textures, &s.planet.tints);
        self.session = Some(Session { icons, ..s });
    }

    fn refuse_catalogue(&mut self, why: String) {
        tracing::error!("{why}");
        self.link = None;
        self.login.busy = false;
        self.login.error = Some(why);
    }

    fn handle_msg(&mut self, m: ServerMsg) {
        let m = match m {
            ServerMsg::Travel(t) => return self.travel(*t),
            ServerMsg::Storage(view) => return self.storage_update(view),
            ServerMsg::Shop(view) => return self.shop_update(view),
            ServerMsg::Atlas(view) => return self.atlas_update(view),
            ServerMsg::AtlasTile { planet, tile, rgb } => {
                if let Some(a) = &mut self.atlas {
                    a.tile(planet, tile, &rgb);
                }
                return;
            }
            ServerMsg::AtlasHeat {
                planet,
                item,
                side,
                bands,
            } => {
                if let Some(a) = &mut self.atlas {
                    a.heat(planet, item, side, &bands);
                }
                return;
            }
            ServerMsg::Helix(view) => {
                if view.closed {
                    if self.menu == Some(Menu::Helix(view.pos)) {
                        self.helix_view = None;
                        self.close_menu(true);
                    }
                } else if self.menu == Some(Menu::Helix(view.pos)) {
                    self.helix_view = Some(view);
                }
                return;
            }
            ServerMsg::GuildControl(view) => {
                if view.closed {
                    if self.menu == Some(Menu::Guild(Some(view.pos))) {
                        self.guild_control_view = None;
                        self.close_menu(true);
                    }
                } else if self.menu == Some(Menu::Guild(Some(view.pos))) {
                    self.guild_control_view = Some(view);
                }
                return;
            }
            ServerMsg::Portal(view) => return self.portal_update(Some(view)),
            ServerMsg::PortalClosed { id } => return self.portal_closed(id),
            ServerMsg::Defeated { debt, by } => {
                // The defeat screen takes the mouse; nothing else stays open.
                self.close_menu(false);
                self.claim_mode = false;
                self.mouse = [false; 2];
                self.set_grab(false);
                if let Some(s) = &mut self.session {
                    s.defeat.defeated(by, debt);
                }
                return;
            }
            ServerMsg::Teleport { spawn, tick, yaw } => {
                if let Some(Menu::Portal(_)) = self.menu {
                    self.close_menu(true);
                }
                if let Some(yaw) = yaw {
                    self.yaw = yaw;
                }
                if let Some(s) = &mut self.session {
                    s.teleport(spawn, tick);
                }
                return;
            }
            // Refused as the portal window opened (not ours, no portal): the
            // reason is the notice, and the window waiting for it goes.
            ServerMsg::Notice { text } if matches!(self.menu, Some(Menu::Portal(_))) && self.portal_view.is_none() => {
                self.close_menu(true);
                ServerMsg::Notice { text }
            }
            m => m,
        };
        let now = self.now();
        let Some(s) = &mut self.session else { return };
        match m {
            // Handled above.
            ServerMsg::Atlas(_) | ServerMsg::AtlasTile { .. } | ServerMsg::AtlasHeat { .. } => {}
            // Milestone 17: a session of ours ended.
            ServerMsg::Creative { at, on } => s.creative(at, on),
            ServerMsg::GuildBody {
                at,
                run,
                grapple,
                lava_percent,
                place_percent,
            } => s.guild_body(at, run, grapple, lava_percent, place_percent),
            ServerMsg::Air { at, air } => s.me.set_air(at, air, &s.movement, s.world.shape, &s.world),
            ServerMsg::Forged { outcome, .. } => {
                let text = crate::forge_screen::outcome_text(&s.items, &outcome);
                self.notices.push(text, now);
            }
            ServerMsg::Creature(v) => s.creatures.view(v),
            ServerMsg::Blast { pos, radius } => {
                let pos = DVec3::from_array(pos.map(|v| v as f64));
                s.shots.blast(pos, radius as f64, s.world.shape);
                s.fx.blast(pos, radius, now);
            }
            ServerMsg::Beam { from, to, ticks } => s.fx.beam(from, DVec3::from_array(to.map(|v| v as f64)), ticks, now),
            ServerMsg::CreatureGone { id } => s.creatures.gone(id),
            ServerMsg::Shot(v) => s.shots.server(&v),
            ServerMsg::ShotEnd { id } => s.shots.end(id),
            ServerMsg::Hit { target, amount, crit } => s.hit(target, amount, crit, now),
            ServerMsg::Chunk { pos, data } => match Chunk::decode(&data) {
                Ok(c) => s.world.insert_chunk(pos, c),
                Err(e) => tracing::warn!("bad chunk {pos:?} from server: {e}"),
            },
            ServerMsg::Sign { anchor, sign } => s.signs.set(anchor, sign),
            ServerMsg::Forget { pos } => {
                s.signs.forget_chunk(s.world.shape.wrap_chunk(pos));
                s.world.forget_chunk(pos);
                if let Some(r) = &mut self.renderer {
                    r.remove_chunk(s.world.shape.wrap_chunk(pos));
                }
            }
            ServerMsg::Far { pos, data } => match bm_core::FarChunk::decode(&data) {
                Ok(c) => s.far.insert(pos, c),
                Err(e) => tracing::warn!("bad far copy {pos:?} from server: {e}"),
            },
            ServerMsg::ForgetFar { pos } => {
                s.far.forget(pos);
                if let Some(r) = &mut self.renderer {
                    r.remove_far(s.world.shape.wrap_chunk(pos));
                }
            }
            ServerMsg::BlockChanged { pos, block } => s.block_changed(pos, block),
            ServerMsg::Character { character, at } => s.character(character, at),
            ServerMsg::Ledger(l) => s.ledger = Some(l),
            ServerMsg::Friends(f) => s.friends = f,
            ServerMsg::Guilds(g) => s.guilds = g,
            ServerMsg::BlockHealth { pos, damage } => s.block_health(pos, damage),
            ServerMsg::EditResult { seq, pos, block, .. } => s.edit_result(seq, pos, block),
            ServerMsg::PlayerJoined { id, name } => s.remotes.joined(id, name),
            ServerMsg::PlayerLeft { id } => s.remotes.left(id),
            ServerMsg::Slots { changes } => s.items.apply_slots(&changes),
            ServerMsg::Station(view) => {
                // Any block of a machine opens it; the answer names the
                // machine's own first block (milestone 11).
                if let Some(Menu::Crafting(Some(opened))) = self.menu
                    && menus::same_machine(s.world.shape, opened, view.pos)
                {
                    self.live_station = Some(LiveStation { view, received: now });
                }
            }
            ServerMsg::Claim(c) => s.claims.upsert(c),
            ServerMsg::ClaimRemoved { id } => s.claims.remove(id),
            ServerMsg::Settlements { settlements, prestige } => s.claims.set_settlements(settlements, prestige),
            ServerMsg::Notice { text } => self.notices.push(text, now),
            ServerMsg::Universe { planets } => s.universe = planets,
            ServerMsg::Chat { lines } => self.chat.receive(lines, now),
            ServerMsg::Reclaim { items } => {
                s.reclaim = items
                    .into_iter()
                    .filter(|i| s.items.registry.contains(i.item) && i.count > 0)
                    .collect();
            }
            ServerMsg::TokenNames { names } => s.items.set_token_names(names),
            ServerMsg::Regions {
                layout_seed,
                names,
                discovered,
            } => s.sky_view.regions(s.world.shape, layout_seed, names, discovered),
            ServerMsg::Discovered { region, xp, .. } => {
                let text = s.sky_view.discovered(region, xp);
                self.notices.push(text, now);
            }
            ServerMsg::Meteor(v) => s.sky_view.meteor(v),
            ServerMsg::MeteorGone { id } => s.sky_view.gone(id),
            ServerMsg::MeteorLife { life } => s.sky_view.life(life),
            ServerMsg::MachineWear { wear, max, .. } => s.machine_wear = Some(((wear, max), now)),
            ServerMsg::CropInfo {
                pos,
                name,
                stage,
                stages,
                ripe_in,
                wild,
                tilled,
                dry,
            } => {
                s.crop_info = Some((
                    CropView {
                        pos,
                        name,
                        stage,
                        stages,
                        ripe_in,
                        wild,
                        tilled,
                        dry,
                    },
                    now,
                ))
            }
            ServerMsg::Welcome(_)
            | ServerMsg::Refused { .. }
            | ServerMsg::Travel(_)
            | ServerMsg::Storage(_)
            | ServerMsg::Portal(_)
            | ServerMsg::PortalClosed { .. }
            | ServerMsg::Shop(_)
            | ServerMsg::GuildControl(_)
            | ServerMsg::Helix(_)
            | ServerMsg::Defeated { .. }
            | ServerMsg::Teleport { .. } => {}
        }
    }

    fn handle_snapshot(&mut self, d: ServerDatagram, reconcile: bool) {
        let local = self.local_ticks();
        if let Some(s) = &mut self.session {
            s.snapshot(d, local, reconcile);
        }
    }

    // ---- claims ----

    fn claim_click(&mut self, add: bool) {
        let now = self.now();
        let Some(s) = &self.session else { return };
        let Some(plot) = s.claim_target else { return };
        let msg = match s.claims.at(plot) {
            Some(c) if c.owner == s.account && add => Err("That plot is already yours.".to_string()),
            Some(c) if c.owner != s.account => Err(format!("That plot belongs to {}.", c.owner)),
            Some(c) => {
                if PlotPos::of(c.stone, s.world.shape) == plot {
                    Err("That plot holds your claim stone. Break the stone to give up the claim.".into())
                } else {
                    Ok(ClientMsg::ReleasePlot { plot })
                }
            }
            None if !add => Err("That plot is not yours to give up.".into()),
            None => match s.claims.adjacent_claim_of(&s.account, plot) {
                Some(claim) => {
                    if s.claims.plots_owned_by(&s.account) as u32 >= s.allowance() {
                        Err(format!("You hold all {} plots you may claim.", s.allowance()))
                    } else {
                        Ok(ClientMsg::ClaimPlot { claim, plot })
                    }
                }
                None => Err("Claim plots next to your claim. A claim stone starts a new claim.".into()),
            },
        };
        match msg {
            Ok(m) => self.send(m),
            Err(text) => self.notices.push(text, now),
        }
    }

    // ---- the frame ----

    fn sample_input(&self) -> PlayerInput {
        let k = |c: KeyCode| self.grabbed && self.menu.is_none() && !self.chat.open && self.keys.contains(&c);
        let (yaw, pitch) = PlayerInput::from_angles(self.yaw, self.pitch);
        PlayerInput {
            forward: k(KeyCode::KeyW) as i8 - k(KeyCode::KeyS) as i8,
            strafe: k(KeyCode::KeyD) as i8 - k(KeyCode::KeyA) as i8,
            jump: k(KeyCode::Space),
            sprint: k(KeyCode::ShiftLeft) || k(KeyCode::ControlLeft),
            yaw,
            pitch,
            slot: self.session.as_ref().map_or(0, |s| s.selected as u8),
            fire: false,
            off: self.session.as_ref().map_or(1, |s| s.selected_off as u8),
            fire_off: false,
            reel: k(KeyCode::KeyR) as i8 - k(KeyCode::KeyF) as i8,
            // Swim down (Boundless's creep key was C, Boundmore's crafting).
            descend: k(KeyCode::KeyX),
        }
    }

    /// `ServerMsg::Atlas`: the open atlas's world, or that it closed.
    fn atlas_update(&mut self, view: bm_protocol::AtlasView) {
        if view.closed {
            if matches!(self.menu, Some(Menu::Atlas(_))) {
                self.close_menu(true);
            }
            self.atlas = None;
            return;
        }
        match &mut self.atlas {
            Some(a) if a.view.planet == view.planet => a.update(view),
            _ => self.atlas = Some(crate::atlas::AtlasMap::new(view)),
        }
    }

    fn update(&mut self) {
        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f64().min(0.25);
        self.last_frame = now;
        self.handle_net();
        if let Some(slot) = self.atlas_request.take() {
            self.open_menu(Menu::Atlas(slot));
        }
        // Defeated (the server said so, or our prediction took its word):
        // the defeat screen holds the mouse until we are back on our feet.
        let down = self.session.as_mut().is_some_and(|s| {
            s.defeat.observe(s.me.state.down);
            s.defeat.is_down()
        });
        if down && (self.grabbed || self.menu.is_some()) {
            self.close_menu(false);
            self.claim_mode = false;
            self.set_grab(false);
        }
        let local = self.local_ticks();

        let input = self.sample_input();
        let playing = self.grabbed && self.menu.is_none() && !self.chat.open;
        let mouse = if playing && !self.claim_mode {
            self.mouse
        } else {
            [false; 2]
        };
        let claim_mode = self.claim_mode;
        // Each button uses its own hand's item, one hand at a time
        // (milestone 28): held both, the one pressed last acts.
        let pressed_hands = [
            std::mem::take(&mut self.attack_pending) && playing && !claim_mode,
            std::mem::take(&mut self.attack_pending_off) && playing && !claim_mode,
        ];
        let hand = if pressed_hands[1] && !pressed_hands[0] {
            1
        } else if pressed_hands[0] {
            0
        } else if mouse[self.last_hand] || !mouse[1 - self.last_hand] {
            self.last_hand
        } else {
            1 - self.last_hand
        };
        self.last_hand = hand;
        let shift = self.keys.contains(&KeyCode::ShiftLeft) || self.keys.contains(&KeyCode::ShiftRight);
        // A press of the acting hand's button, however short (a sling's click).
        let mut pressed = pressed_hands[hand];
        let (yaw, pitch) = (self.yaw, self.pitch);
        let now_s = self.now();
        // `--demo-ui bombs`: a hand on the button, on a cycle.
        let demo_held = self.demo_bombs && now_s % DEMO_BOMB_CYCLE < DEMO_BOMB_HOLD;
        pressed |= demo_held && !self.demo_was_held;
        self.demo_was_held = demo_held;
        let Some(s) = &mut self.session else { return };
        let Some(link) = &mut self.link else { return };
        s.update_journey(now_s);
        // Nothing is broken or placed on the way between planets.
        let use_down = !s.travelling() && mouse[hand];
        // A grapple in a hand fires (and lets go) with that hand's button,
        // inside the simulation, instead of breaking or placing (milestone
        // 28). Through its traits, as the server steps it (built again for
        // each tick below, with the holder as they stand before that step).
        let slots = [s.selected, s.selected_off];
        let grapples = slots.map(|i| s.items.grapple_of(s.items.slot(i).as_ref(), &Held::default()));
        let stopped = !playing || claim_mode || s.travelling();
        if grapples[0].is_none() || stopped {
            self.fire_pending = false;
        }
        if grapples[1].is_none() || stopped {
            self.fire_pending_off = false;
        }
        // The acting hand's button: what it does to the world. A grapple
        // fires (above); a block places; a chisel cuts, or with Shift fills;
        // anything else is used (swung, shot, eaten).
        let acting = s.items.slot(slots[hand]);
        let place_hand = acting.is_some_and(|h| s.place_choice(h.item).is_some());
        let chisel_hand = acting
            .and_then(|h| s.items.def(&h))
            .is_some_and(|d| d.chisel().is_some());
        let mouse = if grapples[hand].is_some() {
            [false; 2]
        } else if chisel_hand {
            [use_down && !shift, use_down && shift]
        } else if place_hand {
            [false, use_down]
        } else {
            [use_down, false]
        };

        // Fixed-rate simulation, decoupled from frame rate. Keeps ticking when
        // the window is hidden or a menu is open, so the server keeps
        // receiving inputs.
        self.accumulator += dt;
        // Until the chunks around us arrive they read as solid here but not
        // on the server, and every tick would be a misprediction. Wait; the
        // server simulates us only on our inputs, so it waits too.
        if !s.world.has_chunks_around(s.me.state.pos) {
            self.accumulator = 0.0;
        }
        if let Some(pose) = self.demo_pose.take_if(|p| {
            p.hook
                .and_then(|h| h.stuck)
                .is_some_and(|b| s.world.block(b).is_some_and(|id| s.world.flags.solid(id)))
        }) {
            s.me.teleport(pose);
        }
        // The first input this frame runs: a swing lands no earlier.
        let first_seq = s.me.seq + 1;
        let mut ticks = 0;
        while self.accumulator >= TICK_DT && ticks < MAX_TICKS_PER_FRAME {
            self.accumulator -= TICK_DT;
            ticks += 1;
            let input = PlayerInput {
                fire: std::mem::take(&mut self.fire_pending),
                fire_off: std::mem::take(&mut self.fire_pending_off),
                ..input
            };
            let held = s.held_now(local);
            let grapple =
                [0, 1].map(|h| grapples[h].and_then(|_| s.items.grapple_of(s.items.slot(slots[h]).as_ref(), &held)));
            // The body quirks of what both hands hold (milestone 17), as the
            // server sets them for this input.
            let quirks =
                bm_core::forge::Quirks::of_hands(s.items.slot(slots[0]).as_ref(), s.items.slot(slots[1]).as_ref());
            let seq = s.me.tick(input, grapple, quirks, &s.movement, s.world.shape, &s.world);
            link.send_datagram(ClientDatagram::Input {
                latest: seq,
                inputs: s.me.recent_inputs(INPUT_REDUNDANCY),
            });
        }
        if ticks == MAX_TICKS_PER_FRAME {
            self.accumulator = 0.0;
        }
        s.me.decay_correction(dt);

        // Creatures where we draw them, shots in the air, and what we lost.
        let shape = s.world.shape;
        s.seen = match s.remotes.server_tick(local) {
            Some(t) => s.creatures.sample(t, shape),
            None => Vec::new(),
        };
        s.creatures.animate(&s.seen, dt as f32, shape);
        {
            let (world, creatures, seen) = (&s.world, &s.creatures, &s.seen);
            let solid = |p: BlockPos| world.block(p).is_some_and(|b| world.flags.solid(b));
            // Where a homing bolt steers: its target's chest. Ours is where
            // we are now; everyone else is where we draw them.
            let chest = s.movement.eye_height - 0.6;
            let me = (s.player_id, s.me.state.pos + DVec3::new(0.0, chest, 0.0));
            let remotes = s.remotes.sample(local, shape);
            let target = |t: Target| match t {
                Target::Player(id) if id == me.0 => Some(me.1),
                Target::Player(id) => remotes
                    .iter()
                    .find(|r| r.0 == id)
                    .map(|r| r.1.pos + DVec3::new(0.0, chest, 0.0)),
                Target::Creature(id) => seen.iter().find(|c| c.id == id).and_then(|c| {
                    creatures
                        .kind(c.kind)
                        .map(|k| c.pos + DVec3::new(0.0, k.height as f64 * 0.5, 0.0))
                }),
            };
            let creature = |a: DVec3, b: DVec3| {
                seen.iter().filter(|c| !c.dead).any(|c| {
                    creatures
                        .hit_box(c, shape.nearest_lap(a, c.pos))
                        .is_some_and(|(lo, hi)| crate::combat::segment_hits(a, b, lo, hi))
                })
            };
            for _ in 0..ticks {
                s.shots.tick(shape, solid, creature, target);
            }
        }
        s.floaters.update(now_s);
        s.fx.update(now_s);
        if let Some(fall) = s.losses.observe(s.me.state.hurt, now_s) {
            s.floaters.add(Anchor::Me, fall, false, true, now_s);
        }
        if s.me.state.hurt > s.last_hurt {
            s.hurt_at = now_s;
        }
        s.last_hurt = s.me.state.hurt;
        // Skill changes the server never answered were refused (it said
        // why): what they changed goes.
        let bodies = s.progress.expire(s.me.seq);
        s.set_bodies(bodies);

        // Targeting and editing.
        let eye = s.me.state.eye(&s.movement);
        let dir = Camera {
            eye,
            yaw,
            pitch,
            fov_y: 1.0,
        }
        .forward();
        s.aim(eye, dir);
        let world = &s.world;
        let solid = |p: BlockPos| world.block(p).is_some_and(|b| world.flags.solid(b));
        s.claim_target = claim_mode.then(|| {
            let at = match raycast(eye, dir, CLAIM_AIM, solid) {
                Some(hit) => hit.block,
                None => {
                    let p = eye + dir * CLAIM_AIM_AIR;
                    BlockPos::containing(p.x, p.y, p.z)
                }
            };
            PlotPos::of(at, world.shape)
        });

        let slot = slots[hand];
        let held = s.items.slot(slot);
        // What the held stack does goes through its traits (milestone 17),
        // with the quirks' conditions as the server will judge them.
        let held_by = s.held_now(local);
        // A chisel in hand cuts with the left button and fills with the
        // right, instead of breaking and placing.
        let chisel = held.and_then(|h| s.items.def(&h)).and_then(|d| d.chisel());
        let advanced = held.and_then(|h| s.items.def(&h)).is_some_and(|d| d.chisel_advanced());
        s.place_cooldown -= dt;
        s.chisel = None;
        if let (Some((family, precise, _)), Some(hit)) = (chisel, s.target) {
            // The chisels epic makes any chisel precise (milestone 12).
            let precise = precise || s.progress.effects().chisel_epic;
            let shape = s.world.block_shape(hit.block);
            let plan = crate::chisel::plan(&hit, shape, precise);
            s.chisel = Some((hit.block, plan));
            let click = if mouse[0] {
                plan.cut.map(|m| (m, false))
            } else if mouse[1] {
                plan.fill.map(|m| (m, true))
            } else {
                None
            };
            if let Some((octants, restore)) = click
                && s.place_cooldown <= 0.0
            {
                s.place_cooldown = PLACE_REPEAT * 100.0 / (100.0 + s.place_percent as f64);
                let state = s.world.state(hit.block).unwrap_or(BlockState::AIR);
                let def = s.world.registry.get(state.id);
                match bm_core::chisel::apply(def, state, family, precise, advanced, octants, restore) {
                    Ok(new) => {
                        // Filling back over ourselves would be refused.
                        let filled = Shape::new(family, !octants).unwrap_or(Shape::FULL);
                        let corner = DVec3::new(hit.block.x as f64, hit.block.y as f64, hit.block.z as f64);
                        let feet = s.world.shape.nearest_lap(corner, s.me.state.pos);
                        let m = &s.movement;
                        let in_me = restore
                            && filled.boxes().any(|(lo, hi)| {
                                let (lo, hi) = (corner + DVec3::from_array(lo), corner + DVec3::from_array(hi));
                                feet.x - m.half_width < hi.x
                                    && feet.x + m.half_width > lo.x
                                    && feet.y < hi.y
                                    && feet.y + m.height > lo.y
                                    && feet.z - m.half_width < hi.z
                                    && feet.z + m.half_width > lo.z
                            });
                        if !in_me {
                            s.edit_seq += 1;
                            // Energy: a cut is spent before anything else is
                            // ruled, as the server does; too tired, it is sent
                            // but cuts nothing.
                            let cost = s.items.energy_of(held.as_ref(), &held_by).unwrap_or(0);
                            let at = s.me.seq;
                            if s.me.spend(
                                at,
                                cost,
                                bm_core::food::Drain::Tool,
                                &s.movement,
                                s.world.shape,
                                &s.world,
                            ) {
                                s.world
                                    .predict_edit(s.edit_seq, hit.block, BlockState { shape: new, ..state });
                            }
                            link.send(ClientMsg::Edit {
                                seq: s.edit_seq,
                                at: s.me.seq,
                                pos: hit.block,
                                action: EditAction::Chisel {
                                    slot: slot as u8,
                                    octants,
                                    restore,
                                },
                            });
                        }
                    }
                    Err(bm_core::chisel::Refused::Told(text)) => self.notices.push(text, now_s),
                    Err(bm_core::chisel::Refused::Quiet(_)) => {}
                }
            }
        }
        let mut mouse = if chisel.is_some() { [false; 2] } else { mouse };
        // Twitchy (milestone 17): an item in hand that went off by itself is
        // used once, as a click would. A grapple went off inside the step.
        if std::mem::take(&mut s.me.twitched) && playing && !claim_mode && !s.travelling() {
            mouse[0] = true;
            pressed = true;
        }
        // Fighting (milestone 13). A creature nearer than the terrain is
        // what we aim at. A sling shoots (never breaking blocks); a tool or
        // the hand stone strikes a living creature in reach; a body in reach
        // is looted once a press. Slings, strikes and swings share one
        // cooldown (`swinger`), as on the server.
        let effects = s.progress.effects().brewed(&s.me.state);
        let sling = s.items.sling_of(held.as_ref(), &held_by);
        let tool = s.items.tool_of(held.as_ref(), &held_by);
        let world = &s.world;
        let terrain = raycast(eye, dir, CREATURE_AIM, |p| {
            world.block(p).is_some_and(|b| world.flags.solid(b))
        })
        .map_or(CREATURE_AIM, |h| h.distance);
        s.aim_creature = s.creatures.pick(&s.seen, eye, dir, terrain, s.world.shape, false);
        let attack_held = mouse[0] || demo_held;
        // Food in hand eats with a press, a bucket fills at water (milestone
        // 16); neither breaks blocks.
        // Looking at a crop: ask the server about it now and then.
        if let Some(hit) = s.target
            && s.world
                .block(hit.block)
                .is_some_and(|b| s.world.registry.get(b).crop > 0)
            && s.crop_asked
                .is_none_or(|(p, t)| p != hit.block || now_s - t > CROP_ASK_SECONDS)
        {
            s.crop_asked = Some((hit.block, now_s));
            link.send(ClientMsg::LookCrop { pos: hit.block });
        }
        let food = held.and_then(|h| s.items.def(&h)).and_then(|d| d.food());
        let bucket = held
            .and_then(|h| s.items.def(&h))
            .is_some_and(|d| matches!(d.kind, bm_core::items::ItemKind::Bucket { .. }));
        // An atlas in hand opens its map with a press (milestone 27).
        let atlas = held
            .and_then(|h| s.items.def(&h))
            .is_some_and(|d| matches!(d.kind, bm_core::items::ItemKind::Atlas));
        if food.is_some() || bucket || atlas {
            mouse[0] = false;
        }
        if atlas && pressed && !s.travelling() && !claim_mode {
            self.atlas_request = Some(slot as u8);
        }
        if let Some(food) = food.filter(|_| pressed && !s.travelling() && !claim_mode) {
            match s.me.state.can_eat(&food) {
                Ok(()) => {
                    // Right after the latest input, on both sides.
                    let at = s.me.seq;
                    s.me.eat(at, food, &s.movement, s.world.shape, &s.world);
                    link.send(ClientMsg::Eat { at, slot: slot as u8 });
                }
                Err(bm_core::food::Refusal::Full) => self.notices.push("You are full".into(), now_s),
                Err(_) => {}
            }
        }
        if bucket && pressed && !s.travelling() && !claim_mode {
            let world = &s.world;
            let reach = s.reach();
            match raycast(eye, dir, reach, |p| {
                world
                    .block(p)
                    .is_some_and(|b| world.flags.solid(b) || bm_core::PhysicsWorld::liquid(world, p))
            }) {
                Some(hit) if bm_core::PhysicsWorld::liquid(world, hit.block) => link.send(ClientMsg::FillBucket {
                    slot: slot as u8,
                    pos: hit.block,
                }),
                _ => self.notices.push("Fill a bucket at water".into(), now_s),
            }
        }
        let bomb = s.items.bomb_of(held.as_ref(), &held_by);
        if let Some(b) = bomb.filter(|_| !s.me.state.down && !s.travelling() && !claim_mode) {
            // A bag of bombs: hold to light, let go to throw (a fuse run out
            // in hand throws it too), on the shared cooldown.
            mouse[0] = false;
            s.trigger.reset();
            let skilled = effects.skilled_bomb(&b);
            if let Some(toss) = s
                .fuse
                .update(pressed, attack_held, s.swinger.ready_at(), first_seq, s.me.seq)
            {
                s.swinger.swung(toss.at, swing_ticks(skilled.action_speed));
                // Spent at the throw, as the server will; too tired, it is
                // still sent (the server restarts the pause too) but nothing
                // is thrown.
                let spent = s.me.spend(
                    toss.at,
                    skilled.energy,
                    bm_core::food::Drain::Ranged,
                    &s.movement,
                    s.world.shape,
                    &s.world,
                );
                link.send(ClientMsg::Throw {
                    at: toss.at,
                    slot: slot as u8,
                    yaw: input.yaw,
                    pitch: input.pitch,
                    hold: toss.hold,
                });
                if spent {
                    s.shots.throw(
                        s.me.state.eye(&s.movement),
                        bm_core::movement::look_dir(input.yaw, input.pitch),
                        bm_core::combat::throw_speed(toss.hold as u32, skilled.throw_speed),
                        bm_core::combat::BOMB_FUSE_TICKS.saturating_sub(toss.hold as u32),
                    );
                }
            }
        } else if let Some(l) = sling.filter(|_| !s.me.state.down && !s.travelling() && !claim_mode) {
            mouse[0] = false;
            s.fuse.reset();
            let skilled = effects.skilled_sling(&l);
            let mode = FireMode::of(&effects);
            if let Some(fire) = s
                .trigger
                .update(mode, pressed, attack_held, s.swinger.ready_at(), first_seq, s.me.seq)
            {
                s.swinger.swung(fire.at, swing_ticks(skilled.action_speed));
                // Spent right after its input, as the server will; too tired,
                // the shot is still sent (the server restarts the pause too)
                // but nothing flies.
                let spent = s.me.spend(
                    fire.at,
                    skilled.energy,
                    bm_core::food::Drain::Ranged,
                    &s.movement,
                    s.world.shape,
                    &s.world,
                );
                link.send(ClientMsg::Shoot {
                    at: fire.at,
                    slot: slot as u8,
                    yaw: input.yaw,
                    pitch: input.pitch,
                    charge: fire.charge,
                });
                if spent {
                    s.shots.fire(
                        s.me.state.eye(&s.movement),
                        bm_core::movement::look_dir(input.yaw, input.pitch),
                        skilled.shot_speed as f64,
                        skilled.range as f64,
                        skilled.falloff,
                    );
                }
            }
        } else {
            s.trigger.reset();
            s.fuse.reset();
            if attack_held
                && let Some((id, distance)) = s.aim_creature
                && distance <= s.reach()
                && let Some(c) = s.creatures.get(id).cloned()
            {
                // Aimed at a creature in reach: the block behind it is safe.
                mouse[0] = false;
                if c.dead {
                    if !c.looted && !s.looted_this_press {
                        s.looted_this_press = true;
                        link.send(ClientMsg::Loot { creature: id });
                    }
                } else if let Some(tool) = tool {
                    if let Some(at) = s.swinger.next(first_seq, s.me.seq) {
                        s.swinger
                            .swung(at, swing_ticks(skilled_tool(&tool, &effects).action_speed));
                        s.me.spend(
                            at,
                            tool.energy,
                            bm_core::food::Drain::Tool,
                            &s.movement,
                            s.world.shape,
                            &s.world,
                        );
                        link.send(ClientMsg::Strike {
                            at,
                            creature: id,
                            slot: slot as u8,
                        });
                    }
                } else if !s.told_no_weapon {
                    s.told_no_weapon = true;
                    self.notices.push(NEED_A_WEAPON.into(), now_s);
                }
            }
        }
        if !attack_held {
            s.looted_this_press = false;
            s.told_no_weapon = false;
        }
        // Swinging: holding break swings the selected tool at once, then
        // every `swing_ticks` of the tool that made the last swing (a bare
        // hand, or a non-tool, breaks nothing: say so once per press). The server rules each swing on the same input
        // timeline (ADR-0004); `Session::swing` predicts what it will do.
        s.damage.heal(s.me.seq);
        let has_tool = tool.is_some() || s.me.state.body.creative;
        if !mouse[0] {
            s.told_no_tool = false;
        } else if !has_tool && s.target.is_some() && !s.told_no_tool {
            s.told_no_tool = true;
            self.notices.push(NEED_A_TOOL.into(), now_s);
        }
        if mouse[0]
            && let Some(hit) = s.target
            && let Some(at) = s.swinger.next(first_seq, s.me.seq)
            && let Some(msg) = s.swing(
                hit.block,
                at,
                slot,
                bm_core::movement::look_dir(input.yaw, input.pitch),
                &held_by,
            )
        {
            link.send(msg);
        }

        // Placing spends one of the selected item, so only a block item can.
        // A prop (a torch) goes on the face clicked: on top, or hanging on a
        // side; never under a block, and only on a solid one.
        let place = held.and_then(|h| s.place_choice(h.item));
        if mouse[1]
            && s.place_cooldown <= 0.0
            && let (Some(hit), Some((block, mount))) = (s.target, place)
        {
            let at = hit.adjacent();
            let free = s.world.block(at).is_some_and(|b| b.is_air() || s.world.flags.liquid(b));
            let blocks_me = s.world.registry.get(block).solid && s.me.state.overlaps_block(&s.movement, at);
            if free && !blocks_me && s.world.shape.contains_y(at.y) {
                s.edit_seq += 1;
                // The block takes the stack's colour, as the server will.
                let colour = match held {
                    Some(h) if s.world.registry.get(block).colourable => h.colour,
                    _ => Colour::NATURAL,
                };
                let mut placed = BlockState::coloured(block, colour);
                let def = s.world.registry.get(block);
                if def.swing.is_some() || def.form.is_some() {
                    let centre = DVec3::new(at.x as f64 + 0.5, at.y as f64 + 0.5, at.z as f64 + 0.5);
                    let d = s.world.shape.delta(centre, s.me.state.pos);
                    let support = mount.and_then(|m| {
                        let (dx, dy, dz) = m.support();
                        s.world.state(at.offset(dx, dy, dz))
                    });
                    placed.shape = bm_core::forms::placed_shape(def, mount, support, d.x, d.z);
                }
                s.world.predict_edit(s.edit_seq, at, placed);
                link.send(ClientMsg::Edit {
                    seq: s.edit_seq,
                    at: s.me.seq,
                    pos: at,
                    action: EditAction::Place {
                        slot: slot as u8,
                        mount,
                    },
                });
                s.place_cooldown = PLACE_REPEAT * 100.0 / (100.0 + s.place_percent as f64);
            }
        }

        // Meshing and uploads.
        let center = BlockPos::containing(eye.x, eye.y, eye.z).chunk();
        s.world
            .schedule_meshing(s.world.shape.wrap_chunk(center), rayon::current_num_threads() * 2);
        // Far meshes after near ones (both on the pool, near queued first).
        let center = s.world.shape.wrap_chunk(center);
        s.far.schedule_meshing(center, rayon::current_num_threads());
        if let Some(r) = &mut self.renderer {
            for m in s.world.take_meshes() {
                r.upload_chunk(m.pos, &m.mesh);
            }
            for m in s.far.take_meshes() {
                r.upload_far(m.pos, &m.mesh);
            }
        }
    }

    /// Runs this frame's menus and HUD and returns them tessellated.
    fn run_ui(&mut self) -> Option<(Vec<egui::ClippedPrimitive>, egui::TexturesDelta, f32)> {
        self.ensure_coloured_icons();
        let local = self.local_ticks();
        let window = self.window.clone()?;
        let e = self.egui.as_mut()?;
        let mut raw = e.winit.take_egui_input(&window);
        if self.chat.swallow {
            // The Enter or T that opened the chat line is not part of it.
            raw.events
                .retain(|ev| !matches!(ev, egui::Event::Text(_) | egui::Event::Key { .. }));
            self.chat.swallow = false;
        }
        let now = self.start.elapsed().as_secs_f64();
        let mut actions = Vec::new();
        let mut chat_actions = Vec::new();
        let chat = &mut self.chat;
        let storage_view = self.storage_view.as_ref();
        let portal_view = self.portal_view.as_ref();
        let shop_view = self.shop_view.as_ref();
        let shop_prices = &mut self.shop_prices;
        let friend_name = &mut self.friend_name;
        let guild_form = &mut self.guild_form;
        let guild_control_view = self.guild_control_view.as_ref();
        let helix_view = self.helix_view.as_ref();
        let mut atlas = self.atlas.as_mut();
        let facing = self.yaw;
        let unix_now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let notices = self.notices.visible(now);
        let menu = self.menu;
        let login = &mut self.login;
        let drag = &mut self.drag;
        let session = self.session.as_ref();
        let live = self.live_station.as_ref();
        let claim_mode = self.claim_mode;
        let shrine_tab = &mut self.shrine_tab;
        let skill_ui = &mut self.skill_ui;
        let forge_ui = &mut self.forge_ui;
        let sign_ui = &mut self.sign_ui;
        let alpha = self.accumulator / TICK_DT;
        let (yaw, pitch) = (self.yaw, self.pitch);
        let mut return_pressed = false;
        let out = e.ctx.run_ui(raw, |ui| {
            let ctx = ui.ctx().clone();
            let Some(s) = session else {
                menus::login(&ctx, login, &mut actions);
                return;
            };
            let shape = s.world.shape;
            let feet = s.me.state.pos;
            let here = PlotPos::of(BlockPos::containing(feet.x, feet.y, feet.z), shape);
            let owner = s.claims.at(here).map(|c| (c.owner.clone(), c.owner == s.account));
            let settlement = s
                .claims
                .at(here)
                .and_then(|c| menus::claim_line(s.claims.prestige(c.id), s.claims.settlement_of(c.id)));
            let claim_help = claim_mode.then(|| {
                format!(
                    "Left click a plot next to your claim to add it. Right click one of yours to give it up.\n\
                     {} of {} plots claimed. P to leave.",
                    s.claims.plots_owned_by(&s.account),
                    s.allowance()
                )
            });
            let aim_hint = if menu.is_some() || claim_mode {
                None
            } else if s.aiming_at_shrine().is_some() {
                Some(("E", "travel"))
            } else if s.aiming_at_storage().is_some() {
                Some(("E", "open"))
            } else if s.aiming_at_portal().is_some() {
                Some(("E", "portal"))
            } else if s.aiming_at_heart().is_some() {
                Some(("E", "take your share"))
            } else {
                None
            };
            menus::hud(
                &ctx,
                &Hud {
                    items: &s.items,
                    icons: &s.icons,
                    selected: s.selected,
                    selected_off: s.selected_off,
                    notices: notices.clone(),
                    owner,
                    settlement,
                    claim_mode: claim_help,
                    aim_hint,
                    crop: s
                        .crop_info
                        .as_ref()
                        .filter(|(v, _)| menu.is_none() && s.target.is_some_and(|t| t.block == v.pos))
                        .map(|(v, at)| v.line(now - at)),
                    reclaim_waiting: !s.reclaim.is_empty(),
                    wear_bar: s
                        .machine_wear
                        .filter(|&(_, at)| menu.is_none() && now - at < WEAR_BAR_SECONDS)
                        .map(|(w, _)| w),
                    block_bar: (menu.is_none() && !claim_mode && s.chisel.is_none() && !s.holding_spanner())
                        .then(|| s.target_health())
                        .flatten()
                        .map(|(_, left, shields)| BlockBar { left, shields }),
                    energy: s.me.state.energy() as f32 / s.me.state.full_energy().max(1) as f32,
                    hunger: s.me.state.hunger_points() as f32 / s.me.state.full_energy().max(1) as f32,
                    food: crate::menus::food_tags(&s.me.state),
                    level: Some(bm_core::skills::level_of(s.progress.character().xp)),
                    health: (s.me.state.health(), s.me.state.max_health()),
                    debt: s.progress.character().debt,
                    region: s.sky_view.place(feet),
                    meteor: s.sky_view.status(feet, shape, s.server_tick(local)),
                    life: s.sky_view.life_text(),
                    air: crate::menus::air_view(&s.me.state),
                },
            );
            // Written signs, drawn in their planes under every window.
            if !s.signs.is_empty() {
                let eye = s.render_eye(alpha);
                let camera = Camera {
                    eye,
                    yaw,
                    pitch,
                    fov_y: FOV_Y,
                };
                crate::signs::draw(
                    &ctx.layer_painter(egui::LayerId::background()),
                    &s.signs,
                    &crate::signs::DrawView {
                        world: &s.world,
                        colours: &s.items.colours,
                        eye,
                        forward: camera.forward().as_vec3(),
                        fov_y: FOV_Y,
                        screen: ctx.content_rect(),
                        sky_light: s.planet.day.sky_light(s.sky_tick(local)),
                    },
                );
            }
            // Fighting: name plates, numbers, the hurt flash, the charge
            // ring and the defeat screen.
            {
                let eye = s.render_eye(alpha);
                let forward = Camera {
                    eye,
                    yaw,
                    pitch,
                    fov_y: FOV_Y,
                }
                .forward()
                .as_vec3();
                let screen = ctx.content_rect();
                let plates = if menu.is_none() {
                    s.plates(eye, forward, screen)
                } else {
                    Vec::new()
                };
                let numbers = s.numbers(eye, forward, screen, now);
                let flash = (1.0 - (now - s.hurt_at) / crate::fight_hud::FLASH_SECONDS).clamp(0.0, 1.0) as f32;
                return_pressed = crate::fight_hud::show(
                    &ctx,
                    &crate::fight_hud::FightView {
                        flash,
                        plates: &plates,
                        numbers: &numbers,
                        charge: s.trigger.charge(s.me.seq),
                        fuse: s.fuse.burned(s.me.seq).map(|burned| {
                            let held = s.fuse.held_for(s.me.seq).unwrap_or(0);
                            let power = held.min(bm_core::combat::THROW_FULL_TICKS) as f32
                                / bm_core::combat::THROW_FULL_TICKS as f32;
                            (burned, power)
                        }),
                        defeat: s.defeat.details(),
                    },
                );
            }
            match menu {
                Some(Menu::Inventory) => menus::inventory(
                    &ctx,
                    &s.items,
                    &s.icons,
                    s.selected,
                    s.ledger.as_ref().map(|l| l.coin),
                    drag,
                    &mut actions,
                ),
                Some(Menu::Crafting(station)) => {
                    let station = station.and_then(|pos| {
                        let block = s.world.block(pos)?;
                        let kind = menus::station_of(s.world.registry.get(block))?;
                        Some((pos, kind, menus::station_name(&s.world.registry, kind)))
                    });
                    // A forge has its own window (milestone 17).
                    if let Some((opened, bm_core::Station::Forge, title)) = station.clone() {
                        crate::forge_screen::forge(
                            &ctx,
                            &ForgeWindow {
                                items: &s.items,
                                icons: &s.icons,
                                title,
                                opened,
                                live,
                                now,
                                forger: s.progress.effects().forger(),
                            },
                            forge_ui,
                            &mut actions,
                        );
                        return;
                    }
                    menus::crafting(
                        &ctx,
                        &CraftingView {
                            items: &s.items,
                            icons: &s.icons,
                            station,
                            live,
                            now,
                            progress: &s.progress,
                            craft_colours: &s.craft_colours,
                        },
                        &mut actions,
                    );
                }
                Some(Menu::Travel(shrine)) => menus::travel(
                    &ctx,
                    &TravelView {
                        planets: &s.universe,
                        tick: s.server_tick(local),
                        here: s.planet.id,
                        shrine,
                        items: &s.items,
                        icons: &s.icons,
                        reclaim: &s.reclaim,
                    },
                    shrine_tab,
                    &mut actions,
                ),
                Some(Menu::Storage(pos)) => {
                    let title = s
                        .world
                        .block(pos)
                        .map(|b| s.world.registry.get(b))
                        .filter(|d| d.storage > 0)
                        .map(|d| d.display_name())
                        .unwrap_or_else(|| "Storage".into());
                    let slots = storage_view.filter(|v| v.pos == pos).map(|v| v.slots.as_slice());
                    menus::storage(
                        &ctx,
                        &StorageWindow {
                            items: &s.items,
                            icons: &s.icons,
                            selected: s.selected,
                            title,
                            pos,
                            slots,
                        },
                        drag,
                        &mut actions,
                    );
                    // Its prices and till, if it is our shop (milestone 24).
                    if let Some(v) = shop_view.filter(|v| v.pos == pos && v.mine) {
                        menus::shop_owner(
                            &ctx,
                            v,
                            &s.items,
                            s.ledger.as_ref().map_or(0, |l| l.coin),
                            shop_prices,
                            &mut actions,
                        );
                    }
                }
                Some(Menu::Feats) => menus::feats(&ctx, &FEAT_BOOK, s.ledger.as_ref(), &mut actions),
                Some(Menu::Atlas(slot)) => match atlas.as_mut() {
                    Some(a) => {
                        let name = |i: ItemId| s.items.registry.get(i).map_or_else(String::new, |d| d.display.clone());
                        let loaded = s
                            .items
                            .slot(slot as usize)
                            .and_then(|st| bm_core::atlas::loaded(st.tag))
                            .map(name);
                        // What an atlas can find: whatever some block drops.
                        let found: std::collections::HashSet<ItemId> = s
                            .world
                            .registry
                            .iter()
                            .filter(|(_, d)| bm_core::atlas::finds(d))
                            .filter_map(|(b, _)| s.items.registry.drop_of(b))
                            .collect();
                        let mut loadable: Vec<(u8, String)> = Vec::new();
                        for i in 0..bm_core::INVENTORY_SLOTS {
                            if let Some(st) = s.items.slot(i)
                                && i != slot as usize
                                && st.colour == Colour::NATURAL
                                && found.contains(&st.item)
                                && !loadable.iter().any(|(_, n)| *n == name(st.item))
                            {
                                loadable.push((i as u8, name(st.item)));
                            }
                        }
                        let here = crate::atlas::Here {
                            planet: s.planet.id,
                            pos: [s.me.state.pos.x, s.me.state.pos.z],
                            yaw: facing,
                            account: &s.account,
                            claims: s.claims.iter().collect(),
                            settlements: s.claims.settlements(),
                            loaded,
                            loadable,
                        };
                        a.show(&ctx, &here, &mut actions);
                    }
                    None => {
                        menus::window("Atlas").show(&ctx, |ui| {
                            ui.label(egui::RichText::new("Opening the map").color(menus::MUTED));
                        });
                    }
                },
                Some(Menu::Helix(at)) => {
                    menus::helix(&ctx, helix_view.filter(|v| v.pos == at), unix_now, &mut actions);
                }
                Some(Menu::Guild(at)) => {
                    let feet = s.me.state.pos;
                    let here = PlotPos::of(BlockPos::containing(feet.x, feet.y, feet.z), s.world.shape);
                    let mine = s.claims.at(here).filter(|c| c.owner == s.account);
                    let control = guild_control_view.filter(|v| Some(v.pos) == at);
                    menus::guild(&ctx, control, &s.guilds, mine, &s.account, guild_form, &mut actions);
                }
                Some(Menu::Friends) => {
                    let feet = s.me.state.pos;
                    let here = PlotPos::of(BlockPos::containing(feet.x, feet.y, feet.z), s.world.shape);
                    let mine = s.claims.at(here).filter(|c| c.owner == s.account);
                    menus::friends(&ctx, &s.friends, mine, friend_name, &mut actions);
                }
                Some(Menu::Shop(pos)) => menus::shop(
                    &ctx,
                    shop_view.filter(|v| v.pos == pos),
                    &s.items,
                    &s.icons,
                    s.ledger.as_ref().map_or(0, |l| l.coin),
                    &mut actions,
                ),
                Some(Menu::Skills) => {
                    let wait = s.progress.switch_wait(s.server_tick(local));
                    crate::skill_screen::skills(
                        &ctx,
                        &SkillView {
                            progress: &s.progress,
                            switch_wait: wait,
                            at_home: s.at_home(),
                        },
                        skill_ui,
                        &mut actions,
                    );
                }
                Some(Menu::Sign(pos)) => match sign_ui.as_mut().map(|u| crate::signs::window(&ctx, u)) {
                    Some(Some(crate::signs::SignAction::Write(text))) => {
                        actions.push(UiAction::Send(ClientMsg::WriteSign { pos, text }));
                        actions.push(UiAction::CloseMenu);
                    }
                    Some(Some(crate::signs::SignAction::Close)) | None => actions.push(UiAction::CloseMenu),
                    Some(None) => {}
                },
                Some(Menu::Portal(pos)) => menus::portal(
                    &ctx,
                    &PortalWindow {
                        items: &s.items,
                        icons: &s.icons,
                        pos,
                        view: portal_view,
                    },
                    &mut actions,
                ),
                None => {}
            }
            chat_actions = chat.show(&ctx, CHAT_BOTTOM, now);
            if let Some(j) = &s.journey {
                let alpha = match j.landed {
                    Some(l) => (1.0 - (now - l) / ARRIVAL_FADE).clamp(0.0, 1.0) as f32,
                    None => 1.0,
                };
                menus::travelling(&ctx, &j.to, alpha, now);
            }
        });
        e.winit.handle_platform_output(&window, out.platform_output);
        let prims = e.ctx.tessellate(out.shapes, out.pixels_per_point);
        let ppp = out.pixels_per_point;
        self.apply_actions(actions);
        if return_pressed {
            let now = self.now();
            if self.session.as_mut().is_some_and(|s| s.defeat.press_return(now)) {
                self.send(ClientMsg::Return);
            }
        }
        for a in chat_actions {
            match a {
                ChatAction::Send(m) => self.send(m),
                ChatAction::Close => self.close_chat(),
                ChatAction::Creative => self.toggle_creative(),
            }
        }
        Some((prims, out.textures_delta, ppp))
    }

    /// Claim outlines for claim mode: (shown, through terrain).
    fn claim_lines(&self, eye: DVec3) -> (Vec<LineVertex>, Vec<LineVertex>) {
        let mut lines = Vec::new();
        let Some(s) = &self.session else {
            return (lines, Vec::new());
        };
        if !self.claim_mode {
            return (lines, Vec::new());
        }
        let shape = s.world.shape;
        let mut push = |plots: &HashSet<PlotPos>, color: [f32; 4], lift: f32| {
            for edge in outline(plots, shape) {
                for p in edge.endpoints(shape, eye) {
                    // Pushed a hair outwards so lines on block edges win the
                    // depth test against the terrain they lie on.
                    let rel = (p - eye).as_vec3();
                    let rel = rel + rel.normalize_or_zero() * -lift;
                    lines.push(LineVertex {
                        pos: rel.to_array(),
                        color,
                    });
                }
            }
        };
        let size = bm_core::PLOT_SIZE as f64;
        for c in s.claims.iter() {
            let near = c.plots.iter().any(|p| {
                let centre = DVec3::new(p.x as f64 + 0.5, p.y as f64 + 0.5, p.z as f64 + 0.5) * size;
                let d = shape.delta(eye, centre);
                d.x.hypot(d.z) < CLAIM_DRAW_RADIUS
            });
            if near {
                let colour = if c.owner == s.account { OWN_LINE } else { OTHER_LINE };
                push(&c.plots.iter().copied().collect(), colour, 0.01);
            }
        }
        if let Some(t) = s.claim_target {
            push(&HashSet::from([t]), TARGET_LINE, 0.02);
        }
        let xray = lines
            .iter()
            .map(|v| LineVertex {
                pos: v.pos,
                color: [v.color[0], v.color[1], v.color[2], v.color[3] * XRAY_ALPHA],
            })
            .collect();
        (lines, xray)
    }

    fn render(&mut self) {
        let mut ui = self.run_ui();
        self.render_frame(ui.as_ref());
        release_ui_textures(&mut ui);
    }

    fn render_frame(&mut self, ui: Option<&(Vec<egui::ClippedPrimitive>, egui::TexturesDelta, f32)>) {
        let ui_frame = ui.map(|(p, t, ppp)| EguiFrame {
            primitives: p,
            textures: t,
            pixels_per_point: *ppp,
        });
        let alpha = self.accumulator / TICK_DT;
        let local = self.local_ticks();
        let time = self.start.elapsed().as_secs_f32();
        let now = self.now();
        let (yaw, pitch) = (self.yaw, self.pitch);
        let Some(s) = &self.session else {
            // Nothing to draw yet: an empty sky behind the login screen.
            let Some(r) = &mut self.renderer else { return };
            let cam = Camera {
                eye: DVec3::ZERO,
                yaw,
                pitch: 0.25,
                fov_y: 75f32.to_radians(),
            };
            let day = DayCycle::default();
            r.render(&Scene {
                camera: &cam,
                sky: &Sky::at(&day, &PlanetLook::default(), day.day_ticks as f64 * 0.18),
                planets: &[],
                shape: PlanetShape::new(8, 1),
                boxes: &[],
                highlight: None,
                breaking: None,
                lines: &[],
                xray_lines: &[],
                ui: ui_frame,
                fog_end: 100.0,
                time,
            });
            return;
        };
        let shape = s.world.shape;
        let sky = Sky::at(&s.planet.day, &s.planet.look, s.sky_tick(local));
        let planets = sky.planets(&s.universe, s.planet.id);
        let eye = s.render_eye(alpha);
        // A blast nearby shakes the camera.
        let eye = shape.wrap_point(eye + s.fx.shake(eye, shape, now));
        let camera = Camera {
            eye,
            yaw,
            pitch,
            fov_y: FOV_Y,
        };
        let remotes = s.remotes.sample(local, shape);
        let mut boxes: Vec<BoxInstance> = remotes
            .iter()
            .map(|&(id, smp)| {
                let pos = shape.nearest_lap(eye, smp.pos);
                // Drawn two blocks tall, a touch above the collision box.
                let half = Vec3::new(
                    s.movement.half_width as f32,
                    PLAYER_VISUAL_HEIGHT / 2.0,
                    s.movement.half_width as f32,
                );
                let centre = (pos - eye).as_vec3() + Vec3::new(0.0, half.y, 0.0);
                BoxInstance::new(
                    centre,
                    half,
                    player_colour(id),
                    BoxInstance::light_of(s.world.light_at(pos + DVec3::new(0.0, 1.0, 0.0))),
                )
            })
            .collect();
        // Creatures, lit where they stand, coats leaning to the planet's colour.
        let light = |p: DVec3| BoxInstance::light_of(s.world.light_at(p));
        boxes.extend(s.creatures.boxes(&s.seen, eye, shape, s.planet.look.color, light));
        // Shots by their kind: pellets with a short trail, bolts, homing
        // orbs and bombs. Ours start at the eye, so they show once clear of it.
        for shot in s.shots.drawn(alpha) {
            let rel = shape.nearest_lap(eye, shot.pos) - eye;
            if shot.mine && rel.length() < 0.9 {
                continue;
            }
            crate::effects::shot_boxes(&mut boxes, rel.as_vec3(), &shot, time, light(shot.pos));
        }
        // Blasts and beams; a beam shines from its drifter's face.
        // Meteors: the fall, the beam over a site, the creature portals.
        let tick_now = s.server_tick(self.local_ticks()) as f64;
        boxes.extend(s.sky_view.boxes(eye, shape, tick_now, time));
        boxes.extend(s.fx.boxes(eye, shape, now, |id| {
            let c = s.seen.iter().find(|c| c.id == id)?;
            let k = s.creatures.kind(c.kind)?;
            Some(c.pos + DVec3::new(0.0, k.height as f64 * 0.45, 0.0))
        }));
        // The sling or the bag of bombs in hand, low and to the right.
        if let Some(def) = s.items.slot(s.selected).and_then(|h| s.items.def(&h))
            && !s.me.state.down
        {
            let turn = glam::Quat::from_rotation_y(-yaw as f32) * glam::Quat::from_rotation_x(pitch as f32);
            let hand = turn * Vec3::new(0.3, -0.3, -0.55);
            if def.sling().is_some() {
                boxes.extend(held_sling(&def.name, turn, hand, light(eye)));
            } else if def.bomb().is_some() {
                boxes.extend(crate::effects::held_bombs(
                    &def.name,
                    s.fuse.burned(s.me.seq),
                    time,
                    turn,
                    hand,
                    light(eye),
                ));
            }
        }
        let (mut lines, xray) = self.claim_lines(eye);
        // Grapple ropes: ours from just below and right of the eye (where a
        // hand would be), everyone else's from their hands.
        let mut ropes: Vec<(DVec3, DVec3)> = remotes
            .iter()
            .filter_map(|(_, smp)| {
                let hands = shape.nearest_lap(eye, smp.pos) + DVec3::new(0.0, bm_core::movement::ROPE_HEIGHT, 0.0);
                smp.hook
                    .map(|h| (hands, hands + h - DVec3::new(0.0, bm_core::movement::ROPE_HEIGHT, 0.0)))
            })
            .collect();
        if let Some(hook) = s.me.state.hook {
            let to = shape.nearest_lap(eye, hook.pos);
            // A flying hook is drawn between ticks, like the body.
            let to = match s.me.previous.hook {
                Some(was) if hook.stuck.is_none() => {
                    let was = shape.nearest_lap(to, was.pos);
                    was + (to - was) * alpha
                }
                _ => to,
            };
            let forward = camera.forward();
            let right = forward.cross(DVec3::Y).normalize_or_zero();
            ropes.push((eye + right * 0.3 - DVec3::Y * 0.35 + forward * 0.4, to));
        }
        // Lines are a pixel wide: a few strands side by side give the rope
        // some body, thick near the eye and thin far off.
        let strands = {
            let forward = camera.forward();
            let right = forward.cross(DVec3::Y).normalize_or_zero();
            let up = right.cross(forward);
            [DVec3::ZERO, right * 0.006, up * 0.006, (right + up) * 0.006]
        };
        for (from, to) in ropes {
            for off in strands {
                let (a, b) = ((from + off - eye).as_vec3(), (to + off - eye).as_vec3());
                lines.push(LineVertex {
                    pos: a.to_array(),
                    color: ROPE_LINE,
                });
                lines.push(LineVertex {
                    pos: b.to_array(),
                    color: ROPE_LINE,
                });
            }
            let b = (to - eye).as_vec3();
            boxes.push(BoxInstance::new(
                b,
                Vec3::splat(0.09),
                HOOK_COLOUR,
                BoxInstance::light_of(s.world.light_at(to)),
            ));
        }
        if let Some(r) = &mut self.renderer {
            let mut lights = s.held_lights(&remotes, &camera);
            lights.extend(s.fx.lights(now));
            lights.extend(s.sky_view.lights(s.world.shape, camera.eye));
            r.set_point_lights(&lights);
        }
        let highlight = if self.claim_mode || s.chisel.is_some() {
            None
        } else {
            s.target.map(|t| t.block)
        };
        if let Some((block, plan)) = s.chisel.filter(|_| !self.claim_mode) {
            let corner = DVec3::new(block.x as f64, block.y as f64, block.z as f64);
            let rel = (shape.nearest_lap(eye, corner) - eye).as_vec3();
            for (mask, color, grow) in [(plan.cut, CUT_LINE, 0.004f32), (plan.fill, FILL_LINE, 0.002)] {
                for (lo, hi) in mask.map(crate::chisel::outline).unwrap_or_default() {
                    let lo = Vec3::new(lo[0] as f32, lo[1] as f32, lo[2] as f32) - Vec3::splat(grow);
                    let hi = Vec3::new(hi[0] as f32, hi[1] as f32, hi[2] as f32) + Vec3::splat(grow);
                    let edges = crate::render::cube_edges(rel + lo, hi - lo);
                    lines.extend(edges.iter().map(|p| LineVertex { pos: *p, color }));
                }
            }
        }
        // Far views (milestone 20) push the fog out to the far radius.
        let view_radius = s.view_radius.max(s.far_radius);
        // Short of air, the fog closes in (milestone 18).
        let fog_end = bm_core::atmosphere::fog_end((view_radius as f32 - 1.0) * 32.0, &s.me.state);
        // Cracks on the aimed block once it has taken damage.
        let breaking = s
            .target_health()
            .filter(|&(_, left, _)| left < 1.0)
            .map(|(pos, left, _)| (pos, 1.0 - left));
        let Some(r) = &mut self.renderer else { return };
        r.render(&Scene {
            camera: &camera,
            sky: &sky,
            planets: &planets,
            ui: ui_frame,
            shape,
            boxes: &boxes,
            highlight,
            breaking,
            lines: &lines,
            xray_lines: &xray,
            fog_end,
            time,
        });
    }

    fn update_title(&mut self) {
        self.frames += 1;
        let since = self.last_title.elapsed().as_secs_f64();
        if since < 0.5 {
            return;
        }
        self.fps = self.frames as f64 / since;
        self.frames = 0;
        self.last_title = Instant::now();
        let Some(w) = &self.window else { return };
        let title = match &self.session {
            Some(s) => {
                let p = s.me.state.pos;
                let held = s
                    .items
                    .slot(s.selected)
                    .and_then(|h| s.items.def(&h))
                    .map(|d| d.display.clone())
                    .unwrap_or_else(|| "empty hand".into());
                let (chunks, meshes, quads, far) = self
                    .renderer
                    .as_ref()
                    .map(|r| {
                        (
                            r.stats.chunks_drawn,
                            r.chunk_meshes(),
                            r.stats.quads_drawn,
                            r.far_meshes(),
                        )
                    })
                    .unwrap_or_default();
                format!(
                    "Boundmore | {} | {:.0} fps | {:.0} ms | {:.0} {:.0} {:.0} | {} chunks, {} far ({} held), {} drawn, {}k quads, {} to mesh | {} corrections | [{}] {}",
                    s.planet.name,
                    self.fps,
                    self.link.as_ref().map(|n| n.rtt_ms()).unwrap_or(0.0),
                    p.x,
                    p.y,
                    p.z,
                    meshes,
                    far,
                    s.far.len(),
                    chunks,
                    quads / 1000,
                    s.world.dirty_count(),
                    s.me.corrections,
                    (s.selected + 1) % 10,
                    held,
                )
            }
            None => format!("Boundmore | {}", self.status),
        };
        let title = if self.missing_textures > 0 {
            format!("{title} | {} TEXTURES MISSING (see log)", self.missing_textures)
        } else {
            title
        };
        w.set_title(&title);
    }

    /// `--demo-ui`: open the screen asked for once the demo welcomes us.
    fn open_demo_screen(&mut self) {
        let Some(screen) = self.opts.demo.clone() else { return };
        let now = self.now();
        match screen.as_str() {
            "inventory" => self.open_menu(Menu::Inventory),
            // Milestone 24: someone's shop stand and request basket, as a
            // customer sees them, with coin to spend; and the feats.
            "shop" | "basket" | "feats" | "friends" | "guild" => {
                let Some(s) = &mut self.session else { return };
                let id = |n: &str| s.items.registry.id(n).ok();
                let mut ledger = bm_core::feats::Ledger {
                    coin: 1_340,
                    ..Default::default()
                };
                let book = &*FEAT_BOOK;
                ledger.add(book, "placed", 3_100);
                ledger.add(book, "mine:rock", 640);
                ledger.add(book, "kill:grazer", 23);
                s.ledger = Some(ledger);
                if let (Some(stone), Some(copper), Some(torch)) = (id("stone"), id("copper_ingot"), id("torch")) {
                    s.items.inventory.slots[3] = Some(bm_core::ItemStack::new(copper, 12));
                    let pos = BlockPos::new(130, 44, 130);
                    let basket = screen == "basket";
                    self.shop_view = Some(bm_protocol::ShopView {
                        pos,
                        kind: if basket {
                            bm_protocol::ShopKind::Basket
                        } else {
                            bm_protocol::ShopKind::Stand
                        },
                        owner: "Alice".into(),
                        mine: false,
                        slots: vec![
                            Some(bm_core::ItemStack::new(
                                if basket { copper } else { stone },
                                if basket { 1 } else { 64 },
                            )),
                            Some(bm_core::ItemStack::new(torch, 20)),
                        ],
                        prices: vec![if basket { 30 } else { 4 }, 25],
                        till: if basket { 900 } else { 0 },
                        fee_permille: bm_core::trade::SALES_FEE_PERMILLE,
                        closed: false,
                    });
                    if screen == "feats" {
                        self.shop_view = None;
                        self.open_menu(Menu::Feats);
                    } else if screen == "friends" {
                        self.shop_view = None;
                        self.open_menu(Menu::Friends);
                    } else if screen == "guild" {
                        self.shop_view = None;
                        self.open_menu(Menu::Guild(None));
                    } else {
                        self.menu = Some(Menu::Shop(pos));
                        self.set_grab(false);
                    }
                }
            }
            // A level 24 character with two pages and marks, some points
            // spent; the demo answers changes as the server would.
            "skills" => {
                if let Some(Link::Demo(d)) = &mut self.link {
                    d.show_skills();
                }
                self.pitch = -0.12;
                self.open_menu(Menu::Skills);
            }
            "crafting" => self.open_menu(Menu::Crafting(None)),
            "station" => {
                // Face the demo workbench (four blocks in a T), two blocks
                // ahead and below the eye, opened by its middle block.
                self.pitch = -0.5;
                self.open_menu(Menu::Crafting(Some(BlockPos::new(128, 44, 126))));
            }
            // The crafting table beside the spawn, the furnace (opened by
            // its crucible) burning coal, or the extractor a block short.
            "table" | "furnace" | "extractor" => {
                let (at, yaw, pitch) = match screen.as_str() {
                    "table" => (crate::demo::TABLE, std::f64::consts::FRAC_PI_2, -0.7),
                    "furnace" => (crate::demo::FURNACE[1], 0.98, -0.2),
                    _ => (crate::demo::EXTRACTOR[1], 0.98, -0.15),
                };
                (self.yaw, self.pitch) = (yaw, pitch);
                self.open_menu(Menu::Crafting(Some(BlockPos::new(at[0], at[1], at[2]))));
            }
            // A meteor mid-fight, seen from the south.
            "meteor" => {
                if let Some(Link::Demo(d)) = &mut self.link {
                    d.show_meteor();
                }
                if let Some(s) = &mut self.session {
                    s.me.teleport(bm_core::PlayerState {
                        pos: DVec3::from_array(crate::demo::METEOR_VIEW),
                        vel: DVec3::ZERO,
                        on_ground: true,
                        hook: None,
                        ..Default::default()
                    });
                }
                (self.yaw, self.pitch) = (0.0, 0.12);
            }
            // Milestone 17: a forge with a session under way on an iron
            // hammer, its window open; or ("forge-queue") a sealed hammer in
            // its queue and a forged one ready to collect.
            // "forge-empty": an idle forge, to choose gear and a deck.
            "forge" | "forge-queue" | "forge-empty" => {
                if let Some(Link::Demo(d)) = &mut self.link {
                    d.show_forge(match screen.as_str() {
                        "forge-queue" => crate::demo::ForgeShow::Queue,
                        "forge-empty" => crate::demo::ForgeShow::Empty,
                        _ => crate::demo::ForgeShow::Session,
                    });
                }
                if let Some(s) = &mut self.session {
                    s.me.teleport(bm_core::PlayerState {
                        pos: DVec3::new(128.5, 44.0, 126.5),
                        vel: DVec3::ZERO,
                        on_ground: true,
                        hook: None,
                        ..Default::default()
                    });
                    s.selected = 1;
                }
                (self.yaw, self.pitch) = (0.0, -0.35);
                let f = crate::demo::FORGE[1];
                self.open_menu(Menu::Crafting(Some(BlockPos::new(f[0], f[1], f[2]))));
            }
            // Spark: two generators, links and a running mixer, its window
            // open with its wear and spark; the other machines behind.
            "spark" => {
                if let Some(Link::Demo(d)) = &mut self.link {
                    d.show_spark();
                }
                if let Some(s) = &mut self.session {
                    s.me.teleport(bm_core::PlayerState {
                        pos: DVec3::from_array(crate::demo::SPARK_VIEW),
                        vel: DVec3::ZERO,
                        on_ground: true,
                        hook: None,
                        ..Default::default()
                    });
                    s.selected = 9;
                }
                (self.yaw, self.pitch) = (0.0, -0.3);
                let m = crate::demo::MIXER[1];
                self.open_menu(Menu::Crafting(Some(BlockPos::new(m[0], m[1], m[2]))));
            }
            // The texture trial's six textures in one view.
            "textures" => {
                if let Some(Link::Demo(d)) = &mut self.link {
                    d.show_textures();
                }
                if let Some(s) = &mut self.session {
                    s.me.teleport(bm_core::PlayerState {
                        pos: DVec3::from_array(crate::demo::TEXTURES_VIEW),
                        vel: DVec3::ZERO,
                        on_ground: true,
                        hook: None,
                        ..Default::default()
                    });
                }
                (self.yaw, self.pitch) = (0.0, -0.18);
            }
            // Every gathering object in daylight: plants, boulders, and
            // fungi in a cave mouth, seen from a post.
            "flora" => {
                if let Some(Link::Demo(d)) = &mut self.link {
                    d.show_flora();
                }
                if let Some(s) = &mut self.session {
                    s.me.teleport(bm_core::PlayerState {
                        pos: DVec3::from_array(crate::demo::FLORA_VIEW),
                        vel: DVec3::ZERO,
                        on_ground: true,
                        hook: None,
                        ..Default::default()
                    });
                }
                (self.yaw, self.pitch) = (0.0, -0.45);
            }
            // One of each creature on level ground, a stone sling in hand;
            // or just defeated by a Hardy Grazer, the defeat screen up; or
            // a bag of bombs in hand, lit and thrown on a cycle, with blasts
            // going off among the creatures.
            "creatures" | "defeat" | "bombs" => {
                let down = screen == "defeat";
                let bombs = screen == "bombs";
                if let Some(Link::Demo(d)) = &mut self.link {
                    if down {
                        d.show_defeat();
                    } else if bombs {
                        d.show_bombs();
                    } else {
                        d.show_creatures();
                    }
                }
                self.demo_bombs = bombs;
                if let Some(s) = &mut self.session {
                    let mut at = bm_core::PlayerState {
                        pos: DVec3::from_array(crate::demo::FIGHT_VIEW),
                        on_ground: true,
                        loose: u16::MAX,
                        ..Default::default()
                    };
                    if down {
                        at.take(at.max_health());
                    }
                    s.me.teleport(at);
                    s.losses.reset(at.hurt);
                    s.last_hurt = at.hurt;
                    s.selected = if bombs { 7 } else { 8 };
                }
                (self.yaw, self.pitch) = (0.0, -0.16);
            }
            "claims" => {
                self.yaw = 0.7;
                self.pitch = -0.32;
                self.claim_mode = true;
            }
            // Facing the shrine by the spawn: its hint, its menu, or the
            // journey after choosing the second planet (held for good).
            "shrine" | "travel" | "travelling" => {
                self.yaw = std::f64::consts::PI;
                self.pitch = -0.38;
                let shrine = BlockPos::new(crate::demo::SHRINE[0], crate::demo::SHRINE[1], crate::demo::SHRINE[2]);
                if screen == "travel" {
                    self.open_menu(Menu::Travel(shrine));
                } else if screen == "travelling" {
                    if let Some(Link::Demo(d)) = &mut self.link {
                        d.hold_arrivals(f64::INFINITY);
                    }
                    self.send(ClientMsg::Warp { shrine, planet: 1 });
                }
            }
            // Facing the demo chest, open beside the inventory.
            "storage" => {
                self.yaw = std::f64::consts::PI * 1.75;
                self.pitch = -0.45;
                self.open_menu(Menu::Storage(BlockPos::new(
                    crate::demo::CHEST[0],
                    crate::demo::CHEST[1],
                    crate::demo::CHEST[2],
                )));
            }
            // The palette wall in daylight, a pack of coloured stacks.
            "colours" => {
                if let Some(Link::Demo(d)) = &mut self.link {
                    d.show_colours();
                }
                self.pitch = 0.42;
            }
            "coloured-icons" => {
                if let Some(Link::Demo(d)) = &mut self.link {
                    d.show_colours();
                }
                self.open_menu(Menu::Inventory);
            }
            // Milestone 22 part 4: written signs on a wall.
            "signs" => {
                if let Some(Link::Demo(d)) = &mut self.link {
                    d.show_signs();
                }
                if let Some(s) = &mut self.session {
                    s.me.teleport(bm_core::PlayerState {
                        pos: DVec3::from_array(crate::demo::SHAPES_VIEW),
                        vel: DVec3::ZERO,
                        on_ground: true,
                        hook: None,
                        ..Default::default()
                    });
                }
                (self.yaw, self.pitch) = (0.0, -0.2);
            }
            // Milestone 22: doors, trapdoors and furnishings in daylight.
            "furnishings" => {
                if let Some(Link::Demo(d)) = &mut self.link {
                    d.show_furnishings();
                }
                if let Some(s) = &mut self.session {
                    s.me.teleport(bm_core::PlayerState {
                        pos: DVec3::from_array(crate::demo::FURNISHINGS_VIEW),
                        vel: DVec3::ZERO,
                        on_ground: true,
                        hook: None,
                        ..Default::default()
                    });
                }
                (self.yaw, self.pitch) = (0.0, -0.5);
            }
            // Chisel cuts of every family in daylight; with "chisel", a
            // bevel chisel in hand aimed at a brick, its cut outlined.
            "shapes" | "chisel" => {
                let in_hand = screen == "chisel";
                if let Some(Link::Demo(d)) = &mut self.link {
                    d.show_shapes(in_hand);
                }
                if let Some(s) = &mut self.session {
                    s.me.teleport(bm_core::PlayerState {
                        pos: DVec3::from_array(crate::demo::SHAPES_VIEW),
                        vel: DVec3::ZERO,
                        on_ground: true,
                        hook: None,
                        ..Default::default()
                    });
                    if in_hand {
                        s.selected = 6;
                    }
                }
                (self.yaw, self.pitch) = if in_hand { (0.075, -0.55) } else { (0.0, -0.62) };
            }
            // A dark stone hall lit by a standing torch and two wall
            // torches at its far end, a torch in hand (slot 0 holds none).
            // Milestone 16: hungry, Sated, a hotbar of food; the HUD shows
            // the shaded energy bar and the food tags.
            // Milestone 16 part 2: a field of every crop at every stage.
            "farm" => {
                if let Some(Link::Demo(d)) = &mut self.link {
                    d.show_farm();
                }
                if let Some(s) = &mut self.session {
                    s.me.teleport(bm_core::PlayerState {
                        pos: DVec3::from_array(crate::demo::FARM_VIEW),
                        vel: DVec3::ZERO,
                        on_ground: true,
                        hook: None,
                        ..Default::default()
                    });
                    s.selected = 0;
                }
                (self.yaw, self.pitch) = (0.0, -0.42);
            }
            "food" => {
                if let Some(Link::Demo(d)) = &mut self.link {
                    d.show_food();
                }
                if let Some(s) = &mut self.session {
                    let mut st = s.me.state;
                    st.hunger = 220 * bm_core::food::HUNGER_SCALE;
                    st.spent = 120;
                    st.buff = bm_core::food::FoodBuff {
                        kind: bm_core::food::Buff::Sated,
                        rank: 1,
                        left: (9 * 60 + 41) * bm_core::TICK_HZ,
                    };
                    s.me.teleport(st);
                    s.selected = 0;
                }
                (self.yaw, self.pitch) = (0.4, -0.1);
            }
            // Milestone 18: on a tier 6 world of burning air with Fire Ward 1,
            // most of the air gone and the fog closing in.
            "air" => {
                if let Some(s) = &mut self.session {
                    let mut st = s.me.state;
                    st.air = bm_core::atmosphere::Air::of(bm_core::atmosphere::AirLine::Burning, 6);
                    st.body.wards = [0, 0, bm_core::atmosphere::ward_points(1)];
                    st.air_used = bm_core::atmosphere::AIR_TICKS * 3 / 4;
                    s.me.teleport(st);
                }
                (self.yaw, self.pitch) = (0.4, -0.05);
            }
            "atlas" => {
                if let Some(Link::Demo(d)) = &mut self.link {
                    d.show_atlas();
                }
                if let Some(s) = &mut self.session {
                    s.selected = 9;
                }
                self.atlas_request = Some(9);
            }
            "lights" => {
                if let Some(Link::Demo(d)) = &mut self.link {
                    d.show_lights();
                }
                if let Some(s) = &mut self.session {
                    s.selected = 9;
                }
                (self.yaw, self.pitch) = (0.0, -0.18);
            }
            "torches" => {
                if let Some(Link::Demo(d)) = &mut self.link {
                    d.show_torches();
                }
                if let Some(s) = &mut self.session {
                    s.selected = 9;
                }
                (self.yaw, self.pitch) = (0.12, -0.12);
            }
            // Hanging from a copper grapple hooked under a stone arch, part
            // way through a swing towards it.
            "grapple" => {
                if let Some(Link::Demo(d)) = &mut self.link {
                    d.show_grapple();
                }
                if let Some(s) = &mut self.session {
                    let [x, y, z] = crate::demo::GRAPPLE_BEAM;
                    let block = BlockPos::new(x + 5, y, z);
                    let anchor = DVec3::new(x as f64 + 5.5, y as f64, z as f64 + 0.5);
                    let pos = anchor + DVec3::new(0.0, -8.0 - bm_core::movement::ROPE_HEIGHT, 6.0);
                    // Taken up once the arch has arrived: a hook in a block
                    // that is not there yet lets go.
                    self.demo_pose = Some(bm_core::PlayerState {
                        pos,
                        vel: DVec3::ZERO,
                        on_ground: false,
                        hook: Some(bm_core::Hook {
                            pos: anchor,
                            vel: DVec3::ZERO,
                            rope: 10.0,
                            stuck: Some(block),
                            reel: 0.0,
                            hand: 0,
                        }),
                        ..Default::default()
                    });
                    s.selected = 7;
                }
                (self.yaw, self.pitch) = (0.1, 0.62);
            }
            // Two open portals ahead, the left one's window open, a token
            // and wayshards in hand.
            "portal" => {
                if let Some(Link::Demo(d)) = &mut self.link {
                    d.show_portals();
                }
                if let Some(s) = &mut self.session {
                    s.selected = 8;
                    // Back from the stations, so both portals show whole.
                    s.me.teleport(bm_core::PlayerState {
                        pos: DVec3::new(128.5, 44.0, 130.3),
                        vel: DVec3::ZERO,
                        on_ground: true,
                        hook: None,
                        ..Default::default()
                    });
                }
                (self.yaw, self.pitch) = (0.0, -0.02);
                let [x, y, z] = crate::demo::PORTALS[0];
                self.open_menu(Menu::Portal(BlockPos::new(x, y, z)));
            }
            // On a tier 3 world, a stone hammer in hand, looking at a half
            // broken stone block: its health bar and one shield.
            "mining" => {
                if let Some(Link::Demo(d)) = &mut self.link {
                    d.show_mining();
                }
                if let Some(s) = &mut self.session {
                    s.selected = 0;
                }
                (self.yaw, self.pitch) = (0.0, -0.5);
            }
            // The shrine window on its Reclaim tab, with things waiting.
            "reclaim" => {
                if let Some(Link::Demo(d)) = &mut self.link {
                    d.sample_reclaim();
                }
                self.yaw = std::f64::consts::PI;
                self.pitch = -0.38;
                let shrine = BlockPos::new(crate::demo::SHRINE[0], crate::demo::SHRINE[1], crate::demo::SHRINE[2]);
                self.open_menu(Menu::Travel(shrine));
                self.shrine_tab = ShrineTab::Reclaim;
            }
            // Lines on every channel, the box open for typing.
            "chat" => {
                self.pitch = -0.12;
                if let Some(Link::Demo(d)) = &mut self.link {
                    d.sample_chat();
                }
                self.open_chat(false);
                self.chat.input = "/w Wren on my way".into();
            }
            // Looking up at the other planets (pair with BOUNDMORE_TIME).
            "sky" => {
                let local = self.local_ticks();
                if let Some(s) = &self.session {
                    let sky = Sky::at(&s.planet.day, &s.planet.look, s.sky_tick(local));
                    // The fullest planets above the horizon (furthest from the
                    // sun): between the two best if both fit in view.
                    let sun = Vec3::from(sky.sun_dir);
                    let mut up: Vec<Vec3> = sky
                        .planets(&s.universe, s.planet.id)
                        .iter()
                        .map(|p| Vec3::from(p.dir))
                        .filter(|d| d.y > 0.12)
                        .collect();
                    up.sort_by(|a, b| a.dot(sun).total_cmp(&b.dot(sun)));
                    let aim = match up.as_slice() {
                        [a, b, ..] if a.angle_between(*b) < 1.1 => (*a + *b).normalize(),
                        [a, ..] => *a,
                        [] => Vec3::new(0.0, 0.5, -1.0).normalize(),
                    };
                    let limit = bm_core::movement::pitch_radians(bm_core::movement::PITCH_LIMIT);
                    self.yaw = (aim.x as f64).atan2(-aim.z as f64).rem_euclid(std::f64::consts::TAU);
                    self.pitch = (aim.y as f64).asin().clamp(-limit, limit);
                }
            }
            _ => {
                self.pitch = -0.12;
                self.notices.push("Your inventory is full".into(), now);
                if let Some(s) = &mut self.session {
                    s.selected = 2;
                }
            }
        }
    }
}

/// A sling held in hand: a forked frame, its band and pouch, in the
/// material the sling is made of; `turn` is the camera's, `hand` where it
/// is held (camera-relative), `light` the light there.
/// After a frame: the renderer has applied (or, with no renderer, skipped)
/// egui's texture changes. egui asserts in debug builds that none is dropped
/// unhandled, which crashed every debug build on its first UI frame.
fn release_ui_textures(ui: &mut Option<(Vec<egui::ClippedPrimitive>, egui::TexturesDelta, f32)>) {
    if let Some((_, textures, _)) = ui {
        textures.clear();
    }
}

fn held_sling(name: &str, turn: glam::Quat, hand: Vec3, light: [f32; 4]) -> Vec<BoxInstance> {
    use glam::Quat;
    let srgb = |c: [u8; 3]| {
        let l = crate::colour::srgb_to_linear(c);
        [l[0], l[1], l[2], 1.0]
    };
    let frame = if name.contains("copper") {
        srgb([214, 122, 70])
    } else if name.contains("iron") {
        srgb([150, 156, 166])
    } else {
        srgb([140, 94, 54])
    };
    let leather = srgb([92, 58, 34]);
    let band = srgb([60, 44, 36]);
    let k = HELD_SLING_SCALE;
    let turn = turn * Quat::from_rotation_x(0.2) * Quat::from_rotation_z(-0.25);
    let part = |at: Vec3, half: Vec3, rot: Quat, colour: [f32; 4]| BoxInstance {
        center: (hand + turn * (at * k)).to_array(),
        half_size: (half * k).to_array(),
        color: colour,
        rotation: (turn * rot).normalize().to_array(),
        light,
    };
    let mut out = vec![
        part(
            Vec3::new(0.0, -0.1, 0.0),
            Vec3::new(0.025, 0.1, 0.025),
            Quat::IDENTITY,
            frame,
        ),
        part(
            Vec3::new(0.0, -0.13, 0.0),
            Vec3::new(0.032, 0.045, 0.032),
            Quat::IDENTITY,
            leather,
        ),
    ];
    for s in [-1.0f32, 1.0] {
        out.push(part(
            Vec3::new(s * 0.04, 0.06, 0.0),
            Vec3::new(0.022, 0.075, 0.022),
            Quat::from_rotation_z(-s * 0.5),
            frame,
        ));
        out.push(part(
            Vec3::new(s * 0.075, 0.13, 0.0),
            Vec3::new(0.026, 0.02, 0.026),
            Quat::IDENTITY,
            frame,
        ));
        // A strand from the prong back to the pouch.
        out.push(part(
            Vec3::new(s * 0.04, 0.125, 0.06),
            Vec3::new(0.007, 0.007, 0.07),
            Quat::from_rotation_y(s * 0.55),
            band,
        ));
    }
    out.push(part(
        Vec3::new(0.0, 0.12, 0.12),
        Vec3::new(0.035, 0.028, 0.022),
        Quat::IDENTITY,
        leather,
    ));
    out
}

fn player_colour(id: u32) -> [f32; 4] {
    let h = id.wrapping_mul(2_654_435_761);
    let c = |shift: u32| 0.25 + 0.6 * ((h >> shift) & 0xff) as f32 / 255.0;
    [c(0), c(8), c(16), 1.0]
}

fn digit_slot(code: KeyCode) -> Option<usize> {
    Some(match code {
        KeyCode::Digit1 => 0,
        KeyCode::Digit2 => 1,
        KeyCode::Digit3 => 2,
        KeyCode::Digit4 => 3,
        KeyCode::Digit5 => 4,
        KeyCode::Digit6 => 5,
        KeyCode::Digit7 => 6,
        KeyCode::Digit8 => 7,
        KeyCode::Digit9 => 8,
        KeyCode::Digit0 => 9,
        _ => return None,
    })
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("Boundmore")
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 720.0));
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                tracing::error!("cannot open a window: {e}");
                event_loop.exit();
                return;
            }
        };
        match pollster::block_on(Renderer::new(window.clone(), self.opts.vsync)) {
            Ok(r) => self.renderer = Some(r),
            Err(e) => {
                tracing::error!("cannot start the renderer: {e:#}");
                event_loop.exit();
                return;
            }
        }
        let ctx = egui::Context::default();
        menus::install_style(&ctx);
        let winit = egui_winit::State::new(
            ctx.clone(),
            egui::ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            None,
            None,
        );
        self.egui = Some(EguiState { ctx, winit });
        self.window = Some(window);
        match self.opts.demo.as_deref() {
            Some("login") => {
                self.login.error = Some("No account by that name. Check it, or create one.".into());
            }
            Some(_) => {
                let name = self.opts.name.clone().unwrap_or_else(|| "Pendragon".into());
                self.link = Some(Link::Demo(Box::new(crate::demo::Demo::new(&name))));
            }
            None => {
                // A password on the command line skips the login screen
                // (headless testing); a refusal still lands on it.
                if let Some(p) = self.opts.password.take() {
                    self.login.password = p;
                    self.start_login(self.opts.register);
                }
            }
        }
        event_loop.set_control_flow(ControlFlow::Poll);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        // egui sees everything first; the game takes what egui does not need.
        let mut egui_typing = false;
        if let (Some(e), Some(w)) = (&mut self.egui, &self.window) {
            let _ = e.winit.on_window_event(w, &event);
            egui_typing = e.ctx.egui_wants_keyboard_input();
        }
        let in_game = self.session.is_some();
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(r) = &mut self.renderer {
                    r.resize(size.width, size.height);
                }
            }
            WindowEvent::Focused(false) => {
                self.keys.clear();
                self.mouse = [false; 2];
                self.drag = None;
                self.set_grab(false);
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let PhysicalKey::Code(code) = event.physical_key else {
                    return;
                };
                if event.state != ElementState::Pressed {
                    self.keys.remove(&code);
                    return;
                }
                if code == KeyCode::F11 {
                    if let Some(w) = &self.window {
                        let full = w.fullscreen().is_some();
                        w.set_fullscreen((!full).then_some(Fullscreen::Borderless(None)));
                    }
                    return;
                }
                // Everything typed into the chat line is the line's: no game
                // key, menu key or hotbar digit acts while it is open.
                if !in_game || self.chat.open || egui_typing || event.repeat {
                    return;
                }
                if code == KeyCode::F8 {
                    self.toggle_creative();
                    return;
                }
                // Down: the defeat screen is all there is (Enter returns; T
                // still opens the chat).
                let down = self.session.as_ref().is_some_and(|s| s.defeat.is_down());
                if down && code != KeyCode::KeyT {
                    return;
                }
                match code {
                    KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::KeyT => {
                        self.open_chat(true);
                        return;
                    }
                    KeyCode::Escape => {
                        if self.menu.is_some() {
                            self.close_menu(true);
                        } else {
                            self.set_grab(false);
                        }
                    }
                    KeyCode::KeyI | KeyCode::Tab => self.toggle(Menu::Inventory),
                    KeyCode::KeyK => self.toggle(Menu::Skills),
                    // Feats and coin (milestone 24).
                    KeyCode::KeyJ => self.toggle(Menu::Feats),
                    // Friends and rights (milestone 25).
                    KeyCode::KeyO => self.toggle(Menu::Friends),
                    // Our guilds (milestone 25 part 2).
                    KeyCode::KeyG => self.toggle(Menu::Guild(None)),
                    KeyCode::KeyC => {
                        let m = self.crafting_menu();
                        self.toggle(m);
                    }
                    KeyCode::KeyE => {
                        if matches!(self.menu, Some(Menu::Travel(_) | Menu::Storage(_) | Menu::Portal(_))) {
                            self.close_menu(true);
                        } else if self.menu.is_none()
                            && !self.claim_mode
                            && let Some(s) = &self.session
                            && !s.travelling()
                        {
                            if let Some(shrine) = s.aiming_at_shrine() {
                                self.open_menu(Menu::Travel(shrine));
                            } else if let Some(pos) = s.aiming_at_storage() {
                                self.open_storage(pos);
                            } else if let Some(pos) = s.aiming_at_portal() {
                                self.open_portal(pos);
                            } else if let Some(pos) = s.aiming_at_heart() {
                                self.send(ClientMsg::LootHeart { pos });
                            } else if let Some(pos) = s.aiming_at_helix() {
                                let pos = s.world.shape.wrap_block(pos);
                                self.open_menu(Menu::Helix(pos));
                                self.send(ClientMsg::OpenHelix { pos: Some(pos) });
                            } else if let Some(pos) = s.aiming_at_guild_control() {
                                let pos = s.world.shape.wrap_block(pos);
                                self.open_menu(Menu::Guild(Some(pos)));
                                self.send(ClientMsg::OpenGuildControl { pos: Some(pos) });
                            } else if let Some(pos) = s.aiming_at_sign() {
                                self.open_sign(pos);
                            } else if let Some(pos) = s.aiming_at_door() {
                                self.send(ClientMsg::SwingDoor { pos });
                            }
                        }
                    }
                    KeyCode::KeyP if self.menu.is_none() => {
                        self.claim_mode = !self.claim_mode;
                        self.mouse = [false; 2];
                    }
                    _ => {}
                }
                // Alt and a number: the right hand's slot (milestone 28).
                let alt = self.keys.contains(&KeyCode::AltLeft) || self.keys.contains(&KeyCode::AltRight);
                if let (Some(d), Some(s)) = (digit_slot(code), &mut self.session) {
                    if alt {
                        s.selected_off = d;
                    } else {
                        s.selected = d;
                    }
                }
                if self.menu.is_none() {
                    self.keys.insert(code);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if !in_game || self.menu.is_some() || self.chat.open {
                    return; // the menus (or the chat box) have it
                }
                if self.session.as_ref().is_some_and(|s| s.defeat.is_down()) {
                    return; // the defeat screen has it
                }
                let down = state == ElementState::Pressed;
                if down && !self.grabbed {
                    self.set_grab(true);
                    return; // the click that grabs the mouse does not dig
                }
                if self.claim_mode {
                    if down {
                        match button {
                            MouseButton::Left => self.claim_click(true),
                            MouseButton::Right => self.claim_click(false),
                            _ => {}
                        }
                    }
                    return;
                }
                match button {
                    MouseButton::Left => {
                        // Every press counts, however short: a grapple shot
                        // must not be lost between two frames.
                        self.fire_pending |= down && !self.mouse[0];
                        self.attack_pending |= down && !self.mouse[0];
                        self.mouse[0] = down;
                    }
                    MouseButton::Right => {
                        self.fire_pending_off |= down && !self.mouse[1];
                        self.attack_pending_off |= down && !self.mouse[1];
                        self.mouse[1] = down;
                    }
                    _ => {}
                }
                if !down && let Some(s) = &mut self.session {
                    // Releasing lets the next click act at once.
                    s.place_cooldown = 0.0;
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if self.menu.is_some() || self.chat.open {
                    return;
                }
                let steps = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y.signum() as i32,
                    MouseScrollDelta::PixelDelta(p) => p.y.signum() as i32,
                };
                let alt = self.keys.contains(&KeyCode::AltLeft) || self.keys.contains(&KeyCode::AltRight);
                if let Some(s) = &mut self.session {
                    let hand = if alt { &mut s.selected_off } else { &mut s.selected };
                    *hand = (*hand as i32 - steps).rem_euclid(HOTBAR_SLOTS as i32) as usize;
                }
            }
            WindowEvent::RedrawRequested => {
                self.render();
                self.update_title();
            }
            _ => {}
        }
    }

    fn device_event(&mut self, _el: &ActiveEventLoop, _id: DeviceId, event: DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta: (dx, dy) } = event
            && self.grabbed
            && self.menu.is_none()
            && !self.chat.open
        {
            self.yaw = (self.yaw + dx * MOUSE_SENSITIVITY).rem_euclid(std::f64::consts::TAU);
            let limit = bm_core::movement::pitch_radians(bm_core::movement::PITCH_LIMIT);
            self.pitch = (self.pitch - dy * MOUSE_SENSITIVITY).clamp(-limit, limit);
        }
    }

    fn about_to_wait(&mut self, _el: &ActiveEventLoop) {
        self.update();
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn crop_lines_count_down_and_say_what_helps() {
        let v = CropView {
            pos: BlockPos::new(0, 0, 0),
            name: "Oats".into(),
            stage: 2,
            stages: 4,
            ripe_in: 3 * 3600 + 20 * 60,
            wild: false,
            tilled: true,
            dry: false,
        };
        assert_eq!(v.line(0.0), "Oats: ripe in 3 h 20 min, tilled");
        assert_eq!(v.line(3.0 * 3600.0), "Oats: ripe in 20 min, tilled");
        assert_eq!(v.line(4.0 * 3600.0), "Oats: ripe, harvest it, tilled");
        let wild = CropView {
            wild: true,
            tilled: false,
            stage: 4,
            ..v.clone()
        };
        assert_eq!(wild.line(0.0), "Wild Oats: ripe, harvest it");
        let dry = CropView {
            name: "Rice".into(),
            tilled: false,
            dry: true,
            ripe_in: 90,
            ..v
        };
        assert_eq!(dry.line(0.0), "Rice: ripe in 2 min, needs water beside it to grow well");
    }

    use super::*;
    use crate::demo::Demo;
    use crate::net::NetEvent;
    use bm_protocol::RemotePlayer;

    #[test]
    fn a_frames_ui_textures_are_released() {
        // egui's first frame always uploads its font atlas. Dropping that
        // change unapplied panics in debug builds (`TexturesDelta`'s check),
        // as every debug build of the client did before 2026-10-21.
        let ctx = egui::Context::default();
        let out = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.label("hello");
        });
        assert!(!out.textures_delta.set.is_empty(), "the first frame uploads textures");
        let mut ui = Some((Vec::new(), out.textures_delta, 1.0));
        release_ui_textures(&mut ui);
        let (_, textures, _) = ui.unwrap();
        assert!(textures.set.is_empty() && textures.free.is_empty());
    }

    fn session_on_the_demo() -> (Session, Demo, Vec<ServerMsg>) {
        let mut demo = Demo::new("Tester");
        demo.hold_arrivals(0.0);
        let mut events = demo.poll().into_iter();
        let Some(NetEvent::Connected(w)) = events.next() else {
            panic!("welcome first")
        };
        let registry = BlockRegistry::from_defs(w.blocks.clone()).unwrap();
        let items = Items::new(
            w.items.clone(),
            w.recipes.clone(),
            w.inventory.clone(),
            w.colours.clone(),
            &registry,
        )
        .unwrap();
        let set = TextureSet {
            layers: vec![],
            faces: vec![],
            crack_base: None,
            missing: vec![],
        };
        let s = Session::new(*w, registry, items, set, ItemIcons::default(), 0.0).unwrap();
        let rest = events
            .filter_map(|e| match e {
                NetEvent::Msg(m) => Some(m),
                _ => None,
            })
            .collect();
        (s, demo, rest)
    }

    fn snapshot(planet: u16, tick: u64, ack: u32, x: f64) -> ServerDatagram {
        ServerDatagram::Snapshot {
            planet,
            tick,
            ack,
            you: PlayerState {
                pos: DVec3::new(x, 44.0, 128.5),
                ..Default::default()
            },
            others: vec![RemotePlayer {
                id: 9,
                pos: [120.0, 44.0, 120.0],
                yaw: 0,
                pitch: 0,
                hook: None,
                light: [0; 3],
            }],
            creatures: Vec::new(),
        }
    }

    use bm_core::PlayerState;

    #[test]
    fn nothing_of_the_old_planet_survives_travel() {
        let (mut s, mut demo, msgs) = session_on_the_demo();
        for m in msgs {
            if let ServerMsg::Chunk { pos, data } = m {
                s.world.insert_chunk(pos, Chunk::decode(&data).unwrap());
            }
        }
        // Life on the first planet: chunks, a remote player, claims, an edit
        // in flight, a block half broken, meshes being made.
        assert!(s.world.chunk_count() > 0);
        assert!(s.snapshot(snapshot(0, 10, 0, 128.5), 10.0, true));
        s.remotes.joined(9, "Wren".into());
        assert!(!s.remotes.sample(30.0, s.world.shape).is_empty());
        assert!(s.claims.iter().count() > 0);
        s.world.predict_edit(1, BlockPos::new(128, 43, 128), BlockState::AIR);
        assert_eq!(s.world.pending_edits(), 1);
        s.block_health(BlockPos::new(129, 43, 128), 600);
        assert_eq!(s.damage.len(), 1);
        let centre = BlockPos::new(128, 44, 128).chunk();
        s.world.schedule_meshing(centre, 64);
        for _ in 0..12 {
            s.me.tick(
                PlayerInput::default(),
                [None, None],
                Default::default(),
                &s.movement,
                s.world.shape,
                &s.world,
            );
        }
        let seq = s.me.seq;

        let shrine = BlockPos::new(crate::demo::SHRINE[0], crate::demo::SHRINE[1], crate::demo::SHRINE[2]);
        demo.send(ClientMsg::Warp { shrine, planet: 2 });
        let mut after = demo.poll().into_iter();
        let Some(NetEvent::Msg(ServerMsg::Travel(t))) = after.next() else {
            panic!("travel")
        };
        let spawn = t.spawn;
        s.arrive(*t, 1.0, 60.0);

        assert_eq!(s.planet.id, 2);
        assert_eq!(s.planet.kind, "Frozen");
        assert_eq!(s.world.chunk_count(), 0, "chunks");
        assert_eq!(s.world.pending_edits(), 0, "predicted edits");
        assert_eq!(s.world.dirty_count(), 0, "meshing queue");
        assert!(
            s.remotes.names.is_empty() && s.remotes.sample(90.0, s.world.shape).is_empty(),
            "players"
        );
        assert_eq!(s.claims.iter().count(), 0, "claims");
        assert!(s.damage.is_empty() && s.target.is_none());
        assert_eq!(s.me.state, spawn);
        assert_eq!(s.me.seq, seq, "input sequences carry on");
        assert!(s.travelling());
        // Meshes started on the old planet never come out of the new world.
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert!(s.world.take_meshes().is_empty());
        // A late snapshot of the old planet is dropped; the new one's is used.
        assert!(!s.snapshot(snapshot(0, 20, seq, 1.5), 61.0, true));
        assert_eq!(s.me.state, spawn);
        assert!(s.remotes.sample(90.0, s.world.shape).is_empty());
        assert!(s.snapshot(snapshot(2, 21, seq, 128.5), 62.0, true));
        assert_eq!(s.me.corrections, 0);

        // The ground arrives; after the screen's minimum the journey lands
        // and then fades away.
        for e in after {
            if let NetEvent::Msg(ServerMsg::Chunk { pos, data }) = e {
                s.world.insert_chunk(pos, Chunk::decode(&data).unwrap());
            }
        }
        s.update_journey(1.2);
        assert!(s.travelling(), "shown for its minimum");
        s.update_journey(1.0 + ARRIVAL_MIN);
        assert!(!s.travelling());
        s.update_journey(1.01 + ARRIVAL_MIN + ARRIVAL_FADE);
        assert!(s.journey.is_none());
    }

    #[test]
    fn the_shrine_is_found_by_name_and_aimed_at() {
        let (mut s, _, msgs) = session_on_the_demo();
        for m in msgs {
            if let ServerMsg::Chunk { pos, data } = m {
                s.world.insert_chunk(pos, Chunk::decode(&data).unwrap());
            }
        }
        let shrine = BlockPos::new(crate::demo::SHRINE[0], crate::demo::SHRINE[1], crate::demo::SHRINE[2]);
        let eye = DVec3::new(128.5, 45.6, 128.5);
        let to = DVec3::new(128.5, 44.5, 131.5) - eye;
        let world = &s.world;
        s.target = raycast(eye, to.normalize(), s.reach(), |p| {
            world.block(p).is_some_and(|b| world.flags.solid(b))
        });
        assert_eq!(s.aiming_at_shrine(), Some(shrine));
    }

    /// Hands what the demo answered to the session, as `handle_msg` does.
    fn deliver(s: &mut Session, events: Vec<NetEvent>) {
        for e in events {
            match e {
                NetEvent::Msg(ServerMsg::BlockHealth { pos, damage }) => s.block_health(pos, damage),
                NetEvent::Msg(ServerMsg::EditResult { seq, pos, block, .. }) => s.edit_result(seq, pos, block),
                NetEvent::Msg(ServerMsg::Notice { .. }) => {}
                NetEvent::Msg(ServerMsg::BlockChanged { pos, block }) => s.block_changed(pos, block),
                NetEvent::Msg(ServerMsg::Slots { changes }) => s.items.apply_slots(&changes),
                _ => {}
            }
        }
    }

    #[test]
    fn swings_predict_what_the_demo_rules() {
        let (mut s, mut demo, msgs) = session_on_the_demo();
        for m in msgs {
            if let ServerMsg::Chunk { pos, data } = m {
                s.world.insert_chunk(pos, Chunk::decode(&data).unwrap());
            }
        }
        // Slot 0 holds a stone hammer; stone lies under the soil. Three
        // swings, each as soon as the cooldown allows.
        let pos = BlockPos::new(128, 39, 128);
        let stone = s.world.registry.id("stone").unwrap();
        assert_eq!(s.world.block(pos), Some(stone));
        let mut at = 5;
        for n in 0..3 {
            let swing_at = s.swinger.next(at, at + 1).unwrap();
            let msg = s.swing(pos, swing_at, 0, DVec3::NEG_Z, &Held::default()).unwrap();
            assert_eq!(s.swinger.next(swing_at, swing_at + 48), None, "cooling down");
            let predicted = s.damage.damage(pos);
            // Broken at once on the third, before the answer.
            assert_eq!(s.world.block(pos) == Some(BlockId::AIR), n == 2, "swing {n}");
            demo.send(msg);
            deliver(&mut s, demo.poll());
            if n < 2 {
                assert_eq!(s.damage.damage(pos), predicted, "the answer agrees");
                assert_eq!(predicted, 400 * (n + 1));
            }
            at = s.swinger.ready_at();
        }
        assert_eq!(s.world.block(pos), Some(BlockId::AIR));
        assert_eq!(s.world.pending_edits(), 0, "the break settled");
        assert!(s.damage.is_empty());

        // A swing at nothing breakable makes none, and starts no cooldown.
        let ready = s.swinger.ready_at();
        assert!(s.swing(pos, ready, 0, DVec3::NEG_Z, &Held::default()).is_none());
        assert_eq!(s.swinger.ready_at(), ready);
    }

    #[test]
    fn a_forged_tool_swings_spends_and_hits_as_the_server_rules() {
        let (mut s, mut demo, msgs) = session_on_the_demo();
        for m in msgs {
            if let ServerMsg::Chunk { pos, data } = m {
                s.world.insert_chunk(pos, Chunk::decode(&data).unwrap());
            }
        }
        // A stone hammer forged Quick 3, Mighty 2 and Thrifty 2, in slot 0
        // on both sides.
        let hammer = s.items.registry.id("stone_hammer").unwrap();
        let forged = ItemStack {
            forge: bm_core::forge::Forged::from_suffix("quick3,mighty2,thrifty2"),
            ..ItemStack::new(hammer, 1)
        };
        demo.put_slot(0, forged);
        deliver(&mut s, demo.poll());
        assert_eq!(s.items.slot(0), Some(forged));
        let def = s.items.registry.get(hammer).unwrap().clone();
        let plain = def.tool().unwrap();
        let t = def.tool_of(&forged, &Held::default()).unwrap();
        assert_eq!(t.action_speed, plain.action_speed + 15.0);
        assert_eq!(t.damage, plain.damage + plain.damage / 10 + 100);
        assert_eq!(t.energy, plain.energy.saturating_sub(4));
        assert_eq!(s.items.tool_of(Some(&forged), &Held::default()), Some(t));
        // The cadence and the energy spent are the forged tool's, through
        // the character's skills exactly as the server applies them.
        let effects = *s.progress.effects();
        let ticks = swing_ticks(skilled_tool(&t, &effects).action_speed);
        assert!(ticks < swing_ticks(skilled_tool(&plain, &effects).action_speed));
        let mut expect = s.me.state;
        assert!(expect.try_spend(t.energy, bm_core::food::Drain::Tool));
        let pos = BlockPos::new(128, 39, 128);
        let at = s.swinger.next(5, 6).unwrap();
        let msg = s.swing(pos, at, 0, DVec3::NEG_Z, &Held::default()).unwrap();
        assert_eq!(s.swinger.ready_at(), at + ticks);
        assert_eq!(s.me.state.energy(), expect.energy());
        let predicted = s.damage.damage(pos);
        assert_eq!(predicted, t.damage, "Mighty's damage, predicted");
        demo.send(msg);
        deliver(&mut s, demo.poll());
        assert_eq!(s.damage.damage(pos), predicted, "the answer agrees");
    }

    #[test]
    fn a_forged_grapple_and_a_glowing_tool_give_their_traits() {
        let (s, _, _) = session_on_the_demo();
        let id = s.items.registry.id("copper_grapple").unwrap();
        let def = s.items.registry.get(id).unwrap();
        let g = ItemStack {
            forge: bm_core::forge::Forged::from_suffix("long_rope2,quick_reel1,hookshot1"),
            ..ItemStack::new(id, 1)
        };
        let base = def.grapple().unwrap();
        let spec = s.items.grapple_of(Some(&g), &Held::default()).unwrap();
        // What the movement step is given, on both sides.
        assert_eq!(Some(spec), def.grapple_of(&g, &Held::default()));
        assert_eq!(spec.range, base.range + 6.0);
        assert_eq!(spec.reel_out_speed, base.reel_out_speed + 5.0);
        assert!(spec.hookshot && spec.reel_in_speed == base.reel_in_speed + 25.0);
        assert_eq!(
            s.items.grapple_of(Some(&ItemStack::new(id, 1)), &Held::default()),
            Some(base),
            "unforged as before"
        );
        // Its shot's energy is the holder's, as the server sets it: Ravenous
        // spends 10 less while hungry.
        let ravenous = ItemStack {
            forge: bm_core::forge::Forged::from_suffix("ravenous1"),
            ..ItemStack::new(id, 1)
        };
        let hungry = Held {
            hunger: Some(0.8),
            ..Held::default()
        };
        let spec = s.items.grapple_of(Some(&ravenous), &hungry).unwrap();
        assert_eq!(spec.energy, base.energy - 10);
        assert_eq!(Some(spec.energy), def.energy_of(&ravenous, &hungry));
        assert_eq!(s.items.grapple_of(Some(&ravenous), &Held::default()), Some(base));
        let hammer = s.items.registry.id("iron_hammer").unwrap();
        let glowing = ItemStack {
            forge: bm_core::forge::Forged::from_suffix("glowing2"),
            ..ItemStack::new(hammer, 1)
        };
        // A Glowing boon lights white.
        assert_eq!(s.items.held_rgb_of(Some(&glowing)), [bm_core::forge::glow_light(2); 3]);
        assert_eq!(s.items.held_rgb_of(Some(&ItemStack::new(hammer, 1))), [0; 3]);
    }

    #[test]
    fn torches_go_on_the_face_aimed_at() {
        let (mut s, _, msgs) = session_on_the_demo();
        for m in msgs {
            if let ServerMsg::Chunk { pos, data } = m {
                s.world.insert_chunk(pos, Chunk::decode(&data).unwrap());
            }
        }
        let torch = s.items.registry.id("torch").unwrap();
        let registry = s.world.registry.clone();
        let block = |n: &str| registry.id(n).unwrap();
        let ground = BlockPos::new(128, 43, 128);
        let aim = |s: &mut Session, at: BlockPos, normal: [i64; 3]| {
            s.target = Some(RayHit {
                block: at,
                normal,
                distance: 2.0,
                point: DVec3::ZERO,
            });
        };
        aim(&mut s, ground, [0, 1, 0]);
        assert_eq!(s.place_choice(torch), Some((block("torch"), Some(Mount::Floor))));
        aim(&mut s, ground, [1, 0, 0]);
        assert_eq!(
            s.place_choice(torch),
            Some((block("torch_west"), Some(Mount::West))),
            "east of the block, hanging on it"
        );
        aim(&mut s, ground, [0, -1, 0]);
        assert_eq!(s.place_choice(torch), None, "never underneath");
        // Ordinary blocks name no mount.
        let stone = s.items.registry.id("stone").unwrap();
        assert_eq!(s.place_choice(stone), Some((block("stone"), None)));
        // A placed torch is aimed at (so it can be broken), though not solid.
        let above = BlockPos::new(128, 44, 128);
        s.world.predict_edit(1, above, BlockState::plain(block("torch")));
        s.aim(DVec3::new(128.5, 46.0, 128.5), DVec3::NEG_Y);
        assert_eq!(s.target.map(|t| t.block), Some(above));
        // But a torch is no support for another.
        aim(&mut s, above, [0, 1, 0]);
        assert_eq!(s.place_choice(torch), None);
    }

    fn app() -> App {
        App::new(Options {
            server: String::new(),
            name: None,
            password: None,
            register: false,
            cert: PathBuf::new(),
            vsync: false,
            assets: PathBuf::from("assets"),
            settings_path: std::env::temp_dir().join("boundmore-test-settings-unused.toml"),
            demo: None,
        })
    }

    fn closed(pos: BlockPos) -> StorageView {
        StorageView {
            pos,
            slots: vec![],
            closed: true,
        }
    }

    #[test]
    fn the_answer_to_our_own_close_does_not_close_a_reopened_chest() {
        let mut a = app();
        let pos = BlockPos::new(1, 2, 3);
        a.open_menu(Menu::Storage(pos));
        a.close_menu(false);
        a.open_menu(Menu::Storage(pos));
        assert_eq!(a.storage_closing, vec![pos]);
        // The server confirms the first close: the window stays.
        a.storage_update(closed(pos));
        assert_eq!(a.menu, Some(Menu::Storage(pos)));
        assert!(a.storage_closing.is_empty());
        // Then it says the block is gone or out of reach: it closes, and
        // nothing more is asked of the server.
        a.storage_update(closed(pos));
        assert_eq!(a.menu, None);
        assert!(a.storage_closing.is_empty());
        // Another block's close never touches this one.
        a.open_menu(Menu::Storage(pos));
        a.storage_update(closed(BlockPos::new(9, 9, 9)));
        assert_eq!(a.menu, Some(Menu::Storage(pos)));
    }

    #[test]
    fn portal_surfaces_are_aimed_at_for_e_only() {
        let (mut s, mut demo, _) = session_on_the_demo();
        demo.show_portals();
        for e in demo.poll() {
            if let NetEvent::Msg(ServerMsg::Chunk { pos, data }) = e {
                s.world.insert_chunk(pos, Chunk::decode(&data).unwrap());
            }
        }
        let [x, y, z] = crate::demo::PORTALS[0];
        let upper = BlockPos::new(x, y + 1, z);
        assert_eq!(s.world.block(upper), Some(s.portal_surface));
        // Standing three blocks in front, looking straight at it.
        s.aim(DVec3::new(x as f64 + 0.5, y as f64 + 1.5, z as f64 + 3.5), DVec3::NEG_Z);
        assert_eq!(s.aiming_at_portal(), Some(upper));
        assert_ne!(s.target.map(|h| h.block), Some(upper), "never broken or built against");
        let frame = bm_core::portal::find_frame(upper, |p| s.is_portal_block(p)).unwrap();
        assert_eq!(
            (frame.width, frame.height, frame.corner),
            (1, 2, BlockPos::new(x, y, z))
        );
        // The brick beside it is no portal.
        s.aim(DVec3::new(x as f64 + 1.5, y as f64 + 1.5, z as f64 + 3.5), DVec3::NEG_Z);
        assert_eq!(s.aiming_at_portal(), None);
    }

    /// An app logged in to the demo, with its portals built and the ground
    /// around the spawn in.
    fn app_on_the_portal_demo() -> App {
        let mut a = app();
        let mut demo = Demo::new("Tester");
        demo.show_portals();
        a.link = Some(Link::Demo(Box::new(demo)));
        a.handle_net();
        assert!(a.session.is_some());
        a
    }

    fn portal_pos() -> BlockPos {
        let [x, y, z] = crate::demo::PORTALS[0];
        BlockPos::new(x, y, z)
    }

    #[test]
    fn the_portal_window_follows_the_server() {
        let mut a = app_on_the_portal_demo();
        let s = a.session.as_ref().unwrap();
        let token = s.items.registry.id("portal_token").unwrap();
        let tip = s.items.tooltip(&ItemStack::tagged(token, 3));
        assert_eq!(tip, "Portal Token\nLinks to: 1 by 2, Saffra, near 212, 96");
        a.open_menu(Menu::Portal(portal_pos()));
        assert!(a.portal_view.is_none(), "opening");
        a.handle_net();
        let v = a.portal_view.clone().expect("the view");
        assert!(v.open && v.fuel == 37 && v.link.is_some());
        // Add fuel: every wayshard carried goes in, stack by stack.
        let s = a.session.as_ref().unwrap();
        let carried = crate::portal::wayshards_carried(&s.items);
        assert_eq!(carried, 100);
        let b = crate::portal::buttons(&s.items, &v, portal_pos());
        a.apply_actions(b.add_fuel.into_iter().map(UiAction::Send).collect());
        a.handle_net();
        assert_eq!(a.portal_view.as_ref().unwrap().fuel, 137);
        assert_eq!(crate::portal::wayshards_carried(&a.session.as_ref().unwrap().items), 0);
        // A late close of another portal (left just before this one opened)
        // leaves this window alone (M8 follow-up).
        a.portal_closed(Some(v.id + 1));
        assert!(a.menu.is_some(), "another portal's close");
        // The server closes it: the window goes, asking nothing.
        a.portal_closed(Some(v.id));
        assert_eq!(a.menu, None);
        // A refusal while opening is told and closes the waiting window.
        a.open_menu(Menu::Portal(BlockPos::new(0, 44, 0)));
        a.handle_net();
        assert_eq!(a.menu, None);
        assert!(
            a.notices
                .visible(a.now())
                .iter()
                .any(|(t, _)| t == bm_core::portal::SHAPE_RULE)
        );
        // E again closes it as any window.
        a.open_menu(Menu::Portal(portal_pos()));
        a.handle_net();
        assert!(a.portal_view.is_some());
        a.close_menu(true);
        assert_eq!((a.menu, a.portal_view.clone()), (None, None));
    }

    #[test]
    fn a_teleport_moves_us_and_closes_the_portal_window_only() {
        let mut a = app_on_the_portal_demo();
        a.open_menu(Menu::Portal(portal_pos()));
        a.handle_net();
        let chunks = a.session.as_ref().unwrap().world.chunk_count();
        let to = PlayerState {
            pos: DVec3::new(132.5, 44.0, 123.5),
            ..Default::default()
        };
        a.yaw = 0.0;
        a.handle_msg(ServerMsg::Teleport {
            spawn: to,
            tick: 500,
            yaw: Some(1.5),
        });
        assert_eq!(a.menu, None);
        assert_eq!(a.yaw, 1.5, "facing out of the portal");
        let s = a.session.as_ref().unwrap();
        assert_eq!(s.me.state, to);
        assert_eq!(s.world.chunk_count(), chunks, "chunks carry on");
        assert!(s.journey.is_none(), "no travel screen on the same planet");
        assert_eq!(s.me.corrections, 0);
        // An older snapshot does not pull us back.
        a.handle_snapshot(snapshot(0, 400, 0, 128.5), true);
        assert_eq!(a.session.as_ref().unwrap().me.state, to);
    }

    #[test]
    fn chat_is_opened_and_closed_without_regrabbing_under_a_menu() {
        let mut a = app();
        a.open_chat(true);
        assert!(a.chat.open && a.chat.swallow);
        a.menu = Some(Menu::Inventory);
        a.close_chat();
        assert!(!a.chat.open && !a.grabbed);
    }
}

/// A creature's name plate: "Exotic Burning Grim Pelter".
fn plate_title(title: &str, exotic: bool, element: Option<bm_core::creatures::Essence>) -> String {
    let mut t = match element {
        Some(e) => format!("{} {title}", e.word()),
        None => title.to_string(),
    };
    if exotic {
        t = format!("Exotic {t}");
    }
    t
}

#[cfg(test)]
mod plate_tests {
    use super::plate_title;
    use bm_core::creatures::{Astral, Element, Essence};

    #[test]
    fn plates_name_the_element() {
        assert_eq!(plate_title("Grim Pelter", false, None), "Grim Pelter");
        assert_eq!(
            plate_title("Grim Pelter", true, Some(Essence::Element(Element::Burn))),
            "Exotic Burning Grim Pelter"
        );
        assert_eq!(
            plate_title("Grazer", false, Some(Essence::Element(Element::Chill))),
            "Chilling Grazer"
        );
        assert_eq!(
            plate_title("Grazer", false, Some(Essence::Astral(Astral::Aurorite))),
            "Auroral Grazer"
        );
    }
}
