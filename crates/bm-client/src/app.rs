//! The client application: window events, the fixed-rate simulation, and the
//! glue between network, world, prediction and renderer.

use crate::net::{self, ConnectOptions, Net, NetEvent};
use crate::predict::{Predictor, RemoteSample, Remotes};
use crate::render::{BoxInstance, Camera, Renderer, Scene};
use crate::world::ClientWorld;
use bm_core::movement::{MoveConfig, PlayerInput};
use bm_core::{BlockId, BlockPos, BlockRegistry, Chunk, PlanetShape, RayHit, TICK_DT, raycast};
use bm_protocol::{ClientDatagram, ClientMsg, EditAction, INPUT_REDUNDANCY, ServerDatagram, ServerMsg};
use glam::{DVec3, Vec3};
use std::collections::HashSet;
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

pub struct Options {
    pub connect: ConnectOptions,
    pub vsync: bool,
}

/// Everything that exists once the server has welcomed us.
struct Session {
    planet_name: String,
    movement: MoveConfig,
    reach: f64,
    view_radius: u8,
    world: ClientWorld,
    me: Predictor,
    remotes: Remotes,
    hotbar: Vec<BlockId>,
    selected: usize,
    edit_seq: u32,
    place_cooldown: f64,
    target: Option<RayHit>,
    /// The block being broken and how far along, 0 to 1.
    mining: Option<(BlockPos, f64)>,
}

pub struct App {
    opts: Options,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    net: Option<Net>,
    session: Option<Session>,
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
}

impl App {
    pub fn new(opts: Options) -> Self {
        let now = Instant::now();
        Self {
            opts,
            window: None,
            renderer: None,
            net: None,
            session: None,
            status: "connecting".into(),
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
        }
    }

    fn local_ticks(&self) -> f64 {
        self.start.elapsed().as_secs_f64() / TICK_DT
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

    fn handle_net(&mut self) {
        let Some(net) = &self.net else { return };
        let events: Vec<NetEvent> = net.events.try_iter().collect();
        for ev in events {
            match ev {
                NetEvent::Connected(w) => match BlockRegistry::from_defs(w.blocks.clone()) {
                    Ok(registry) => {
                        if let Some(r) = &mut self.renderer {
                            r.write_palette(&registry);
                        }
                        let hotbar: Vec<BlockId> =
                            registry.iter().filter(|(_, d)| d.placeable).map(|(id, _)| id).collect();
                        tracing::info!("welcomed to {} as #{}", w.planet_name, w.player_id);
                        self.status = format!("on {}", w.planet_name);
                        self.session = Some(Session {
                            planet_name: w.planet_name.clone(),
                            movement: w.movement,
                            reach: w.reach,
                            view_radius: w.view_radius,
                            world: ClientWorld::new(w.planet, registry),
                            me: Predictor::new(w.spawn),
                            remotes: Remotes::default(),
                            hotbar,
                            selected: 0,
                            edit_seq: 0,
                            place_cooldown: 0.0,
                            target: None,
                            mining: None,
                        });
                    }
                    Err(e) => self.status = format!("the server sent a bad block catalogue: {e}"),
                },
                NetEvent::Msg(m) => self.handle_msg(m),
                NetEvent::Snapshot(d) => self.handle_snapshot(d),
                NetEvent::Disconnected(reason) => {
                    tracing::warn!("disconnected: {reason}");
                    self.status = format!("disconnected: {reason}");
                    self.session = None;
                }
            }
        }
    }

    fn handle_msg(&mut self, m: ServerMsg) {
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
            ServerMsg::Welcome(_) | ServerMsg::Refused { .. } => {}
        }
    }

    fn handle_snapshot(&mut self, d: ServerDatagram) {
        let local = self.local_ticks();
        let Some(s) = &mut self.session else { return };
        let ServerDatagram::Snapshot { tick, ack, you, others } = d;
        let shape = s.world.shape;
        s.me.reconcile(tick, ack, you, &s.movement, shape, &s.world);
        s.remotes.record(
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
    }

    fn sample_input(&self) -> PlayerInput {
        let k = |c: KeyCode| self.grabbed && self.keys.contains(&c);
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
        let mouse = self.mouse;
        let grabbed = self.grabbed;
        let (yaw, pitch) = (self.yaw, self.pitch);
        let Some(s) = &mut self.session else { return };
        let net = self.net.as_ref().expect("session implies net");

        // Fixed-rate simulation, decoupled from frame rate. Keeps ticking when
        // the window is hidden, so the server keeps receiving inputs.
        self.accumulator += dt;
        let mut ticks = 0;
        while self.accumulator >= TICK_DT && ticks < MAX_TICKS_PER_FRAME {
            self.accumulator -= TICK_DT;
            ticks += 1;
            let seq = s.me.tick(input, &s.movement, s.world.shape, &s.world);
            net.send_datagram(ClientDatagram::Input {
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
        s.target = raycast(eye, dir, s.reach, |p| {
            world.block(p).is_some_and(|b| world.flags.solid(b))
        });
        // Breaking takes the block's hardness in seconds of holding. The
        // server times the same hold from our StartBreak (ADR-0004), so the
        // progress here is a prediction of what it will accept.
        let breaking = grabbed && mouse[0];
        let mut mining = s.mining.take();
        if breaking && let Some(hit) = s.target {
            let def = s.world.registry.get(s.world.block(hit.block).unwrap_or(BlockId::AIR));
            if def.breakable {
                let hardness = def.hardness as f64;
                let progress = match mining {
                    Some((b, p)) if b == hit.block => p,
                    _ => {
                        if hardness > 0.0 {
                            net.send(ClientMsg::StartBreak { pos: hit.block });
                        }
                        0.0
                    }
                };
                let progress = if hardness > 0.0 { progress + dt / hardness } else { 1.0 };
                if progress >= 1.0 {
                    s.edit_seq += 1;
                    s.world.predict_edit(s.edit_seq, hit.block, BlockId::AIR);
                    net.send(ClientMsg::Edit {
                        seq: s.edit_seq,
                        pos: hit.block,
                        action: EditAction::Break,
                    });
                    mining = None;
                } else {
                    mining = Some((hit.block, progress));
                }
            } else {
                mining = None;
            }
        } else {
            mining = None;
        }
        s.mining = mining;

        s.place_cooldown -= dt;
        if grabbed
            && mouse[1]
            && s.place_cooldown <= 0.0
            && let (Some(hit), Some(&block)) = (s.target, s.hotbar.get(s.selected))
        {
            let at = hit.adjacent();
            let free = s
                .world
                .block(at)
                .is_some_and(|b| b.is_air() || s.world.registry.get(b).name == "water");
            let blocks_me = s.world.registry.get(block).solid && s.me.state.overlaps_block(&s.movement, at);
            if free && !blocks_me && s.world.shape.contains_y(at.y) {
                s.edit_seq += 1;
                s.world.predict_edit(s.edit_seq, at, block);
                net.send(ClientMsg::Edit {
                    seq: s.edit_seq,
                    pos: at,
                    action: EditAction::Place(block),
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

    fn render(&mut self) {
        let alpha = self.accumulator / TICK_DT;
        let local = self.local_ticks();
        let time = self.start.elapsed().as_secs_f32();
        let (yaw, pitch) = (self.yaw, self.pitch);
        let Some(r) = &mut self.renderer else { return };
        let Some(s) = &self.session else {
            // Nothing to draw yet: an empty sky.
            let cam = Camera {
                eye: DVec3::ZERO,
                yaw,
                pitch,
                fov_y: 75f32.to_radians(),
            };
            r.render(&Scene {
                camera: &cam,
                shape: PlanetShape::new(8, 1),
                boxes: &[],
                highlight: None,
                fog_end: 100.0,
                time,
            });
            return;
        };
        let shape = s.world.shape;
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
        r.render(&Scene {
            camera: &camera,
            shape,
            boxes: &boxes,
            highlight: s.target.map(|t| t.block),
            fog_end: (s.view_radius as f32 - 1.0) * 32.0,
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
                let block = s
                    .hotbar
                    .get(s.selected)
                    .map(|b| s.world.registry.get(*b).name.clone())
                    .unwrap_or_default();
                let (chunks, meshes, quads) = self
                    .renderer
                    .as_ref()
                    .map(|r| (r.stats.chunks_drawn, r.chunk_meshes(), r.stats.quads_drawn))
                    .unwrap_or_default();
                format!(
                    "Boundmore | {} | {:.0} fps | {:.0} ms | {:.0} {:.0} {:.0} | {} chunks, {} drawn, {}k quads, {} to mesh | {} corrections | [{}] {}",
                    s.planet_name,
                    self.fps,
                    self.net.as_ref().map(|n| n.rtt_ms()).unwrap_or(0.0),
                    p.x,
                    p.y,
                    p.z,
                    meshes,
                    chunks,
                    quads / 1000,
                    s.world.dirty_count(),
                    s.me.corrections,
                    s.selected + 1,
                    block,
                )
            }
            None => format!("Boundmore | {}", self.status),
        };
        w.set_title(&title);
    }
}

fn player_colour(id: u32) -> [f32; 4] {
    let h = id.wrapping_mul(2_654_435_761);
    let c = |shift: u32| 0.25 + 0.6 * ((h >> shift) & 0xff) as f32 / 255.0;
    [c(0), c(8), c(16), 1.0]
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
        self.window = Some(window);
        let c = &self.opts.connect;
        self.net = Some(net::spawn(ConnectOptions {
            server: c.server,
            name: c.name.clone(),
            cert: c.cert.clone(),
        }));
        event_loop.set_control_flow(ControlFlow::Poll);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
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
                self.set_grab(false);
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let PhysicalKey::Code(code) = event.physical_key else {
                    return;
                };
                if event.state == ElementState::Pressed {
                    self.keys.insert(code);
                    match code {
                        KeyCode::Escape => self.set_grab(false),
                        KeyCode::F11 => {
                            if let Some(w) = &self.window {
                                let full = w.fullscreen().is_some();
                                w.set_fullscreen((!full).then_some(Fullscreen::Borderless(None)));
                            }
                        }
                        _ => {}
                    }
                    let digit = match code {
                        KeyCode::Digit1 => Some(0),
                        KeyCode::Digit2 => Some(1),
                        KeyCode::Digit3 => Some(2),
                        KeyCode::Digit4 => Some(3),
                        KeyCode::Digit5 => Some(4),
                        KeyCode::Digit6 => Some(5),
                        KeyCode::Digit7 => Some(6),
                        KeyCode::Digit8 => Some(7),
                        KeyCode::Digit9 => Some(8),
                        _ => None,
                    };
                    if let (Some(d), Some(s)) = (digit, &mut self.session)
                        && d < s.hotbar.len()
                    {
                        s.selected = d;
                    }
                } else {
                    self.keys.remove(&code);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let down = state == ElementState::Pressed;
                if down && !self.grabbed {
                    self.set_grab(true);
                    return; // the click that grabs the mouse does not dig
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
                let steps = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y.signum() as i32,
                    MouseScrollDelta::PixelDelta(p) => p.y.signum() as i32,
                };
                if let Some(s) = &mut self.session
                    && !s.hotbar.is_empty()
                {
                    let n = s.hotbar.len() as i32;
                    s.selected = (s.selected as i32 - steps).rem_euclid(n) as usize;
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
