//! The client application: window events, the fixed-rate simulation, and the
//! glue between network, world, prediction, menus and renderer.

use crate::chat::{ChatAction, ChatBox};
use crate::claims::{Claims, outline};
use crate::icons::icon_for;
use crate::items::Items;
use crate::menus::{
    self, Arrival, CraftingView, Drag, Hud, ItemIcons, LiveStation, LoginForm, Menu, Notices, ShrineTab, StorageWindow,
    TravelView, UiAction,
};
use crate::net::{self, ConnectOptions, Link, NetEvent};
use crate::predict::{Predictor, RemoteSample, Remotes};
use crate::render::{BoxInstance, Camera, EguiFrame, LineVertex, Renderer, Scene};
use crate::settings::Settings;
use crate::sky::Sky;
use crate::textures::TextureSet;
use crate::world::ClientWorld;
use bm_core::movement::{MoveConfig, PlayerInput};
use bm_core::{
    BlockId, BlockPos, BlockRegistry, BlockState, Chunk, Colour, DayCycle, HOTBAR_SLOTS, ItemId, ItemStack, PlanetLook,
    PlanetShape, PlotPos, RayHit, Shape, TICK_DT, raycast, raycast_shaped,
};
use bm_protocol::{
    ClientDatagram, ClientMsg, EditAction, INPUT_REDUNDANCY, MIN_PASSWORD_LEN, PlanetInfo, PlanetSummary,
    ServerDatagram, ServerMsg, StorageView, Travel,
};
use glam::{DVec3, Vec3};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Fullscreen, Window, WindowId};

const MOUSE_SENSITIVITY: f64 = 0.0022;
/// Seconds between repeated places while the button is held.
const PLACE_REPEAT: f64 = 0.22;
const MAX_TICKS_PER_FRAME: u32 = 8;
const PLAYER_VISUAL_HEIGHT: f32 = 2.0;
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
    /// `coloured-icons`, `reclaim` or `login`).
    pub demo: Option<String>,
}

/// The block being broken: where, holding what, and how far along (0 to 1).
#[derive(Clone, Copy)]
struct Mining {
    pos: BlockPos,
    slot: usize,
    item: Option<ItemId>,
    progress: f64,
}

/// The journey to another planet, shown until its ground has arrived.
struct Journey {
    to: Arrival,
    started: f64,
    /// When the chunks around us had arrived; the screen fades from here.
    landed: Option<f64>,
}

/// Everything that exists once the server has welcomed us.
struct Session {
    /// The planet we are on: everything in `world`, `claims` and `remotes`
    /// belongs to it.
    planet: PlanetInfo,
    /// Every planet on the server, for the travel menu and the sky.
    universe: Vec<PlanetSummary>,
    account: String,
    /// Server tick in the welcome and our clock when it arrived, for the time
    /// of day before the first snapshot.
    welcome_tick: u64,
    welcome_local: f64,
    movement: MoveConfig,
    reach: f64,
    view_radius: u8,
    world: ClientWorld,
    me: Predictor,
    remotes: Remotes,
    items: Items,
    icons: ItemIcons,
    claims: Claims,
    plot_allowance: u32,
    /// Hotbar slot, 0 to 9.
    selected: usize,
    edit_seq: u32,
    place_cooldown: f64,
    target: Option<RayHit>,
    /// With a chisel in hand: the aimed block and what each click would do.
    chisel: Option<(BlockPos, crate::chisel::Plan)>,
    /// The plot claim mode points at.
    claim_target: Option<PlotPos>,
    mining: Option<Mining>,
    warp_shrine: BlockId,
    /// Kept to repaint item icons in a new planet's tints.
    textures: TextureSet,
    journey: Option<Journey>,
    /// What waits in this account's reclaim (`ServerMsg::Reclaim`).
    reclaim: Vec<ItemStack>,
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
        let shape = w.planet.shape;
        Ok(Self {
            account: w.account,
            welcome_tick: w.tick,
            welcome_local: local_ticks,
            movement: w.movement,
            reach: w.reach,
            view_radius: w.view_radius,
            claims: Claims::new(shape, w.claims),
            world: ClientWorld::new(shape, registry),
            me: Predictor::new(w.spawn),
            remotes: Remotes::default(),
            items,
            icons,
            plot_allowance: w.plot_allowance,
            selected: 0,
            edit_seq: 0,
            place_cooldown: 0.0,
            target: None,
            chisel: None,
            claim_target: None,
            mining: None,
            warp_shrine,
            textures,
            journey: None,
            reclaim: Vec::new(),
            planet: w.planet,
            universe: w.universe,
        })
    }

    /// Moves us to another planet. Everything of the old one goes: chunks,
    /// meshes in flight (the new world has its own mesher), light, edits
    /// waiting for an answer, remote players, claims, aim and mining.
    /// Inventory, hotbar choice and input sequences carry on.
    fn arrive(&mut self, t: Travel, now: f64, local_ticks: f64) {
        let shape = t.planet.shape;
        let registry = self.world.registry.clone();
        self.world = ClientWorld::new(shape, registry);
        self.claims = Claims::new(shape, t.claims);
        self.remotes = Remotes::default();
        self.me.teleport(t.spawn);
        self.welcome_tick = t.tick;
        self.welcome_local = local_ticks;
        self.target = None;
        self.claim_target = None;
        self.mining = None;
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

    /// Applies a snapshot; one about another planet (sent before we
    /// travelled, overtaking the stream) is dropped. Returns whether it was used.
    fn snapshot(&mut self, d: ServerDatagram, local: f64) -> bool {
        let ServerDatagram::Snapshot {
            planet,
            tick,
            ack,
            you,
            others,
        } = d;
        if planet != self.planet.id {
            return false;
        }
        let shape = self.world.shape;
        self.me.reconcile(tick, ack, you, &self.movement, shape, &self.world);
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

    /// The storage block aimed at (within reach, as aiming is), if any.
    fn aiming_at_storage(&self) -> Option<BlockPos> {
        let hit = self.target?;
        let block = self.world.block(hit.block)?;
        (self.world.registry.get(block).storage > 0).then_some(hit.block)
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
    /// The warp shrine window's open tab.
    shrine_tab: ShrineTab,
    /// Storage blocks we closed whose `closed` answer has not come back yet:
    /// that answer must not close the same block opened again meanwhile.
    storage_closing: Vec<BlockPos>,
    chat: ChatBox,
    claim_mode: bool,
    notices: Notices,
    status: String,
    keys: HashSet<KeyCode>,
    mouse: [bool; 2],
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
            shrine_tab: ShrineTab::Worlds,
            storage_closing: Vec::new(),
            chat: ChatBox::default(),
            claim_mode: false,
            notices: Notices::default(),
            status: "logging in".into(),
            keys: HashSet::new(),
            mouse: [false; 2],
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
            self.send(ClientMsg::WatchStation { pos: Some(pos) });
        }
        if let Menu::Storage(pos) = menu {
            self.storage_view = None;
            self.send(ClientMsg::OpenStorage { pos: Some(pos) });
        }
        if let Menu::Travel(_) = menu {
            self.shrine_tab = ShrineTab::Worlds;
        }
        self.menu = Some(menu);
        self.keys.clear();
        self.mouse = [false; 2];
        if let Some(s) = &mut self.session {
            s.mining = None;
        }
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
            s.world.registry.get(block).station.map(|_| hit.block)
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
        if let Some(s) = &mut self.session {
            s.mining = None;
        }
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
        if let Some(c) = s.claims.at(PlotPos::of(pos, s.world.shape))
            && c.owner != s.account
        {
            let text = format!("This land belongs to {}", c.owner);
            self.notices.push(text, now);
            return;
        }
        // The server names storage by canonical position.
        let pos = s.world.shape.wrap_block(pos);
        self.open_menu(Menu::Storage(pos));
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
                NetEvent::Snapshot(d) => self.handle_snapshot(d),
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
            m => m,
        };
        let now = self.now();
        let Some(s) = &mut self.session else { return };
        match m {
            ServerMsg::Chunk { pos, data } => match Chunk::decode(&data) {
                Ok(c) => s.world.insert_chunk(pos, c),
                Err(e) => tracing::warn!("bad chunk {pos:?} from server: {e}"),
            },
            ServerMsg::Forget { pos } => {
                s.world.forget_chunk(pos);
                if let Some(r) = &mut self.renderer {
                    r.remove_chunk(s.world.shape.wrap_chunk(pos));
                }
            }
            ServerMsg::BlockChanged { pos, block } => s.world.set_block(pos, block),
            ServerMsg::EditResult { seq, pos, block, .. } => s.world.apply_edit_result(seq, pos, block),
            ServerMsg::PlayerJoined { id, name } => s.remotes.joined(id, name),
            ServerMsg::PlayerLeft { id } => s.remotes.left(id),
            ServerMsg::Slots { changes } => s.items.apply_slots(&changes),
            ServerMsg::Station(view) => {
                if self.menu == Some(Menu::Crafting(Some(view.pos))) {
                    self.live_station = Some(LiveStation { view, received: now });
                }
            }
            ServerMsg::Claim(c) => s.claims.upsert(c),
            ServerMsg::ClaimRemoved { id } => s.claims.remove(id),
            ServerMsg::Notice { text } => self.notices.push(text, now),
            ServerMsg::Universe { planets } => s.universe = planets,
            ServerMsg::Chat { lines } => self.chat.receive(lines, now),
            ServerMsg::Reclaim { items } => {
                s.reclaim = items
                    .into_iter()
                    .filter(|i| s.items.registry.contains(i.item) && i.count > 0)
                    .collect();
            }
            ServerMsg::Welcome(_) | ServerMsg::Refused { .. } | ServerMsg::Travel(_) | ServerMsg::Storage(_) => {}
        }
    }

    fn handle_snapshot(&mut self, d: ServerDatagram) {
        let local = self.local_ticks();
        if let Some(s) = &mut self.session {
            s.snapshot(d, local);
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
                    if s.claims.plots_owned_by(&s.account) as u32 >= s.plot_allowance {
                        Err(format!("You hold all {} plots you may claim.", s.plot_allowance))
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
        }
    }

    fn update(&mut self) {
        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f64().min(0.25);
        self.last_frame = now;
        self.handle_net();

        let input = self.sample_input();
        let playing = self.grabbed && self.menu.is_none() && !self.chat.open;
        let mouse = if playing && !self.claim_mode {
            self.mouse
        } else {
            [false; 2]
        };
        let claim_mode = self.claim_mode;
        let (yaw, pitch) = (self.yaw, self.pitch);
        let now_s = self.now();
        let Some(s) = &mut self.session else { return };
        let Some(link) = &mut self.link else { return };
        s.update_journey(now_s);
        // Nothing is broken or placed on the way between planets.
        let mouse = if s.travelling() { [false; 2] } else { mouse };

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
        let mut ticks = 0;
        while self.accumulator >= TICK_DT && ticks < MAX_TICKS_PER_FRAME {
            self.accumulator -= TICK_DT;
            ticks += 1;
            let seq = s.me.tick(input, &s.movement, s.world.shape, &s.world);
            link.send_datagram(ClientDatagram::Input {
                latest: seq,
                inputs: s.me.recent_inputs(INPUT_REDUNDANCY),
            });
        }
        if ticks == MAX_TICKS_PER_FRAME {
            self.accumulator = 0.0;
        }
        s.me.decay_correction(dt);

        // Targeting and editing.
        let eye = s.me.state.eye(&s.movement);
        let dir = Camera {
            eye,
            yaw,
            pitch,
            fov_y: 1.0,
        }
        .forward();
        let world = &s.world;
        let solid = |p: BlockPos| world.block(p).is_some_and(|b| world.flags.solid(b));
        // Cut blocks are aimed at by their kept octants, as the server does.
        s.target = raycast_shaped(eye, dir, s.reach, |p| {
            let st = world.state(p)?;
            world.flags.solid(st.id).then(|| world.block_shape(p))
        });
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

        // Breaking takes `break_seconds` of holding: the block's hardness,
        // shortened by a suitable tool in the selected slot. The server times
        // the same hold from our StartBreak (ADR-0004), so the progress here
        // is a prediction of what it will accept.
        let slot = s.selected;
        let held = s.items.slot(slot);
        // A chisel in hand cuts with the left button and fills with the
        // right, instead of breaking and placing.
        let chisel = held.and_then(|h| s.items.def(&h)).and_then(|d| d.chisel());
        s.place_cooldown -= dt;
        s.chisel = None;
        if let (Some((family, precise, _)), Some(hit)) = (chisel, s.target) {
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
                s.place_cooldown = PLACE_REPEAT;
                let state = s.world.state(hit.block).unwrap_or(BlockState::AIR);
                let def = s.world.registry.get(state.id);
                match bm_core::chisel::apply(def, state, family, precise, octants, restore) {
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
                            s.world
                                .predict_edit(s.edit_seq, hit.block, BlockState { shape: new, ..state });
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
        let mouse = if chisel.is_some() { [false; 2] } else { mouse };
        let mut mining = s.mining.take();
        if mouse[0]
            && let Some(hit) = s.target
        {
            let block = s.world.block(hit.block).unwrap_or(BlockId::AIR);
            let def = s.world.registry.get(block);
            if def.breakable {
                let item = held.map(|h| h.item);
                let seconds = s.items.registry.break_seconds(def, item) as f64;
                let progress = match mining {
                    Some(m) if m.pos == hit.block && m.slot == slot && m.item == item => m.progress,
                    _ => {
                        if seconds > 0.0 {
                            link.send(ClientMsg::StartBreak {
                                at: s.me.seq,
                                pos: hit.block,
                                slot: slot as u8,
                            });
                        }
                        0.0
                    }
                };
                let progress = if seconds > 0.0 { progress + dt / seconds } else { 1.0 };
                if progress >= 1.0 {
                    s.edit_seq += 1;
                    // With no room for what it yields the server refuses the
                    // break; predicting it would only flash the block away.
                    let fits = s
                        .items
                        .registry
                        .drop_of(block)
                        .is_none_or(|d| s.items.inventory.can_fit(&s.items.registry, d, 1));
                    if fits {
                        s.world.predict_edit(s.edit_seq, hit.block, BlockState::AIR);
                    }
                    link.send(ClientMsg::Edit {
                        seq: s.edit_seq,
                        at: s.me.seq,
                        pos: hit.block,
                        action: EditAction::Break,
                    });
                    mining = None;
                } else {
                    mining = Some(Mining {
                        pos: hit.block,
                        slot,
                        item,
                        progress,
                    });
                }
            } else {
                mining = None;
            }
        } else {
            mining = None;
        }
        s.mining = mining;

        // Placing spends one of the selected item, so only a block item can.
        let place_block = held.and_then(|h| s.items.def(&h)).and_then(|d| d.block());
        if mouse[1]
            && s.place_cooldown <= 0.0
            && let (Some(hit), Some(block)) = (s.target, place_block)
        {
            let at = hit.adjacent();
            let free = s
                .world
                .block(at)
                .is_some_and(|b| b.is_air() || s.world.registry.get(b).name == "water");
            let blocks_me = s.world.registry.get(block).solid && s.me.state.overlaps_block(&s.movement, at);
            if free && !blocks_me && s.world.shape.contains_y(at.y) {
                s.edit_seq += 1;
                // The block takes the stack's colour, as the server will.
                let colour = match held {
                    Some(h) if s.world.registry.get(block).colourable => h.colour,
                    _ => Colour::NATURAL,
                };
                s.world
                    .predict_edit(s.edit_seq, at, BlockState::coloured(block, colour));
                link.send(ClientMsg::Edit {
                    seq: s.edit_seq,
                    at: s.me.seq,
                    pos: at,
                    action: EditAction::Place { slot: slot as u8 },
                });
                s.place_cooldown = PLACE_REPEAT;
            }
        }

        // Meshing and uploads.
        let center = BlockPos::containing(eye.x, eye.y, eye.z).chunk();
        s.world
            .schedule_meshing(s.world.shape.wrap_chunk(center), rayon::current_num_threads() * 2);
        if let Some(r) = &mut self.renderer {
            for m in s.world.take_meshes() {
                r.upload_chunk(m.pos, &m.mesh);
            }
        }
    }

    /// Runs this frame's menus and HUD and returns them tessellated.
    fn run_ui(&mut self) -> Option<(Vec<egui::ClippedPrimitive>, egui::TexturesDelta, f32)> {
        self.ensure_coloured_icons();
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
        let notices = self.notices.visible(now);
        let menu = self.menu;
        let login = &mut self.login;
        let drag = &mut self.drag;
        let session = self.session.as_ref();
        let live = self.live_station.as_ref();
        let claim_mode = self.claim_mode;
        let shrine_tab = &mut self.shrine_tab;
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
            let claim_help = claim_mode.then(|| {
                format!(
                    "Left click a plot next to your claim to add it. Right click one of yours to give it up.\n\
                     {} of {} plots claimed. P to leave.",
                    s.claims.plots_owned_by(&s.account),
                    s.plot_allowance
                )
            });
            let aim_hint = if menu.is_some() || claim_mode {
                None
            } else if s.aiming_at_shrine().is_some() {
                Some(("E", "travel"))
            } else if s.aiming_at_storage().is_some() {
                Some(("E", "open"))
            } else {
                None
            };
            menus::hud(
                &ctx,
                &Hud {
                    items: &s.items,
                    icons: &s.icons,
                    selected: s.selected,
                    notices: notices.clone(),
                    owner,
                    claim_mode: claim_help,
                    aim_hint,
                    reclaim_waiting: !s.reclaim.is_empty(),
                },
            );
            match menu {
                Some(Menu::Inventory) => menus::inventory(&ctx, &s.items, &s.icons, s.selected, drag, &mut actions),
                Some(Menu::Crafting(station)) => {
                    let station = station.and_then(|pos| {
                        let block = s.world.block(pos)?;
                        let kind = s.world.registry.get(block).station?;
                        Some((pos, kind, menus::station_name(&s.world.registry, kind)))
                    });
                    menus::crafting(
                        &ctx,
                        &CraftingView {
                            items: &s.items,
                            icons: &s.icons,
                            station,
                            live,
                            now,
                        },
                        &mut actions,
                    );
                }
                Some(Menu::Travel(shrine)) => menus::travel(
                    &ctx,
                    &TravelView {
                        planets: &s.universe,
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
                        .map(|d| bm_core::items::title_case(&d.name))
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
                }
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
        for a in chat_actions {
            match a {
                ChatAction::Send(m) => self.send(m),
                ChatAction::Close => self.close_chat(),
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
        let ui = self.run_ui();
        let ui_frame = ui.as_ref().map(|(p, t, ppp)| EguiFrame {
            primitives: p,
            textures: t,
            pixels_per_point: *ppp,
        });
        let alpha = self.accumulator / TICK_DT;
        let local = self.local_ticks();
        let time = self.start.elapsed().as_secs_f32();
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
        let feet = s.me.render_pos(alpha, shape);
        let eye = shape.wrap_point(feet + DVec3::new(0.0, s.movement.eye_height, 0.0));
        let camera = Camera {
            eye,
            yaw,
            pitch,
            fov_y: 75f32.to_radians(),
        };
        let boxes: Vec<BoxInstance> = s
            .remotes
            .sample(local, shape)
            .into_iter()
            .map(|(id, smp)| {
                let pos = shape.nearest_lap(eye, smp.pos);
                // Drawn two blocks tall, a touch above the collision box.
                let half = Vec3::new(
                    s.movement.half_width as f32,
                    PLAYER_VISUAL_HEIGHT / 2.0,
                    s.movement.half_width as f32,
                );
                let centre = (pos - eye).as_vec3() + Vec3::new(0.0, half.y, 0.0);
                BoxInstance {
                    center: centre.to_array(),
                    half_size: half.to_array(),
                    color: player_colour(id),
                }
            })
            .collect();
        let (mut lines, xray) = self.claim_lines(eye);
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
        let view_radius = s.view_radius;
        let breaking = s.mining.map(|m| (m.pos, m.progress as f32));
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
            fog_end: (view_radius as f32 - 1.0) * 32.0,
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
                let (chunks, meshes, quads) = self
                    .renderer
                    .as_ref()
                    .map(|r| (r.stats.chunks_drawn, r.chunk_meshes(), r.stats.quads_drawn))
                    .unwrap_or_default();
                format!(
                    "Boundmore | {} | {:.0} fps | {:.0} ms | {:.0} {:.0} {:.0} | {} chunks, {} drawn, {}k quads, {} to mesh | {} corrections | [{}] {}",
                    s.planet.name,
                    self.fps,
                    self.link.as_ref().map(|n| n.rtt_ms()).unwrap_or(0.0),
                    p.x,
                    p.y,
                    p.z,
                    meshes,
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
            "crafting" => self.open_menu(Menu::Crafting(None)),
            "station" => {
                // Face the demo workbench, two blocks ahead and below the eye.
                self.pitch = -0.5;
                self.open_menu(Menu::Crafting(Some(BlockPos::new(128, 44, 126))));
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
                    });
                    if in_hand {
                        s.selected = 6;
                    }
                }
                (self.yaw, self.pitch) = if in_hand { (0.075, -0.55) } else { (0.0, -0.62) };
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
                    KeyCode::KeyC => {
                        let m = self.crafting_menu();
                        self.toggle(m);
                    }
                    KeyCode::KeyE => {
                        if matches!(self.menu, Some(Menu::Travel(_) | Menu::Storage(_))) {
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
                            }
                        }
                    }
                    KeyCode::KeyP if self.menu.is_none() => {
                        self.claim_mode = !self.claim_mode;
                        self.mouse = [false; 2];
                    }
                    _ => {}
                }
                if let (Some(d), Some(s)) = (digit_slot(code), &mut self.session) {
                    s.selected = d;
                }
                if self.menu.is_none() {
                    self.keys.insert(code);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if !in_game || self.menu.is_some() || self.chat.open {
                    return; // the menus (or the chat box) have it
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
                    MouseButton::Left => self.mouse[0] = down,
                    MouseButton::Right => self.mouse[1] = down,
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
                if let Some(s) = &mut self.session {
                    s.selected = (s.selected as i32 - steps).rem_euclid(HOTBAR_SLOTS as i32) as usize;
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
    use super::*;
    use crate::demo::Demo;
    use crate::net::NetEvent;
    use bm_protocol::RemotePlayer;

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
            }],
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
        assert!(s.snapshot(snapshot(0, 10, 0, 128.5), 10.0));
        s.remotes.joined(9, "Wren".into());
        assert!(!s.remotes.sample(30.0, s.world.shape).is_empty());
        assert!(s.claims.iter().count() > 0);
        s.world.predict_edit(1, BlockPos::new(128, 43, 128), BlockState::AIR);
        assert_eq!(s.world.pending_edits(), 1);
        s.mining = Some(Mining {
            pos: BlockPos::new(129, 43, 128),
            slot: 0,
            item: None,
            progress: 0.5,
        });
        let centre = BlockPos::new(128, 44, 128).chunk();
        s.world.schedule_meshing(centre, 64);
        for _ in 0..12 {
            s.me.tick(PlayerInput::default(), &s.movement, s.world.shape, &s.world);
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
        assert!(s.mining.is_none() && s.target.is_none());
        assert_eq!(s.me.state, spawn);
        assert_eq!(s.me.seq, seq, "input sequences carry on");
        assert!(s.travelling());
        // Meshes started on the old planet never come out of the new world.
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert!(s.world.take_meshes().is_empty());
        // A late snapshot of the old planet is dropped; the new one's is used.
        assert!(!s.snapshot(snapshot(0, 20, seq, 1.5), 61.0));
        assert_eq!(s.me.state, spawn);
        assert!(s.remotes.sample(90.0, s.world.shape).is_empty());
        assert!(s.snapshot(snapshot(2, 21, seq, 128.5), 62.0));
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
        s.target = raycast(eye, to.normalize(), s.reach, |p| {
            world.block(p).is_some_and(|b| world.flags.solid(b))
        });
        assert_eq!(s.aiming_at_shrine(), Some(shrine));
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
    fn chat_is_opened_and_closed_without_regrabbing_under_a_menu() {
        let mut a = app();
        a.open_chat(true);
        assert!(a.chat.open && a.chat.swallow);
        a.menu = Some(Menu::Inventory);
        a.close_chat();
        assert!(!a.chat.open && !a.grabbed);
    }
}
