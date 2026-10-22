//! End to end: real server, real QUIC, headless players.

use bm_core::skills::{Character, xp_for_level};
use bm_core::{
    BlockId, BlockPos, BlockRegistry, BlockState, Chiselling, Chunk, ChunkPos, Colour, Inventory, ItemRegistry,
    ItemStack, PlayerInput, PlayerState, PlotPos, ShapeFamily, Station,
};
use bm_net::quinn::Connection;
use bm_net::{read_msg, write_msg};
use bm_protocol::{
    Channel, ChatLine, ClaimView, ClientDatagram, ClientMsg, CreatureView, EditAction, PROTOCOL_VERSION, PortalAction,
    PortalView, RemoteCreature, ServerDatagram, ServerMsg, ShotView, SkillAction, SlotRef, StationView, StorageView,
    Target, Welcome,
};
use bm_server::config::{PlanetSpec, ServerConfig};
use bm_server::store::{PlayerData, SavedBody, SavedStack, SavedStation, Store, UNIVERSE_FILE, WriteBatch};
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;
use tokio::sync::mpsc;

const PASSWORD: &str = "correct horse";
/// The hand stone's slot: every player starts with one there (a bare hand
/// breaks nothing), and `seed` puts one there unless told otherwise.
const HAND: u8 = 9;

struct TestClient {
    conn: Connection,
    send: bm_net::quinn::SendStream,
    msgs: mpsc::UnboundedReceiver<ServerMsg>,
    snaps: mpsc::UnboundedReceiver<ServerDatagram>,
    welcome: Welcome,
    items: ItemRegistry,
    inv: Inventory,
    notices: Vec<String>,
    /// Every chat line heard, in order.
    chat: Vec<ChatLine>,
    /// Storage views not yet looked at by `storage`.
    views: std::collections::VecDeque<StorageView>,
    /// The last reclaim heard of.
    reclaim: Option<Vec<ItemStack>>,
    /// The last claim heard of.
    claim: Option<ClaimView>,
    /// Portal token names heard, by tag.
    token_names: HashMap<u32, String>,
    chunks: HashMap<ChunkPos, Chunk>,
    seq: u32,
    /// The input the last swing followed, and its tool's ticks.
    last_swing: Option<(u32, u32)>,
    /// The planet the player is on now, and where they arrived on it.
    planet: bm_protocol::PlanetInfo,
    spawn: PlayerState,
    /// The character as last heard of (milestone 12).
    character: Character,
    /// Milestone 13: creatures as last told, shots of others heard, shots
    /// heard to end, hits heard, and the last defeat.
    creatures: HashMap<u32, CreatureView>,
    shots: Vec<ShotView>,
    shot_ends: Vec<u32>,
    hits: Vec<(Target, u32, bool)>,
    defeated: Option<(u64, String)>,
    /// Milestone 13 part 2: blasts heard (where, radius).
    blasts: Vec<([f32; 3], f32)>,
    /// Milestone 14: the planet's regions (layout seed, names, discovered),
    /// discoveries heard (region, nth, xp), meteors as last told, and the
    /// meteor life.
    regions: Option<(u64, Vec<String>, Vec<u16>)>,
    discovered: Vec<(u16, u32, u64)>,
    meteors: HashMap<u32, bm_protocol::MeteorView>,
    life: Option<(u32, Option<u32>)>,
}

/// Opens a connection and says hello; the welcome, or the refusal's reason.
async fn hello(
    server: &bm_server::RunningServer,
    name: &str,
    password: &str,
    register: bool,
) -> Result<TestClient, String> {
    let endpoint = bm_net::client_endpoint(&server.cert).unwrap();
    let addr = format!("127.0.0.1:{}", server.addr.port()).parse().unwrap();
    let conn = bm_net::connect(&endpoint, addr).await.unwrap();
    let (mut send, mut recv) = conn.open_bi().await.unwrap();
    write_msg(
        &mut send,
        &ClientMsg::Hello {
            protocol: PROTOCOL_VERSION,
            name: name.into(),
            password: password.into(),
            register,
        },
    )
    .await
    .unwrap();
    let welcome = match read_msg::<ServerMsg>(&mut recv).await {
        Ok(Some(ServerMsg::Welcome(w))) => *w,
        Ok(Some(ServerMsg::Refused { reason })) => return Err(reason),
        other => panic!("expected welcome, got {other:?}"),
    };
    let blocks = BlockRegistry::from_defs(welcome.blocks.clone()).unwrap();
    let items = ItemRegistry::from_defs(welcome.items.clone(), &blocks).unwrap();
    let (mtx, msgs) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        // Keep the endpoint alive as long as the stream is read.
        let _endpoint = endpoint;
        while let Ok(Some(m)) = read_msg::<ServerMsg>(&mut recv).await {
            if mtx.send(m).is_err() {
                break;
            }
        }
    });
    let (stx, snaps) = mpsc::unbounded_channel();
    let dconn = conn.clone();
    tokio::spawn(async move {
        while let Ok(b) = dconn.read_datagram().await {
            if stx.send(bm_protocol::decode::<ServerDatagram>(&b).unwrap()).is_err() {
                break;
            }
        }
    });
    Ok(TestClient {
        conn,
        send,
        msgs,
        snaps,
        inv: welcome.inventory.clone(),
        planet: welcome.planet.clone(),
        spawn: welcome.spawn,
        character: welcome.character.clone(),
        welcome,
        items,
        notices: Vec::new(),
        chat: Vec::new(),
        views: Default::default(),
        reclaim: None,
        claim: None,
        token_names: HashMap::new(),
        chunks: HashMap::new(),
        seq: 0,
        last_swing: None,
        creatures: HashMap::new(),
        shots: Vec::new(),
        shot_ends: Vec::new(),
        hits: Vec::new(),
        defeated: None,
        blasts: Vec::new(),
        regions: None,
        discovered: Vec::new(),
        meteors: HashMap::new(),
        life: None,
    })
}

/// What one swing did to the block it hit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Swung {
    /// It stands, with this much damage taken.
    Damaged(u32),
    Broke,
    /// The swing was refused: the block there, and its true damage.
    Refused(BlockId, u32),
}

impl TestClient {
    async fn register(server: &bm_server::RunningServer, name: &str) -> Self {
        hello(server, name, PASSWORD, true).await.expect("registered")
    }

    async fn login(server: &bm_server::RunningServer, name: &str) -> Self {
        hello(server, name, PASSWORD, false).await.expect("logged in")
    }

    fn item(&self, name: &str) -> bm_core::ItemId {
        self.items.id(name).unwrap()
    }

    fn block(&self, name: &str) -> BlockId {
        BlockId(self.welcome.blocks.iter().position(|b| b.name == name).unwrap() as u16)
    }

    fn recipe(&self, output: &str) -> u16 {
        let id = self.item(output);
        self.welcome.recipes.iter().position(|r| r.output == id).unwrap() as u16
    }

    fn count(&self, name: &str) -> u32 {
        self.inv.count(self.item(name))
    }

    /// The block under the player's feet where they arrived.
    fn feet(&self) -> BlockPos {
        let s = self.spawn.pos;
        BlockPos::containing(s.x, s.y - 0.5, s.z)
    }

    /// The warp shrine they arrived beside (the server puts arrivals two
    /// blocks along +X from it).
    fn shrine(&self) -> BlockPos {
        self.feet().offset(-2, 1, 0)
    }

    /// Reads reliable messages until `pred` matches one (chunks, block
    /// changes, slots and notices are applied as they pass).
    async fn until(&mut self, mut pred: impl FnMut(&ServerMsg) -> bool) -> ServerMsg {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
        loop {
            let m = tokio::time::timeout_at(deadline, self.msgs.recv())
                .await
                .expect("timed out waiting for a message")
                .expect("stream closed");
            self.apply(&m);
            if pred(&m) {
                return m;
            }
        }
    }

    async fn notice(&mut self, text: &str) {
        if !self.notices.iter().any(|n| n == text) {
            self.until(|m| matches!(m, ServerMsg::Notice { text: t } if t == text))
                .await;
        }
    }

    /// Keeps the local copies (chunks, inventory, notices) up to date.
    fn apply(&mut self, m: &ServerMsg) {
        match m {
            ServerMsg::Chunk { pos, data } => {
                self.chunks.insert(*pos, Chunk::decode(data).expect("valid chunk"));
            }
            // An edit result carries what the block now is, as a change would.
            ServerMsg::BlockChanged { pos, block } | ServerMsg::EditResult { pos, block, .. } => {
                let pos = self.shape().wrap_block(*pos);
                if let Some(c) = self.chunks.get_mut(&pos.chunk()) {
                    c.set_state(pos.local(), *block);
                }
            }
            ServerMsg::Slots { changes } => {
                for (slot, s) in changes {
                    self.inv.slots[*slot as usize] = *s;
                }
            }
            ServerMsg::Notice { text } => self.notices.push(text.clone()),
            ServerMsg::Chat { lines } => self.chat.extend(lines.iter().cloned()),
            ServerMsg::Storage(v) => self.views.push_back(v.clone()),
            ServerMsg::Reclaim { items } => self.reclaim = Some(items.clone()),
            ServerMsg::Claim(c) => self.claim = Some(c.clone()),
            ServerMsg::TokenNames { names } => self.token_names.extend(names.iter().cloned()),
            ServerMsg::Character { character, .. } => self.character = character.clone(),
            ServerMsg::Creature(v) => {
                self.creatures.insert(v.id, v.clone());
            }
            ServerMsg::CreatureGone { id } => {
                self.creatures.remove(id);
            }
            ServerMsg::Shot(v) => self.shots.push(*v),
            ServerMsg::ShotEnd { id } => self.shot_ends.push(*id),
            ServerMsg::Hit { target, amount, crit } => self.hits.push((*target, *amount, *crit)),
            ServerMsg::Defeated { debt, by } => self.defeated = Some((*debt, by.clone())),
            ServerMsg::Blast { pos, radius } => self.blasts.push((*pos, *radius)),
            ServerMsg::Regions {
                layout_seed,
                names,
                discovered,
            } => self.regions = Some((*layout_seed, names.clone(), discovered.clone())),
            ServerMsg::Discovered { region, nth, xp } => self.discovered.push((*region, *nth, *xp)),
            ServerMsg::Meteor(v) => {
                self.meteors.insert(v.id, *v);
            }
            ServerMsg::MeteorGone { id } => {
                self.meteors.remove(id);
            }
            ServerMsg::MeteorLife { life } => self.life = *life,
            // Everything of the old planet goes, as in the game.
            ServerMsg::Travel(t) => {
                self.chunks.clear();
                self.planet = t.planet.clone();
                self.spawn = t.spawn;
            }
            _ => {}
        }
    }

    fn shape(&self) -> bm_core::PlanetShape {
        self.planet.shape
    }

    /// Lets everything already sent arrive.
    async fn settle(&mut self) {
        tokio::time::sleep(Duration::from_millis(300)).await;
        while let Ok(m) = self.msgs.try_recv() {
            self.apply(&m);
        }
    }

    async fn wait_for_chunk(&mut self, p: ChunkPos) {
        if self.chunks.contains_key(&p) {
            return;
        }
        self.until(|m| matches!(m, ServerMsg::Chunk { pos, .. } if *pos == p))
            .await;
    }

    fn send_inputs(&mut self, n: u32, input: PlayerInput) {
        for _ in 0..n {
            self.seq += 1;
            bm_net::send_datagram(
                &self.conn,
                &ClientDatagram::Input {
                    latest: self.seq,
                    inputs: vec![input; 4],
                },
            );
        }
    }

    async fn latest_snapshot(&mut self) -> (u64, u32, PlayerState, Vec<bm_protocol::RemotePlayer>) {
        let ServerDatagram::Snapshot {
            planet: _,
            mut tick,
            mut ack,
            mut you,
            mut others,
            ..
        } = tokio::time::timeout(Duration::from_secs(5), self.snaps.recv())
            .await
            .unwrap()
            .unwrap();
        while let Ok(ServerDatagram::Snapshot {
            planet: _,
            tick: t,
            ack: a,
            you: y,
            others: o,
            ..
        }) = self.snaps.try_recv()
        {
            (tick, ack, you, others) = (t, a, y, o);
        }
        (tick, ack, you, others)
    }

    async fn send(&mut self, msg: ClientMsg) {
        write_msg(&mut self.send, &msg).await.unwrap();
    }

    /// Ticks the item in hotbar `slot` needs between swings.
    fn swing_ticks(&self, slot: u8) -> u32 {
        let item = self.inv.get(slot as usize).map(|s| s.item);
        // No tool: the swing is refused anyway; any cadence will do.
        self.items
            .swing_tool(item)
            .map_or(1, |t| bm_core::mining::swing_ticks(t.action_speed))
    }

    /// One swing at `pos` with hotbar `slot`, following the last input sent,
    /// and what the server said of the block it hit.
    async fn swing(&mut self, seq: u32, pos: BlockPos, slot: u8) -> Swung {
        let at = self.seq;
        self.last_swing = Some((at, self.swing_ticks(slot)));
        self.send(ClientMsg::Swing { seq, at, pos, slot }).await;
        let shape = self.shape();
        let canon = shape.wrap_block(pos);
        let m = self
            .until(|m| match m {
                ServerMsg::EditResult { seq: s, pos: p, .. } => *s == seq && shape.wrap_block(*p) == canon,
                ServerMsg::BlockHealth { pos: p, .. } => *p == canon,
                _ => false,
            })
            .await;
        match m {
            ServerMsg::EditResult { accepted: true, .. } => Swung::Broke,
            ServerMsg::EditResult { block, .. } => {
                // The block's true damage follows a refusal.
                match self
                    .until(|m| matches!(m, ServerMsg::BlockHealth { pos: p, .. } if *p == canon))
                    .await
                {
                    ServerMsg::BlockHealth { damage, .. } => Swung::Refused(block.id, damage),
                    _ => unreachable!(),
                }
            }
            ServerMsg::BlockHealth { damage, .. } => Swung::Damaged(damage),
            _ => unreachable!(),
        }
    }

    /// Sends idle inputs until the last swing's tool allows the next, as
    /// the game's swing loop waits.
    fn ready(&mut self) {
        if let Some((at, ticks)) = self.last_swing {
            let next = at + ticks;
            if self.seq < next {
                self.send_inputs(next - self.seq, PlayerInput::default());
            }
        }
    }

    /// Swings hotbar `slot` at `pos` at the tool's cadence until it breaks
    /// (true) or a swing is refused (false); the block the server says is
    /// there.
    async fn dig_with(&mut self, seq: u32, pos: BlockPos, slot: u8) -> (bool, BlockId) {
        for _ in 0..200 {
            self.ready();
            match self.swing(seq, pos, slot).await {
                Swung::Broke => {
                    self.settle().await;
                    return (true, BlockId::AIR);
                }
                Swung::Refused(block, _) => {
                    self.settle().await;
                    return (false, block);
                }
                Swung::Damaged(_) => {}
            }
        }
        panic!("200 swings did not break {pos:?}");
    }

    /// Digs `pos` by hand.
    async fn dig(&mut self, seq: u32, pos: BlockPos) -> (bool, BlockId) {
        self.dig_with(seq, pos, HAND).await
    }

    async fn edit(&mut self, seq: u32, pos: BlockPos, action: EditAction) -> (bool, BlockId) {
        let at = self.seq;
        self.send(ClientMsg::Edit { seq, at, pos, action }).await;
        match self
            .until(|m| matches!(m, ServerMsg::EditResult { seq: s, .. } if *s == seq))
            .await
        {
            ServerMsg::EditResult { accepted, block, .. } => {
                // Slots and notices for the edit follow its result.
                self.settle().await;
                (accepted, block.id)
            }
            _ => unreachable!(),
        }
    }

    /// The next storage view matching `pred` (passing over earlier ones).
    async fn storage(&mut self, mut pred: impl FnMut(&StorageView) -> bool) -> StorageView {
        loop {
            while let Some(v) = self.views.pop_front() {
                if pred(&v) {
                    return v;
                }
            }
            self.until(|m| matches!(m, ServerMsg::Storage(_))).await;
        }
    }

    /// Waits until a chat line matching `pred` has been heard.
    async fn heard(&mut self, mut pred: impl FnMut(&ChatLine) -> bool) -> ChatLine {
        if let Some(l) = self.chat.iter().find(|l| pred(l)) {
            return l.clone();
        }
        loop {
            if let ServerMsg::Chat { lines } = self.until(|m| matches!(m, ServerMsg::Chat { .. })).await
                && let Some(l) = lines.iter().find(|l| pred(l))
            {
                return l.clone();
            }
        }
    }

    fn has_heard(&self, text: &str) -> bool {
        self.chat.iter().any(|l| l.text == text)
    }

    async fn say(&mut self, channel: Channel, text: &str) {
        self.send(ClientMsg::Chat {
            channel,
            text: text.into(),
        })
        .await;
    }

    /// A skill action following the last input sent, and the character the
    /// server answered with (after a refusal, as it was).
    async fn skill(&mut self, action: SkillAction) -> Character {
        if self.seq == 0 {
            // `at` 0 is how XP updates are tagged; follow a real input.
            self.send_inputs(1, PlayerInput::default());
        }
        let at = self.seq;
        self.send(ClientMsg::Skill { at, action }).await;
        match self
            .until(|m| matches!(m, ServerMsg::Character { at: a, .. } if *a == at))
            .await
        {
            ServerMsg::Character { character, .. } => character,
            _ => unreachable!(),
        }
    }

    /// `n` ranks of skill `id`, each accepted.
    async fn learn(&mut self, id: &str, n: u32) {
        for _ in 0..n {
            let before = self.character.clone();
            let after = self.skill(SkillAction::Learn(id.into())).await;
            assert_ne!(after, before, "learning {id}: {:?}", self.notices.last());
        }
    }

    /// A skill action the server refuses with `text`, changing nothing.
    async fn skill_refused(&mut self, action: SkillAction, text: &str) {
        self.notices.clear();
        let before = self.character.clone();
        let after = self.skill(action).await;
        assert_eq!(after, before, "refused: nothing changes");
        assert_eq!(self.notices, vec![text.to_string()]);
    }

    /// Waits until the character has `xp` in all.
    async fn until_xp(&mut self, xp: u64) {
        if self.character.xp != xp {
            self.until(|m| matches!(m, ServerMsg::Character { character, .. } if character.xp == xp))
                .await;
        }
    }

    fn mine_xp(&self, block: BlockId) -> u64 {
        self.welcome.blocks[block.0 as usize].mine_xp as u64
    }

    fn build_xp(&self, block: &str) -> u64 {
        self.welcome.blocks[self.block(block).0 as usize].build_xp as u64
    }

    async fn station(&mut self, mut pred: impl FnMut(&StationView) -> bool) -> StationView {
        match self.until(|m| matches!(m, ServerMsg::Station(v) if pred(v))).await {
            ServerMsg::Station(v) => v,
            _ => unreachable!(),
        }
    }
}

/// One small planet. These tests dig where they stand, beside the shrine, so
/// its guard is off (the shrine tests use the default).
fn config(dir: &Path) -> ServerConfig {
    ServerConfig {
        bind: "127.0.0.1:0".parse().unwrap(),
        data_dir: dir.to_path_buf(),
        planets: vec![PlanetSpec::new("Testworld", "temperate", 7, 16)],
        height_chunks: 12,
        view_radius: 3,
        shrine_guard: 0,
        // The game's reach is Boundless's untrained 4 (Nathan, 2026-10-20);
        // these layouts were built for 7 and the reach rule has its own
        // checks, so the test worlds keep 7.
        reach: 7.0,
        // Portal tests open windows with untrained characters; the epic's
        // rule has its own test.
        portal_epic_required: false,
        // Tests count XP exactly; discovery has its own test.
        discovery_xp: false,
        ..Default::default()
    }
}

/// Two small planets, with the shrine guard on.
fn universe(dir: &Path) -> ServerConfig {
    ServerConfig {
        planets: vec![
            PlanetSpec::new("Testworld", "temperate", 7, 16),
            PlanetSpec::new("Dryside", "arid", 8, 16),
        ],
        shrine_guard: 4,
        ..config(dir)
    }
}

fn planet_file(dir: &Path) -> std::path::PathBuf {
    dir.join(UNIVERSE_FILE)
}

/// What save version 2 called the planet's file.
fn old_planet_file(dir: &Path) -> std::path::PathBuf {
    dir.join("testworld.redb")
}

/// Registers an account before the server starts, holding these stacks
/// (slot, item, count, wear).
fn seed(dir: &Path, name: &str, stacks: &[(usize, &str, u16, u16)]) {
    seed_character(dir, name, stacks, Character::default());
}

/// `seed`, with this character's progression.
fn seed_character(dir: &Path, name: &str, stacks: &[(usize, &str, u16, u16)], character: Character) {
    seed_hurt(dir, name, stacks, character, 0);
}

/// `seed_character`, with this much health lost.
fn seed_hurt(dir: &Path, name: &str, stacks: &[(usize, &str, u16, u16)], character: Character, hurt: u32) {
    let store = Store::open(&planet_file(dir)).unwrap();
    let account = bm_server::accounts::register(&store, name, PASSWORD).unwrap();
    let mut inventory = vec![None; bm_core::INVENTORY_SLOTS];
    inventory[HAND as usize] = Some(SavedStack {
        item: "hand_stone".into(),
        count: 1,
        wear: 0,
    });
    for &(slot, item, count, wear) in stacks {
        inventory[slot] = Some(SavedStack {
            item: item.into(),
            count,
            wear,
        });
    }
    store
        .write(&WriteBatch {
            players: vec![(
                account.key,
                PlayerData {
                    planet: 0,
                    state: None,
                    inventory,
                    last_seen: 0,
                    character: character.into(),
                    hurt,
                },
            )],
            ..Default::default()
        })
        .unwrap();
}

/// A new player's pack: the hand stone in its slot and nothing else (a bare
/// hand breaks nothing).
fn only_the_hand_stone(c: &TestClient) -> bool {
    c.welcome.inventory.slots.iter().enumerate().all(|(i, s)| match s {
        Some(s) => i == HAND as usize && s.item == c.item("hand_stone"),
        None => i != HAND as usize,
    })
}

fn stack(c: &TestClient, slot: usize) -> Option<ItemStack> {
    c.inv.get(slot)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_players_share_one_world() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path(), "Alice", &[(1, "stone", 10, 0)]);
    let server = bm_server::start(config(dir.path())).unwrap();

    let mut alice = TestClient::login(&server, "Alice").await;
    assert!(alice.welcome.blocks.len() > 10, "catalogue arrives in the welcome");
    assert_eq!(alice.welcome.account, "Alice");
    assert_eq!(alice.count("stone"), 10, "inventory arrives in the welcome");
    let feet = alice.feet();
    alice.wait_for_chunk(feet.chunk()).await;
    let ground = alice.chunks[&feet.chunk()].get(feet.local());
    assert_ne!(ground, BlockId::AIR, "spawn stands on something");
    let gathered = alice.items.drop_of(ground).expect("the ground yields something");

    let mut bob = TestClient::register(&server, "Bob").await;
    bob.wait_for_chunk(feet.chunk()).await;
    let joined = alice.until(|m| matches!(m, ServerMsg::PlayerJoined { .. })).await;
    assert!(matches!(joined, ServerMsg::PlayerJoined { ref name, .. } if name == "Bob"));

    // A swing lands at once; another before the hand's cadence is refused,
    // with the block's true damage.
    assert_eq!(
        alice.swing(90, feet, HAND).await,
        Swung::Damaged(200),
        "the hand stone deals 200"
    );
    assert_eq!(
        alice.swing(91, feet, HAND).await,
        Swung::Refused(ground, 200),
        "a swing sooner than the cadence"
    );
    // A bare hand breaks nothing (Nathan, 2026-10-20: as in Boundless).
    alice.ready();
    assert_eq!(
        alice.swing(92, feet, 2).await,
        Swung::Refused(ground, 200),
        "an empty slot is a bare hand"
    );
    alice
        .notice("You need a tool to break blocks. A hand stone needs nothing to craft")
        .await;
    // Rules: bedrock cannot be broken; far blocks cannot be reached.
    alice.ready();
    let floor = BlockPos::new(feet.x, 0, feet.z);
    assert!(
        matches!(alice.swing(2, floor, HAND).await, Swung::Refused(..)),
        "out of reach / unbreakable"
    );
    let far = BlockPos::new(feet.x + 40, feet.y, feet.z);
    assert!(
        matches!(alice.swing(3, far, HAND).await, Swung::Refused(..)),
        "out of reach"
    );

    // Alice digs the block she stands on; it goes into her inventory, and
    // Bob sees it go.
    let (ok, block) = alice.dig(1, feet).await;
    assert!(ok, "break accepted");
    assert_eq!(block, BlockId::AIR);
    assert_eq!(alice.inv.count(gathered), 1, "the block was gathered");
    let gathered_slot = (0..10)
        .find(|&i| stack(&alice, i).is_some_and(|s| s.item == gathered))
        .unwrap();
    bob.until(|m| matches!(m, ServerMsg::BlockChanged { pos, .. } if *pos == feet))
        .await;
    assert_eq!(bob.chunks[&feet.chunk()].get(feet.local()), BlockId::AIR);

    // An empty slot places nothing.
    assert!(
        !alice
            .edit(
                6,
                feet,
                EditAction::Place {
                    slot: HAND,
                    mount: None
                }
            )
            .await
            .0
    );
    // Place it back, spending what was gathered. No input has followed the
    // last swing, so nobody has fallen: the hole is empty and can be filled.
    let (ok, placed) = alice
        .edit(
            4,
            feet,
            EditAction::Place {
                slot: gathered_slot as u8,
                mount: None,
            },
        )
        .await;
    assert!(ok, "refill accepted");
    assert_eq!(alice.items.item_for_block(placed), Some(gathered));
    assert_eq!(alice.inv.count(gathered), 0, "placing spends");
    // But not where the players stand.
    let body = feet.offset(0, 1, 0);
    assert!(
        !alice.edit(5, body, EditAction::Place { slot: 1, mount: None }).await.0,
        "placing into a player is refused"
    );
    assert_eq!(alice.count("stone"), 10, "a refused place spends nothing");

    // Moving and throwing away items.
    alice
        .send(ClientMsg::MoveItems {
            from: 1,
            to: 20,
            count: 4,
        })
        .await;
    alice.settle().await;
    assert_eq!(
        (stack(&alice, 1).unwrap().count, stack(&alice, 20).unwrap().count),
        (6, 4)
    );
    alice.send(ClientMsg::Discard { slot: 20 }).await;
    alice.settle().await;
    assert_eq!(alice.count("stone"), 6);

    // Movement reaches Bob through snapshots.
    let (_, _, before, _) = alice.latest_snapshot().await;
    let walk = PlayerInput {
        forward: 1,
        jump: true,
        ..Default::default()
    };
    for _ in 0..40 {
        alice.send_inputs(1, walk);
        tokio::time::sleep(Duration::from_millis(16)).await;
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    let (_, ack, after, _) = alice.latest_snapshot().await;
    assert!(ack >= 40, "server applied the inputs (ack {ack})");
    let moved = (after.pos - before.pos).length();
    assert!(moved > 0.5, "alice moved {moved}");
    let (_, _, _, others) = bob.latest_snapshot().await;
    let seen = others
        .iter()
        .find(|o| o.id == alice.welcome.player_id)
        .expect("bob sees alice");
    assert!((seen.pos[2] as f64 - after.pos.z).abs() < 0.5);

    // A client cannot move faster than real time by flooding inputs.
    let (_, ack0, _, _) = alice.latest_snapshot().await;
    alice.send_inputs(600, walk);
    tokio::time::sleep(Duration::from_millis(200)).await;
    let (_, ack1, _, _) = alice.latest_snapshot().await;
    assert!(ack1 - ack0 < 40, "applied {} inputs in 200 ms", ack1 - ack0);

    // Logging in again replaces the old session, inventory and all.
    let alice2 = TestClient::login(&server, "alice").await;
    assert_ne!(alice2.welcome.player_id, alice.welcome.player_id);
    assert_eq!(alice2.welcome.account, "Alice", "the registered spelling");
    assert_eq!(alice2.count("stone"), 6);
    bob.until(|m| matches!(m, ServerMsg::PlayerLeft { id } if *id == alice.welcome.player_id))
        .await;
    drop(alice);
    drop(bob);
    server.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn accounts_register_log_in_and_refuse() {
    let dir = tempfile::tempdir().unwrap();
    let server = bm_server::start(config(dir.path())).unwrap();
    let alice = TestClient::register(&server, "Alice").await;
    assert_eq!(alice.welcome.account, "Alice");
    drop(alice);

    let taken = hello(&server, "ALICE", "another password", true).await.err();
    assert_eq!(taken.as_deref(), Some("that name is taken"));
    let short = hello(&server, "Carol", "short", true).await.err();
    assert!(short.unwrap().contains("password"), "too short a password is refused");
    let wrong = hello(&server, "Alice", "not her password", false).await.err();
    assert_eq!(wrong.as_deref(), Some("wrong name or password"));
    let nobody = hello(&server, "Nobody", PASSWORD, false).await.err();
    assert_eq!(
        nobody.as_deref(),
        Some("wrong name or password"),
        "no hint that the name is free"
    );

    let back = TestClient::login(&server, "alice").await;
    assert_eq!(back.welcome.account, "Alice");
    drop(back);

    // Five failures from one address lock it out for a while, even with
    // the right password (two have failed above).
    for _ in 0..3 {
        assert_eq!(
            hello(&server, "Alice", "guessing again", false).await.err().as_deref(),
            Some("wrong name or password")
        );
    }
    let locked = hello(&server, "Alice", PASSWORD, false).await.err().unwrap();
    assert!(locked.contains("too many"), "{locked}");
    server.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn edits_positions_and_inventories_survive_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let (feet, moved_to) = {
        let server = bm_server::start(config(dir.path())).unwrap();
        let mut alice = TestClient::register(&server, "Alice").await;
        let feet = alice.feet();
        alice.wait_for_chunk(feet.chunk()).await;
        assert!(alice.dig(1, feet).await.0);
        tokio::time::sleep(Duration::from_millis(500)).await;
        let (_, _, state, _) = alice.latest_snapshot().await;
        drop(alice);
        server.stop();
        (feet, state.pos)
    };
    let server = bm_server::start(config(dir.path())).unwrap();
    let mut alice = TestClient::login(&server, "Alice").await;
    assert!(
        (alice.welcome.spawn.pos - moved_to).length() < 1.0,
        "rejoined where she was ({:?} vs {:?})",
        alice.welcome.spawn.pos,
        moved_to
    );
    assert_eq!(
        alice
            .inv
            .slots
            .iter()
            .enumerate()
            .filter(|&(i, _)| i != HAND as usize)
            .filter_map(|(_, s)| s.as_ref())
            .map(|s| s.count)
            .sum::<u16>(),
        1,
        "kept what she dug"
    );
    alice.wait_for_chunk(feet.chunk()).await;
    assert_eq!(
        alice.chunks[&feet.chunk()].get(feet.local()),
        BlockId::AIR,
        "the hole is still there"
    );
    server.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn full_inventories_refuse_breaks_and_tools_hit_harder_and_wear_out() {
    let dir = tempfile::tempdir().unwrap();
    seed(
        dir.path(),
        "Alice",
        &[
            (0, "stone_hammer", 1, 0),
            (1, "stone", 10, 0),
            (3, "stone_hammer", 1, 597),
        ],
    );
    // Every slot but the hand stone's full.
    let full: Vec<(usize, &str, u16, u16)> = (0..40)
        .filter(|&i| i != HAND as usize)
        .map(|i| (i, "copper_ingot", 100, 0))
        .collect();
    seed(dir.path(), "Bob", &full);
    let server = bm_server::start(config(dir.path())).unwrap();

    // Bob's pack is full: the block stays, and he is told.
    let mut bob = TestClient::login(&server, "Bob").await;
    let feet = bob.feet();
    bob.wait_for_chunk(feet.chunk()).await;
    let ground = bob.chunks[&feet.chunk()].get(feet.local());
    let (ok, block) = bob.dig(1, feet).await;
    assert!(!ok, "break refused with a full inventory");
    assert_eq!(block, ground, "nothing destroyed");
    bob.notice("Your inventory is full").await;
    // It stands one point short of breaking: making room, one more swing
    // takes it.
    bob.send(ClientMsg::Discard { slot: 0 }).await;
    bob.ready();
    assert_eq!(bob.swing(2, feet, HAND).await, Swung::Broke);
    drop(bob);

    let mut alice = TestClient::login(&server, "Alice").await;
    alice.wait_for_chunk(feet.chunk()).await;
    let stone = alice.block("stone");
    assert!(alice.edit(10, feet, EditAction::Place { slot: 1, mount: None }).await.0);
    // The hand stone deals 200 to stone, the stone hammer 400 (tuning
    // 2.1): one swing with the hand stone and three with the hammer break
    // its 1200.
    assert_eq!(alice.swing(11, feet, HAND).await, Swung::Damaged(200));
    alice.ready();
    assert_eq!(
        alice.swing(12, feet, 0).await,
        Swung::Damaged(600),
        "the hammer hits harder"
    );
    alice.ready();
    assert_eq!(alice.swing(13, feet, 0).await, Swung::Damaged(1000));
    alice.ready();
    assert_eq!(alice.swing(14, feet, 0).await, Swung::Broke);
    alice.settle().await;
    assert_eq!(alice.count("stone"), 10, "the stone is gathered");
    assert_eq!(stack(&alice, 0).unwrap().wear, 3, "every swing wears the tool");
    assert_ne!(stone, BlockId::AIR);

    // A hammer three uses from its end breaks with the stone, on its third
    // swing.
    assert!(alice.edit(15, feet, EditAction::Place { slot: 1, mount: None }).await.0);
    assert!(alice.dig_with(16, feet, 3).await.0);
    alice.notice("Your Stone Hammer broke").await;
    assert_eq!(stack(&alice, 3), None, "the broken tool is gone");
    server.stop();
}

/// Asks for a station's view again and returns the next one matching `pred`.
async fn look_at_station(c: &mut TestClient, pos: BlockPos, pred: impl FnMut(&StationView) -> bool) -> StationView {
    c.send(ClientMsg::WatchStation { pos: Some(pos) }).await;
    c.station(pred).await
}

async fn craft_at(c: &mut TestClient, recipe: u16, count: u16, station: Option<BlockPos>) {
    c.send(ClientMsg::Craft { recipe, count, station }).await;
}

async fn fuel(c: &mut TestClient, pos: BlockPos, slot: u8, count: u16, into: bool) {
    c.send(ClientMsg::StationFuel { pos, slot, count, into }).await;
}

/// Milestone 11, acceptance 3 and 4: hand recipes anywhere, the crafting
/// table crafting over time, and a workbench that works only as 4 blocks
/// touching, whose queue and output belong to the whole group.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn crafting_by_hand_at_a_table_and_at_a_four_block_workbench() {
    let dir = tempfile::tempdir().unwrap();
    seed(
        dir.path(),
        "Alice",
        &[
            (0, "workbench", 5, 0),
            (5, "trunk", 6, 0),
            (6, "stone", 1, 0),
            (10, "stones", 6, 0),
            (11, "glow_crystal", 1, 0),
            (12, "copper_ingot", 3, 0),
            (13, "relic_fragment", 1, 0),
        ],
    );
    let server = bm_server::start(config(dir.path())).unwrap();
    let mut alice = TestClient::login(&server, "Alice").await;
    let shape = alice.shape();
    let feet = alice.feet();
    for dx in -1..=4 {
        for dz in -1..=4 {
            alice.wait_for_chunk(feet.offset(dx, 1, dz).chunk()).await;
        }
    }

    // Hand recipes are instant and all or nothing; 2 timber a trunk.
    let timber = alice.recipe("timber");
    craft_at(&mut alice, timber, 7, None).await;
    alice.notice("You do not have the materials").await;
    assert_eq!(alice.count("trunk"), 6);
    craft_at(&mut alice, timber, 5, None).await;
    alice.settle().await;
    assert_eq!((alice.count("trunk"), alice.count("timber")), (1, 10));
    let r = alice.recipe("crafting_table");
    craft_at(&mut alice, r, 1, None).await;
    alice.settle().await;
    assert_eq!((alice.count("crafting_table"), alice.count("timber")), (1, 0));

    // A table recipe needs a table.
    let stones = alice.recipe("stones");
    craft_at(&mut alice, stones, 1, None).await;
    alice.notice("That is made at a Crafting Table").await;

    let table = shape.wrap_block(feet.offset(1, 1, 0));
    let slot = (0..10)
        .find(|&i| stack(&alice, i).is_some_and(|s| s.item == alice.item("crafting_table")))
        .unwrap() as u8;
    assert!(place(&mut alice, 1, table, slot, None).await.0);
    let empty = look_at_station(&mut alice, table, |_| true).await;
    assert_eq!(
        (empty.pos, empty.station, empty.idle.clone()),
        (table, Station::CraftingTable, None)
    );
    assert!(empty.jobs.is_empty() && empty.output.is_empty());
    // Hand recipes work at a table too, still instantly.
    craft_at(&mut alice, timber, 1, Some(table)).await;
    alice.settle().await;
    assert_eq!((alice.count("trunk"), alice.count("timber")), (0, 2));
    // A workbench recipe is not made at a table.
    let r = alice.recipe("claim_stone");
    craft_at(&mut alice, r, 1, Some(table)).await;
    alice.notice("That is made at a Workbench").await;
    // Stones from rock, over time; the table cannot be broken meanwhile.
    craft_at(&mut alice, stones, 1, Some(table)).await;
    let queued = alice.station(|v| !v.jobs.is_empty()).await;
    assert_eq!(queued.jobs[0].recipe, stones);
    alice.settle().await;
    assert_eq!(alice.count("stone"), 0, "inputs taken at once");
    alice
        .station(|v| v.jobs.first().is_some_and(|j| j.progress > 0.05))
        .await;
    assert!(!alice.dig(2, table).await.0, "a working station cannot be broken");
    alice.notice("Empty the Crafting Table before breaking it").await;
    let done = look_at_station(&mut alice, table, |v| !v.output.is_empty()).await;
    assert_eq!((done.output[0].item, done.output[0].count), (alice.item("stones"), 2));
    alice.send(ClientMsg::CollectStation { pos: table }).await;
    alice.station(|v| v.output.is_empty()).await;
    alice.settle().await;
    assert_eq!(alice.count("stones"), 8);
    // Three workbench blocks do nothing.
    let row = [feet.offset(1, 1, 2), feet.offset(2, 1, 2), feet.offset(3, 1, 2)].map(|p| shape.wrap_block(p));
    for (i, p) in row.iter().enumerate() {
        assert!(place(&mut alice, 10 + i as u32, *p, 0, None).await.0);
    }
    let idle = look_at_station(&mut alice, row[1], |_| true).await;
    assert_eq!(idle.idle.as_deref(), Some("A workbench needs 4 blocks touching"));
    assert_eq!(idle.station, Station::Workbench);
    let claim_stone = alice.recipe("claim_stone");
    craft_at(&mut alice, claim_stone, 1, Some(row[1])).await;
    alice.notice("A workbench needs 4 blocks touching").await;
    assert_eq!(alice.count("glow_crystal"), 1, "nothing taken");

    // The fourth, touching the end of the row by a face: it works, and the
    // view (pushed to her as it formed) names its first block.
    let fourth = shape.wrap_block(feet.offset(3, 1, 3));
    assert!(place(&mut alice, 13, fourth, 0, None).await.0);
    let first = *[row[0], row[1], row[2], fourth]
        .iter()
        .min_by_key(|p| (p.y, p.z, p.x))
        .unwrap();
    let working = look_at_station(&mut alice, row[1], |_| true).await;
    assert_eq!(working.idle, None);
    assert_eq!(working.pos, first);
    // A fifth touching is refused.
    assert!(
        !place(&mut alice, 14, shape.wrap_block(feet.offset(4, 1, 2)), 0, None)
            .await
            .0
    );
    alice.notice("Only 4 Workbench blocks can touch").await;

    // Crafting at any of its blocks queues at the machine.
    craft_at(&mut alice, claim_stone, 1, Some(fourth)).await;
    let queued = alice.station(|v| !v.jobs.is_empty()).await;
    assert_eq!((queued.pos, queued.jobs[0].recipe), (first, claim_stone));
    for (seq, p) in [(15, row[1]), (16, fourth)] {
        assert!(!alice.dig(seq, p).await.0, "no block of it breaks while it works");
        alice.notice("Empty the Workbench before breaking it").await;
    }
    let done = look_at_station(&mut alice, row[2], |v| !v.output.is_empty()).await;
    assert_eq!((done.pos, done.output[0].item), (first, alice.item("claim_stone")));
    alice.send(ClientMsg::CollectStation { pos: row[1] }).await;
    alice.station(|v| v.output.is_empty()).await;
    alice.settle().await;
    assert_eq!(alice.count("claim_stone"), 1, "collected from another block");

    // Empty, a block breaks, and what is left says it needs 4 again.
    assert!(alice.dig(17, fourth).await.0, "an empty machine breaks");
    let broken = look_at_station(&mut alice, row[1], |_| true).await;
    assert_eq!(broken.idle.as_deref(), Some("A workbench needs 4 blocks touching"));
    assert_eq!(alice.count("workbench"), 2);
    server.stop();
}

/// Milestone 11, acceptance 5: a furnace is a crucible on a base; a craft
/// starts when there is heat for it, burning fuel from its slot; without,
/// it waits; fuel moves in and out; a furnace with fuel cannot be broken.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_furnace_burns_fuel_for_heat_and_waits_without_it() {
    let dir = tempfile::tempdir().unwrap();
    seed(
        dir.path(),
        "Alice",
        &[
            (0, "furnace", 1, 0),
            (1, "furnace_crucible", 1, 0),
            (2, "coal", 3, 0),
            (3, "copper_ore", 2, 0),
            (4, "stone", 1, 0),
        ],
    );
    let server = bm_server::start(config(dir.path())).unwrap();
    let mut alice = TestClient::login(&server, "Alice").await;
    let shape = alice.shape();
    let feet = alice.feet();
    alice.wait_for_chunk(feet.chunk()).await;
    alice.wait_for_chunk(feet.offset(1, 2, 0).chunk()).await;
    let base = shape.wrap_block(feet.offset(1, 1, 0));
    let top = shape.wrap_block(feet.offset(1, 2, 0));
    let copper = alice.recipe("copper_ingot");

    // A base alone does nothing.
    assert!(place(&mut alice, 1, base, 0, None).await.0);
    let alone = look_at_station(&mut alice, base, |_| true).await;
    assert_eq!(alone.idle.as_deref(), Some("A furnace needs its crucible on top"));
    craft_at(&mut alice, copper, 1, Some(base)).await;
    alice.notice("A furnace needs its crucible on top").await;
    alice.notices.clear();
    fuel(&mut alice, base, 2, 1, true).await;
    alice.notice("A furnace needs its crucible on top").await;
    alice.settle().await;
    assert_eq!(
        (alice.count("copper_ore"), alice.count("coal")),
        (2, 3),
        "nothing taken"
    );

    // With its crucible it works, opened from either block.
    assert!(place(&mut alice, 2, top, 1, None).await.0);
    let v = look_at_station(&mut alice, top, |_| true).await;
    assert_eq!(
        (v.pos, v.station, v.idle.clone(), v.fuel, v.heat),
        (base, Station::Furnace, None, None, 0)
    );

    // No fuel: the job waits, without progress.
    craft_at(&mut alice, copper, 2, Some(top)).await;
    let waiting = alice.station(|v| !v.jobs.is_empty()).await;
    assert_eq!(waiting.jobs[0].progress, 0.0);
    tokio::time::sleep(Duration::from_millis(600)).await;
    let still = look_at_station(&mut alice, base, |_| true).await;
    assert_eq!(still.jobs[0].progress, 0.0, "waits for heat");

    // Only what burns goes in.
    fuel(&mut alice, top, 4, 1, true).await;
    alice.notice("That does not burn").await;
    // Two coal: one burns at once for the first craft, one waits.
    fuel(&mut alice, top, 2, 2, true).await;
    let lit = alice.station(|v| v.fuel.is_some_and(|f| f.count == 1)).await;
    assert_eq!(lit.heat, 0, "a coal is 100 heat, a copper bar takes 100");
    alice.settle().await;
    assert_eq!(alice.count("coal"), 1);
    alice
        .station(|v| v.jobs.first().is_some_and(|j| j.progress > 0.0))
        .await;

    // Fuel in the slot: neither block breaks.
    for (seq, p) in [(3, top), (4, base)] {
        assert!(!alice.dig(seq, p).await.0);
        alice.notice("Empty the Furnace before breaking it").await;
    }
    // Taking the fuel out (into a chosen slot) leaves the craft burning.
    fuel(&mut alice, base, 7, 5, false).await;
    let out = look_at_station(&mut alice, base, |v| v.fuel.is_none()).await;
    assert!(out.jobs[0].progress > 0.0);
    alice.settle().await;
    assert_eq!(
        stack(&alice, 7).map(|s| (s.item, s.count)),
        Some((alice.item("coal"), 1))
    );
    assert_eq!(alice.count("coal"), 2);
    server.stop();
}

/// Milestone 11, acceptance 9: jobs, the fuel slot, banked heat and a craft
/// waiting for heat all survive a restart.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn station_jobs_fuel_and_heat_continue_across_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    seed(
        dir.path(),
        "Alice",
        &[
            (0, "furnace", 2, 0),
            (1, "furnace_crucible", 2, 0),
            (2, "coal", 3, 0),
            (3, "iron_ore", 1, 0),
            (4, "copper_ore", 1, 0),
        ],
    );
    let (burning, waiting) = {
        let server = bm_server::start(config(dir.path())).unwrap();
        let mut alice = TestClient::login(&server, "Alice").await;
        let shape = alice.shape();
        let feet = alice.feet();
        for dz in 0..=2 {
            alice.wait_for_chunk(feet.offset(1, 2, dz).chunk()).await;
        }
        let burning = shape.wrap_block(feet.offset(1, 1, 0));
        let waiting = shape.wrap_block(feet.offset(1, 1, 2));
        for (seq, b) in [(1, burning), (3, waiting)] {
            assert!(place(&mut alice, seq, b, 0, None).await.0);
            assert!(place(&mut alice, seq + 1, b.offset(0, 1, 0), 1, None).await.0);
        }
        // Three coal; an iron bar burns two and banks 50.
        alice.send(ClientMsg::WatchStation { pos: Some(burning) }).await;
        fuel(&mut alice, burning, 2, 3, true).await;
        alice.station(|v| v.fuel.is_some()).await;
        let r = alice.recipe("iron_ingot");
        craft_at(&mut alice, r, 1, Some(burning)).await;
        let v = alice.station(|v| !v.jobs.is_empty()).await;
        assert_eq!((v.fuel.map(|f| f.count), v.heat), (Some(1), 50));
        // The other has nothing to burn.
        alice.send(ClientMsg::WatchStation { pos: Some(waiting) }).await;
        let r = alice.recipe("copper_ingot");
        craft_at(&mut alice, r, 1, Some(waiting)).await;
        alice.station(|v| !v.jobs.is_empty()).await;
        drop(alice);
        server.stop();
        (burning, waiting)
    };
    let server = bm_server::start(config(dir.path())).unwrap();
    let mut alice = TestClient::login(&server, "Alice").await;
    assert_eq!(
        (alice.count("iron_ore"), alice.count("copper_ore"), alice.count("coal")),
        (0, 0, 0),
        "the inputs and fuel stay spent"
    );
    alice.wait_for_chunk(burning.chunk()).await;
    alice.wait_for_chunk(waiting.chunk()).await;
    let resumed = look_at_station(&mut alice, burning, |_| true).await;
    assert_eq!(resumed.jobs.len(), 1, "the job survived");
    assert_eq!((resumed.fuel.map(|f| f.count), resumed.heat), (Some(1), 50));
    let p = resumed.jobs[0].progress;
    assert!(p > 0.0 && p < 1.0);
    alice.station(|v| v.jobs.first().is_some_and(|j| j.progress > p)).await;
    let held = look_at_station(&mut alice, waiting, |_| true).await;
    assert_eq!((held.jobs.len(), held.jobs[0].progress, held.fuel), (1, 0.0, None));
    tokio::time::sleep(Duration::from_millis(400)).await;
    let held = look_at_station(&mut alice, waiting, |_| true).await;
    assert_eq!(held.jobs[0].progress, 0.0, "still waiting for heat");
    server.stop();
}

/// Milestone 11, acceptance 4 and 9: a station saved at one workbench
/// block before workbenches had four keeps its contents, and moves to the
/// machine's first block when the group forms around it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_workbench_saved_at_one_block_joins_its_machine() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path(), "Alice", &[(0, "workbench", 4, 0)]);
    let (old, shape, feet) = {
        let server = bm_server::start(config(dir.path())).unwrap();
        let mut alice = TestClient::login(&server, "Alice").await;
        let shape = alice.shape();
        let feet = alice.feet();
        alice.wait_for_chunk(feet.offset(2, 1, 2).chunk()).await;
        let old = shape.wrap_block(feet.offset(2, 1, 2));
        assert!(place(&mut alice, 1, old, 0, None).await.0);
        drop(alice);
        server.stop();
        (old, shape, feet)
    };
    {
        // What a save from before milestone 11 holds: output at one block.
        let store = Store::open(&planet_file(dir.path())).unwrap();
        store
            .write(&WriteBatch {
                stations: vec![(
                    0,
                    old,
                    Some(SavedStation {
                        kind: Station::Workbench,
                        jobs: vec![],
                        started: 0,
                        output: vec![Some(SavedStack {
                            item: "stones".into(),
                            count: 5,
                            wear: 0,
                        })],
                        fuel: None,
                        heat: 0,
                        waiting: false,
                        xp: 0,
                        wear: bm_core::spark::MACHINE_WEAR,
                        spark: 0,
                        burn_total: 0,
                        burn_elapsed: 0,
                        blocks: 1,
                    }),
                )],
                ..Default::default()
            })
            .unwrap();
    }
    let server = bm_server::start(config(dir.path())).unwrap();
    let mut alice = TestClient::login(&server, "Alice").await;
    alice.wait_for_chunk(old.chunk()).await;
    let v = look_at_station(&mut alice, old, |_| true).await;
    assert_eq!(v.pos, old);
    assert_eq!(v.idle.as_deref(), Some("A workbench needs 4 blocks touching"));
    assert_eq!(v.output[0].count, 5, "nothing lost");
    assert!(!alice.dig(2, old).await.0);
    alice.notice("Empty the Workbench before breaking it").await;
    // Three more, so the old block is not the first.
    let others = [feet.offset(1, 1, 2), feet.offset(3, 1, 2), feet.offset(1, 1, 1)].map(|p| shape.wrap_block(p));
    for (i, p) in others.iter().enumerate() {
        assert!(place(&mut alice, 3 + i as u32, *p, 0, None).await.0);
    }
    let first = *others.iter().chain([&old]).min_by_key(|p| (p.y, p.z, p.x)).unwrap();
    assert_ne!(first, old);
    let v = look_at_station(&mut alice, others[1], |_| true).await;
    assert_eq!((v.pos, v.idle.clone()), (first, None));
    assert_eq!(v.output[0].count, 5, "the record moved to the machine");
    alice.send(ClientMsg::CollectStation { pos: old }).await;
    alice.station(|v| v.output.is_empty()).await;
    alice.settle().await;
    assert_eq!(alice.count("stones"), 5);
    server.stop();
}

/// Milestone 11, acceptance 2: gathering objects break with any tool and
/// give their drops (whole expected counts always), and a plant falls with
/// the ground it stands on, its drops into the breaker's pack.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn plants_and_boulders_give_their_drops_and_fall_with_their_ground() {
    let dir = tempfile::tempdir().unwrap();
    seed(
        dir.path(),
        "Alice",
        &[
            (0, "stone_axe", 1, 0),
            (1, "stone_hammer", 1, 0),
            (2, "stone_shovel", 1, 0),
        ],
    );
    // Grow a spikeleaf, a boulder and another spikeleaf beside the arrival
    // spot, by saving the chunk with them in it.
    let (plant, boulder, perched) = {
        let server = bm_server::start(config(dir.path())).unwrap();
        let mut alice = TestClient::login(&server, "Alice").await;
        let shape = alice.shape();
        let feet = alice.feet();
        let spots = [feet.offset(1, 1, 0), feet.offset(2, 1, 0), feet.offset(3, 1, 0)].map(|p| shape.wrap_block(p));
        let mut chunks: HashMap<ChunkPos, Chunk> = HashMap::new();
        for (p, name) in spots.iter().zip(["spikeleaf", "boulder", "spikeleaf"]) {
            alice.wait_for_chunk(p.chunk()).await;
            let c = chunks
                .entry(p.chunk())
                .or_insert_with(|| alice.chunks[&p.chunk()].clone());
            assert!(c.state(p.local()).id.is_air());
            c.set_state(p.local(), BlockState::plain(alice.block(name)));
        }
        drop(alice);
        server.stop();
        let store = Store::open(&planet_file(dir.path())).unwrap();
        store
            .write(&WriteBatch {
                chunks: chunks.into_iter().map(|(p, c)| (0, p, c.encode())).collect(),
                ..Default::default()
            })
            .unwrap();
        (spots[0], spots[1], spots[2])
    };
    let server = bm_server::start(config(dir.path())).unwrap();
    let mut alice = TestClient::login(&server, "Alice").await;
    for p in [plant, boulder, perched] {
        alice.wait_for_chunk(p.chunk()).await;
    }
    assert_eq!(state_at(&alice, plant).id, alice.block("spikeleaf"));
    // A spikeleaf: at least 1 fibrous leaf and 1 tuber (1.3 each).
    assert!(alice.dig_with(1, plant, 0).await.0);
    assert!(alice.count("fibrous_leaf") >= 1 && alice.count("tuber") >= 1);
    // A boulder: at least 1 flint (1.3).
    assert!(alice.dig_with(2, boulder, 1).await.0);
    assert!(alice.count("flint") >= 1);
    // Digging out the ground under the other drops it into the pack.
    let leaves = alice.count("fibrous_leaf");
    assert!(alice.dig_with(3, perched.offset(0, -1, 0), 2).await.0);
    assert_eq!(state_at(&alice, perched), BlockState::AIR, "it fell");
    assert!(alice.count("fibrous_leaf") > leaves, "and its leaves are hers");
    server.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn claims_keep_other_players_out() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path(), "Alice", &[(5, "claim_stone", 1, 0)]);
    let server = bm_server::start(config(dir.path())).unwrap();
    let mut alice = TestClient::login(&server, "Alice").await;
    let mut bob = TestClient::register(&server, "Bob").await;
    let feet = alice.feet();
    alice.wait_for_chunk(feet.chunk()).await;
    bob.wait_for_chunk(feet.chunk()).await;
    assert_eq!(alice.welcome.plot_allowance, 64);

    assert!(alice.dig(1, feet).await.0);
    assert!(alice.edit(2, feet, EditAction::Place { slot: 5, mount: None }).await.0);
    let claim = match bob.until(|m| matches!(m, ServerMsg::Claim(_))).await {
        ServerMsg::Claim(c) => c,
        _ => unreachable!(),
    };
    let shape = alice.shape();
    let home = PlotPos::of(feet, shape);
    assert_eq!(
        (claim.owner.as_str(), claim.stone, claim.plots.clone()),
        ("Alice", feet, vec![home])
    );

    // Bob cannot break or build there.
    assert!(!bob.dig(1, feet).await.0);
    bob.notice("This land belongs to Alice").await;

    // Alice grows and shrinks her claim.
    let next = home.offset(1, 0, 0).wrap(shape);
    alice
        .send(ClientMsg::ClaimPlot {
            claim: claim.id,
            plot: next,
        })
        .await;
    let grown = match alice
        .until(|m| matches!(m, ServerMsg::Claim(c) if c.plots.len() == 2))
        .await
    {
        ServerMsg::Claim(c) => c,
        _ => unreachable!(),
    };
    assert!(grown.plots.contains(&next));
    alice
        .send(ClientMsg::ClaimPlot {
            claim: claim.id,
            plot: home.offset(5, 0, 0),
        })
        .await;
    alice.notice("A new plot must touch your claim").await;
    alice.send(ClientMsg::ReleasePlot { plot: home }).await;
    alice.notice("The claim stone's plot cannot be released").await;
    bob.send(ClientMsg::ReleasePlot { plot: next }).await;
    bob.notice("That is not your claim").await;
    alice.send(ClientMsg::ReleasePlot { plot: next }).await;
    alice
        .until(|m| matches!(m, ServerMsg::Claim(c) if c.plots.len() == 1))
        .await;

    // Breaking the stone releases the claim, and the stone comes back.
    assert!(alice.dig(3, feet).await.0);
    bob.until(|m| matches!(m, ServerMsg::ClaimRemoved { id } if *id == claim.id))
        .await;
    assert_eq!(alice.count("claim_stone"), 1);
    // A later login hears of no claims.
    let carol = TestClient::register(&server, "Carol").await;
    assert!(carol.welcome.claims.is_empty());
    server.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_version_1_planet_opens_and_keeps_its_players() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = config(dir.path());
    let pos = glam::DVec3::new(100.5, 300.0, 120.5);
    {
        // Exactly what save version 1 wrote, by generator 1.
        let db = redb::Database::create(old_planet_file(dir.path())).unwrap();
        let tx = db.begin_write().unwrap();
        {
            let mut meta =
                bm_server::store::legacy::v2_meta("Testworld", bm_core::PlanetShape::new(16, cfg.height_chunks), 7, 1);
            meta.save_version = 1;
            let meta_table: redb::TableDefinition<&str, &[u8]> = redb::TableDefinition::new("meta");
            let players: redb::TableDefinition<&str, &[u8]> = redb::TableDefinition::new("players");
            tx.open_table(meta_table)
                .unwrap()
                .insert("planet", postcard::to_stdvec(&meta).unwrap().as_slice())
                .unwrap();
            let state = SavedBody {
                pos,
                ..Default::default()
            };
            tx.open_table(players)
                .unwrap()
                .insert("Alice", postcard::to_stdvec(&state).unwrap().as_slice())
                .unwrap();
        }
        tx.commit().unwrap();
    }
    let server = bm_server::start(cfg).unwrap();
    let alice = TestClient::register(&server, "Alice").await;
    assert!(
        (alice.welcome.spawn.pos - pos).length() < 1e-9,
        "the version 1 position is hers: {:?}",
        alice.welcome.spawn.pos
    );
    assert!(
        alice.welcome.inventory.slots.iter().all(Option::is_none),
        "her saved (empty) pack, not a new player's"
    );
    assert_eq!(alice.welcome.planet.name, "Testworld");
    drop(alice);
    server.stop();
    let store = Store::open(&planet_file(dir.path())).unwrap();
    let kinds = bm_worldgen::PlanetKinds::builtin(&BlockRegistry::builtin());
    assert_eq!(store.universe(&kinds).unwrap(), None, "already version 3");
    assert!(store.planet(0).unwrap().unwrap().shrine.is_some());
    assert!(old_planet_file(dir.path()).exists(), "the old file is kept as a backup");
}

/// A save version 2 planet becomes planet 0 of the universe with everything
/// kept: its builds, claims, characters and inventories.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_version_2_planet_opens_as_the_first_planet_with_everything_kept() {
    use bm_server::store::legacy::{V2Player, v2_meta, write_v2_planet};
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = universe(dir.path());
    // A server.toml of version 2 named the planet like this.
    cfg.planets[0].name = "Verdance".into();
    let shape = bm_core::PlanetShape::new(16, cfg.height_chunks);
    let blocks = BlockRegistry::builtin();
    let glass = blocks.id("glass").unwrap();
    // A build: one chunk high in the sky turned to glass.
    let build = ChunkPos::new(3, 10, 3);
    let pos = glam::DVec3::new(100.5, 300.0, 100.5);
    write_v2_planet(
        &old_planet_file(dir.path()),
        &v2_meta("Testworld", shape, 7, 1),
        &[(build, Chunk::filled(glass).encode())],
        &[V2Player {
            key: "alice",
            state: Some(SavedBody {
                pos,
                ..Default::default()
            }),
            inventory: vec![Some(SavedStack {
                item: "stone".into(),
                count: 5,
                wear: 0,
            })],
        }],
        &[],
        &[(
            4,
            "alice",
            "Alice",
            BlockPos::new(100, 300, 100),
            vec![PlotPos::of(BlockPos::new(100, 300, 100), shape)],
        )],
        1000,
    )
    .unwrap();
    let server = bm_server::start(cfg).unwrap();
    let mut alice = TestClient::register(&server, "Alice").await;
    assert_eq!(
        (alice.welcome.planet.id, alice.welcome.planet.name.as_str()),
        (0, "Testworld"),
        "the old planet, its name kept"
    );
    assert_eq!(alice.welcome.universe.len(), 2);
    assert!((alice.welcome.spawn.pos - pos).length() < 1e-9, "where she was");
    assert_eq!(alice.count("stone"), 5, "her inventory");
    assert_eq!(alice.welcome.claims.len(), 1);
    assert_eq!(
        (alice.welcome.claims[0].id, alice.welcome.claims[0].owner.as_str()),
        (4, "Alice")
    );
    assert!(alice.welcome.tick >= 1000, "the clock carries on");
    alice.wait_for_chunk(build).await;
    assert_eq!(alice.chunks[&build].uniform(), Some(glass), "the build is there");
    server.stop();
}

/// Holding break straight on from one block to the next, as the game does:
/// the break of the first block waits for the input it follows, and the swing
/// at the next block, sent just after, must not overtake it (a hold did, in
/// milestone 3, and every second block was refused and flashed back).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn digging_block_after_block_keeps_every_break() {
    let dir = tempfile::tempdir().unwrap();
    let server = bm_server::start(config(dir.path())).unwrap();
    let mut carol = TestClient::register(&server, "Carol").await;
    let first = carol.feet();
    let second = first.offset(0, -1, 0);
    carol.wait_for_chunk(first.chunk()).await;
    carol.wait_for_chunk(second.chunk()).await;
    let tier = carol.planet.tier;
    let hits = |c: &TestClient, p: BlockPos| {
        let def = &c.welcome.blocks[c.chunks[&p.chunk()].get(p.local()).0 as usize];
        let stone = c.items.swing_tool(Some(c.item("hand_stone"))).unwrap();
        bm_core::mining::hits_to_break(&stone, def, tier).unwrap()
    };
    let (n1, n2) = (hits(&carol, first), hits(&carol, second));
    assert!(n1 > 1 && n2 > 1, "the ground takes several swings");
    let ticks = carol.swing_ticks(HAND);

    // Inputs at the real rate, as the game sends them, and each swing sent
    // ten inputs before the one it follows: the last swing at the first
    // block breaks it, and the first at the second (sent straight after)
    // must not overtake it.
    let base = carol.seq + 20;
    let mut swings = (0..n1 + n2).map(|k| {
        let (seq, pos) = if k < n1 { (1, first) } else { (2, second) };
        (base + k * ticks, seq, pos)
    });
    let mut next = swings.next();
    while let Some((at, seq, pos)) = next {
        if carol.seq + 10 >= at {
            carol
                .send(ClientMsg::Swing {
                    seq,
                    at,
                    pos,
                    slot: HAND,
                })
                .await;
            next = swings.next();
            continue;
        }
        carol.send_inputs(1, PlayerInput::default());
        tokio::time::sleep(Duration::from_secs_f64(1.0 / 60.0)).await;
    }
    for _ in 0..30 {
        carol.send_inputs(1, PlayerInput::default());
        tokio::time::sleep(Duration::from_secs_f64(1.0 / 60.0)).await;
    }

    let mut results = Vec::new();
    while results.len() < 2 {
        if let ServerMsg::EditResult { seq, accepted, .. } =
            carol.until(|m| matches!(m, ServerMsg::EditResult { .. })).await
        {
            results.push((seq, accepted));
        }
    }
    assert_eq!(
        results,
        vec![(1, true), (2, true)],
        "both breaks kept, no swing refused"
    );
}

/// Travel by warp shrine: refused away from one, accepted beside one; the
/// traveller is told, then streamed the new planet; others see them go and
/// come; the planet left behind holds no chunks; the shrine and its ground
/// cannot be changed.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn warp_shrines_take_players_between_planets() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path(), "Alice", &[(1, "stone", 10, 0)]);
    let server = bm_server::start(universe(dir.path())).unwrap();
    let mut alice = TestClient::login(&server, "Alice").await;
    let mut bob = TestClient::register(&server, "Bob").await;
    assert_eq!(alice.welcome.planet.id, 0);
    let names: Vec<&str> = alice.welcome.universe.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["Testworld", "Dryside"]);
    assert_eq!(alice.welcome.universe[1].kind, "Arid");
    assert_eq!(alice.welcome.planet.tints.len(), alice.welcome.blocks.len());
    let shrine = alice.shrine();
    alice.wait_for_chunk(shrine.chunk()).await;
    alice.wait_for_chunk(alice.feet().chunk()).await;
    assert_eq!(
        alice.chunks[&shrine.chunk()].get(shrine.local()),
        alice.block("warp_shrine"),
        "she arrived beside the shrine"
    );

    // The shrine cannot be broken, nor its ground changed or built on.
    let feet = alice.feet();
    assert!(!alice.dig(1, shrine).await.0, "the shrine stands");
    alice.notice("The ground around a warp shrine cannot be changed").await;
    assert!(!alice.dig(2, feet).await.0, "nor the ground beside it");
    assert!(
        !alice
            .edit(3, shrine.offset(0, 1, 0), EditAction::Place { slot: 1, mount: None })
            .await
            .0,
        "nor can it be built over"
    );
    assert_eq!(alice.count("stone"), 10);

    // Away from a shrine, travel is refused.
    alice
        .send(ClientMsg::Warp {
            shrine: shrine.offset(40, 0, 0),
            planet: 1,
        })
        .await;
    alice.notice("Stand at a warp shrine to travel").await;
    alice.send(ClientMsg::Warp { shrine, planet: 0 }).await;
    alice.notice("You are already on Testworld").await;

    // Beside it, she goes.
    alice.send(ClientMsg::Warp { shrine, planet: 1 }).await;
    let travel = match alice.until(|m| matches!(m, ServerMsg::Travel(_))).await {
        ServerMsg::Travel(t) => t,
        _ => unreachable!(),
    };
    assert_eq!((travel.planet.id, travel.planet.name.as_str()), (1, "Dryside"));
    assert_eq!(travel.planet.kind, "Arid");
    assert!(travel.claims.is_empty());
    bob.until(|m| matches!(m, ServerMsg::PlayerLeft { id } if *id == alice.welcome.player_id))
        .await;
    // The new planet streams in, shrine and all.
    let there = alice.shrine();
    alice.wait_for_chunk(there.chunk()).await;
    assert_eq!(
        alice.chunks[&there.chunk()].get(there.local()),
        alice.block("warp_shrine")
    );
    let snap_planet = loop {
        let ServerDatagram::Snapshot { planet, .. } = alice.snaps.recv().await.unwrap();
        if planet == 1 {
            break planet;
        }
    };
    assert_eq!(snap_planet, 1, "snapshots name the new planet");

    // Bob follows and each sees the other arrive.
    let bob_shrine = bob.shrine();
    bob.send(ClientMsg::Warp {
        shrine: bob_shrine,
        planet: 1,
    })
    .await;
    bob.until(|m| matches!(m, ServerMsg::Travel(_))).await;
    bob.until(|m| matches!(m, ServerMsg::PlayerJoined { id, .. } if *id == alice.welcome.player_id))
        .await;
    alice
        .until(|m| matches!(m, ServerMsg::PlayerJoined { id, .. } if *id == bob.welcome.player_id))
        .await;
    alice
        .until(|m| matches!(m, ServerMsg::Universe { planets } if planets[1].players == 2 && planets[0].players == 0))
        .await;

    // Nobody is left on the first planet: it holds no chunks.
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let load = server.planet_load();
        if load[0].players == 0 && load[0].resident == 0 && load[0].held == 0 && load[1].resident > 0 {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "first planet still holds chunks: {load:?}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Her travel is saved: she logs in again where she went.
    drop(alice);
    tokio::time::sleep(Duration::from_millis(300)).await;
    let alice = TestClient::login(&server, "Alice").await;
    assert_eq!(alice.welcome.planet.id, 1);
    assert_eq!(alice.count("stone"), 10);
    drop(bob);
    server.stop();
}

/// Storage chests: placed, opened by two players who both see every change,
/// moves in, out and within, refused moves answered with the contents, no
/// breaking while anything is inside, and closed for everyone when broken.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn storage_chests_hold_items_for_everyone_who_opens_them() {
    let dir = tempfile::tempdir().unwrap();
    seed(
        dir.path(),
        "Alice",
        &[(5, "storage_chest", 1, 0), (1, "stone", 50, 0), (2, "sand", 10, 0)],
    );
    let server = bm_server::start(config(dir.path())).unwrap();
    let mut alice = TestClient::login(&server, "Alice").await;
    let mut bob = TestClient::register(&server, "Bob").await;
    let feet = alice.feet();
    alice.wait_for_chunk(feet.chunk()).await;
    bob.wait_for_chunk(feet.chunk()).await;

    // Opening what is not a storage block gets a closed answer.
    alice.send(ClientMsg::OpenStorage { pos: Some(feet) }).await;
    assert!(alice.storage(|v| v.pos == feet).await.closed);

    assert!(alice.dig(1, feet).await.0);
    assert!(alice.edit(2, feet, EditAction::Place { slot: 5, mount: None }).await.0);
    alice.send(ClientMsg::OpenStorage { pos: Some(feet) }).await;
    let v = alice.storage(|v| v.pos == feet).await;
    assert!(!v.closed);
    assert_eq!(v.slots, vec![None; 20], "twenty empty slots");
    bob.send(ClientMsg::OpenStorage { pos: Some(feet) }).await;
    assert!(!bob.storage(|v| v.pos == feet).await.closed);

    let move_items = |from, to, count| ClientMsg::StorageMove {
        pos: feet,
        from,
        to,
        count,
    };
    let stone = alice.item("stone");
    let holds = |v: &StorageView, slot: usize, count: u16| v.slots[slot].is_some_and(|s| s.count == count);

    // In: the pack loses what the chest gains, and both viewers see it.
    alice
        .send(move_items(SlotRef::Inventory(1), SlotRef::Storage(0), 30))
        .await;
    let v = alice.storage(|v| holds(v, 0, 30)).await;
    assert_eq!(v.slots[0].unwrap().item, stone);
    alice.settle().await;
    assert_eq!(alice.count("stone"), 20);
    bob.storage(|v| holds(v, 0, 30)).await;

    // Out, by the other player; the first sees it go.
    bob.send(move_items(SlotRef::Storage(0), SlotRef::Inventory(3), 10))
        .await;
    bob.storage(|v| holds(v, 0, 20)).await;
    bob.settle().await;
    assert_eq!(bob.count("stone"), 10);
    alice.storage(|v| holds(v, 0, 20)).await;

    // Within the chest.
    alice
        .send(move_items(SlotRef::Storage(0), SlotRef::Storage(19), 5))
        .await;
    alice.storage(|v| holds(v, 0, 15) && holds(v, 19, 5)).await;

    // A move that cannot happen changes nothing and is answered with what
    // the chest holds: a slot beyond it, part of a stack onto another item.
    alice
        .send(move_items(SlotRef::Inventory(2), SlotRef::Storage(20), 1))
        .await;
    alice
        .send(move_items(SlotRef::Inventory(2), SlotRef::Storage(0), 3))
        .await;
    alice.settle().await;
    assert_eq!(alice.count("sand"), 10);

    // Full, it cannot be broken.
    assert!(!alice.dig(3, feet).await.0, "a chest with things in it stands");
    alice.notice("Empty the Storage Chest before breaking it").await;

    // Emptied, it breaks like any block, and closes for everyone.
    alice
        .send(move_items(SlotRef::Storage(0), SlotRef::Inventory(1), 100))
        .await;
    alice
        .send(move_items(SlotRef::Storage(19), SlotRef::Inventory(1), 100))
        .await;
    alice.storage(|v| v.slots.iter().all(Option::is_none)).await;
    alice.settle().await;
    assert_eq!(alice.count("stone"), 40);
    assert!(alice.dig(4, feet).await.0, "an empty chest breaks");
    assert_eq!(alice.count("storage_chest"), 1, "and comes back as an item");
    assert!(bob.storage(|v| v.pos == feet && v.closed).await.slots.is_empty());
    alice.storage(|v| v.pos == feet && v.closed).await;
    server.stop();
}

/// A chest's contents are saved with everything else; walking away closes it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn storage_contents_survive_a_restart_and_close_out_of_reach() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path(), "Alice", &[(5, "storage_chest", 1, 0), (1, "stone", 50, 0)]);
    let feet = {
        let server = bm_server::start(config(dir.path())).unwrap();
        let mut alice = TestClient::login(&server, "Alice").await;
        // In the plaza ground, five blocks behind her (she walks off along
        // -Z, the way the plaza is open).
        let feet = alice.feet().offset(0, 0, 5);
        alice.wait_for_chunk(feet.chunk()).await;
        assert!(alice.dig(1, feet).await.0);
        assert!(alice.edit(2, feet, EditAction::Place { slot: 5, mount: None }).await.0);
        alice.send(ClientMsg::OpenStorage { pos: Some(feet) }).await;
        alice
            .send(ClientMsg::StorageMove {
                pos: feet,
                from: SlotRef::Inventory(1),
                to: SlotRef::Storage(3),
                count: 25,
            })
            .await;
        alice.storage(|v| v.slots[3].is_some()).await;
        drop(alice);
        server.stop();
        feet
    };
    let server = bm_server::start(config(dir.path())).unwrap();
    let mut alice = TestClient::login(&server, "Alice").await;
    assert_eq!(alice.count("stone"), 25, "the pack was saved with the chest");
    alice.wait_for_chunk(feet.chunk()).await;
    assert_eq!(
        alice.chunks[&feet.chunk()].get(feet.local()),
        alice.block("storage_chest")
    );
    alice.send(ClientMsg::OpenStorage { pos: Some(feet) }).await;
    let v = alice.storage(|v| v.pos == feet).await;
    assert_eq!(v.slots[3].map(|s| (s.item, s.count)), Some((alice.item("stone"), 25)));

    // Walk away at the real input rate until the server closes it.
    let walk = PlayerInput {
        forward: 1,
        sprint: true,
        jump: true,
        ..Default::default()
    };
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        alice.send_inputs(1, walk);
        tokio::time::sleep(Duration::from_secs_f64(1.0 / 60.0)).await;
        while let Ok(m) = alice.msgs.try_recv() {
            alice.apply(&m);
            if let ServerMsg::Storage(v) = &m
                && v.closed
            {
                assert_eq!(v.pos, feet);
                server.stop();
                return;
            }
        }
        if tokio::time::Instant::now() >= deadline {
            let (_, ack, st, _) = alice.latest_snapshot().await;
            panic!("never closed walking away: at {:?} ack {ack}, chest {feet:?}", st.pos);
        }
    }
}

/// Like a station, a chest on someone else's claim cannot be opened.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn chests_on_a_claim_open_only_for_its_owner() {
    let dir = tempfile::tempdir().unwrap();
    seed(
        dir.path(),
        "Alice",
        &[(5, "storage_chest", 1, 0), (6, "claim_stone", 1, 0), (1, "stone", 5, 0)],
    );
    let server = bm_server::start(config(dir.path())).unwrap();
    let mut alice = TestClient::login(&server, "Alice").await;
    let mut bob = TestClient::register(&server, "Bob").await;
    let feet = alice.feet();
    let beside = feet.offset(1, 0, 0);
    alice.wait_for_chunk(feet.chunk()).await;
    alice.wait_for_chunk(beside.chunk()).await;
    bob.wait_for_chunk(feet.chunk()).await;
    assert!(alice.dig(1, feet).await.0);
    assert!(alice.edit(2, feet, EditAction::Place { slot: 5, mount: None }).await.0);
    assert!(alice.dig(3, beside).await.0);
    assert!(
        alice
            .edit(4, beside, EditAction::Place { slot: 6, mount: None })
            .await
            .0
    );
    let claim = match bob.until(|m| matches!(m, ServerMsg::Claim(_))).await {
        ServerMsg::Claim(c) => c,
        _ => unreachable!(),
    };
    let chest_plot = PlotPos::of(feet, alice.shape());
    if !claim.plots.contains(&chest_plot) {
        alice
            .send(ClientMsg::ClaimPlot {
                claim: claim.id,
                plot: chest_plot,
            })
            .await;
        bob.until(|m| matches!(m, ServerMsg::Claim(c) if c.plots.contains(&chest_plot)))
            .await;
    }

    bob.send(ClientMsg::OpenStorage { pos: Some(feet) }).await;
    assert!(bob.storage(|v| v.pos == feet).await.closed, "refused");
    bob.notice("This land belongs to Alice").await;
    bob.send(ClientMsg::StorageMove {
        pos: feet,
        from: SlotRef::Storage(0),
        to: SlotRef::Inventory(0),
        count: 1,
    })
    .await;
    bob.settle().await;
    assert!(
        bob.inv
            .slots
            .iter()
            .enumerate()
            .all(|(i, s)| i == HAND as usize || s.is_none()),
        "nothing taken"
    );

    alice.send(ClientMsg::OpenStorage { pos: Some(feet) }).await;
    assert!(!alice.storage(|v| v.pos == feet).await.closed, "the owner opens it");
    server.stop();
}

/// Registers players on a first run, then moves `far` 150 blocks away and
/// `elsewhere` to the second planet before the server starts again.
fn place_players(dir: &Path, far: &str, elsewhere: &str) {
    let store = Store::open(&planet_file(dir)).unwrap();
    let mut f = store.player(far).unwrap().unwrap();
    let s = f.state.as_mut().unwrap();
    s.pos += glam::DVec3::new(150.0, 40.0, 0.0);
    let mut e = store.player(elsewhere).unwrap().unwrap();
    e.planet = 1;
    e.state = None;
    store
        .write(&WriteBatch {
            players: vec![(far.into(), f), (elsewhere.into(), e)],
            ..Default::default()
        })
        .unwrap();
}

/// Who hears what: local near and not far, planet, universe, whispers;
/// cleaning, the rate limit, history on joining, arrival and leaving lines.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn chat_reaches_whoever_the_channel_says() {
    let dir = tempfile::tempdir().unwrap();
    {
        let server = bm_server::start(universe(dir.path())).unwrap();
        for name in ["Alice", "Bob", "Far", "Dave"] {
            drop(TestClient::register(&server, name).await);
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
        server.stop();
    }
    place_players(dir.path(), "far", "dave");
    let server = bm_server::start(universe(dir.path())).unwrap();
    let mut alice = TestClient::login(&server, "Alice").await;
    let mut far = TestClient::login(&server, "Far").await;
    let mut dave = TestClient::login(&server, "Dave").await;
    let mut bob = TestClient::login(&server, "Bob").await;
    assert_eq!(dave.welcome.planet.id, 1);
    assert!((far.spawn.pos - alice.spawn.pos).length() > 100.0);
    assert!(
        only_the_hand_stone(&alice),
        "a new player starts with the hand stone alone"
    );

    // Arrivals are announced on the planet, by the server.
    let line = alice.heard(|l| l.text == "Bob arrived on Testworld").await;
    assert_eq!((line.from.as_str(), &line.channel), ("", &Channel::Planet));
    far.heard(|l| l.text == "Bob arrived on Testworld").await;
    bob.heard(|l| l.text == "Bob arrived on Testworld").await;

    // Local: near, not far, not another planet. The text is cleaned.
    alice.say(Channel::Local, "  hello\u{7}   near \n").await;
    let line = bob.heard(|l| l.channel == Channel::Local).await;
    assert_eq!((line.from.as_str(), line.text.as_str()), ("Alice", "hello near"));
    assert!(line.time > 1_700_000_000);
    alice.heard(|l| l.text == "hello near").await;
    // Planet: far hears it, the other planet does not.
    alice.say(Channel::Planet, "planet wide").await;
    far.heard(|l| l.text == "planet wide").await;
    // Universe: everyone.
    alice.say(Channel::Universe, "to everyone").await;
    dave.heard(|l| l.text == "to everyone").await;
    far.heard(|l| l.text == "to everyone").await;
    bob.heard(|l| l.text == "to everyone").await;
    // Lines that clean to nothing are dropped (and cost nothing).
    alice.say(Channel::Universe, " \u{0}\u{200B} ").await;
    // Whisper: by name in any case, the sender gets a copy, nobody else hears.
    alice.say(Channel::Whisper("dAVE".into()), "psst").await;
    let line = dave.heard(|l| l.text == "psst").await;
    assert_eq!(
        (line.from.as_str(), &line.channel),
        ("Alice", &Channel::Whisper("Dave".into()))
    );
    alice.heard(|l| l.text == "psst").await;
    alice.say(Channel::Whisper("Nobody".into()), "hello?").await;
    alice.notice("Nobody called Nobody is here").await;

    for c in [&mut alice, &mut bob, &mut far, &mut dave] {
        c.settle().await;
    }
    assert!(!far.has_heard("hello near"), "too far for local");
    assert!(
        !dave.has_heard("hello near") && !dave.has_heard("planet wide"),
        "another planet"
    );
    assert!(!bob.has_heard("psst") && !far.has_heard("psst"), "whispers are private");
    assert!(!far.has_heard("hello?"));
    assert_eq!(alice.chat.iter().filter(|l| l.text == "psst").count(), 1);

    // Talking too fast: a burst of five, then a notice.
    for i in 0..8 {
        bob.say(Channel::Universe, &format!("spam {i}")).await;
    }
    bob.notice("You are talking too fast").await;
    alice.heard(|l| l.text == "spam 4").await;
    alice.settle().await;
    assert!(!alice.has_heard("spam 5"), "dropped past the burst");

    // Joining brings the universe history, after the welcome.
    let mut carol = TestClient::register(&server, "Carol").await;
    let history = match carol.until(|m| matches!(m, ServerMsg::Chat { .. })).await {
        ServerMsg::Chat { lines } => lines,
        _ => unreachable!(),
    };
    let texts: Vec<&str> = history.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(
        texts,
        ["to everyone", "spam 0", "spam 1", "spam 2", "spam 3", "spam 4"],
        "universe lines only, oldest first"
    );
    alice.heard(|l| l.text == "Carol arrived on Testworld").await;

    // Leaving and travelling away are announced where they left.
    drop(carol);
    alice.heard(|l| l.text == "Carol left").await;
    let shrine = bob.shrine();
    bob.send(ClientMsg::Warp { shrine, planet: 1 }).await;
    alice.heard(|l| l.text == "Bob left").await;
    dave.heard(|l| l.text == "Bob arrived on Dryside").await;
    bob.heard(|l| l.text == "Bob arrived on Dryside").await;
    server.stop();
}

/// The block state a client holds at `p`.
fn state_at(c: &TestClient, p: BlockPos) -> BlockState {
    let p = c.shape().wrap_block(p);
    c.chunks[&p.chunk()].state(p.local())
}

/// Ash in the palette colour nearest a block's painted side colour.
fn natural_ash(c: &TestClient, block: &str) -> BlockState {
    let def = &c.welcome.blocks[c.block(block).0 as usize];
    BlockState::coloured(c.block("ash"), c.welcome.colours.nearest(def.color))
}

/// Spots in the ground row beside the player's feet, in the plot holding
/// them, nearest first (never the feet themselves).
fn spots_in_plot(c: &TestClient, feet: BlockPos) -> Vec<BlockPos> {
    let shape = c.shape();
    let plot = PlotPos::of(feet, shape);
    let mut v: Vec<(i64, BlockPos)> = (-3i64..=3)
        .flat_map(|dz| (-3i64..=3).map(move |dx| (dx, dz)))
        .filter(|&(dx, dz)| (dx, dz) != (0, 0))
        .map(|(dx, dz)| (dx * dx + dz * dz, feet.offset(dx, 0, dz)))
        .filter(|(_, p)| PlotPos::of(*p, shape) == plot)
        .collect();
    v.sort_by_key(|(d, p)| (*d, p.x, p.z));
    v.into_iter().map(|(_, p)| p).collect()
}

/// A claim whose owner has been away too long crumbles: what they built turns
/// to ash in its colours, what they dug stays dug, natural ground stays, the
/// chest's contents wait in reclaim (across a restart) for a warp shrine.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_lapsed_claim_turns_to_ash_and_its_contents_wait_in_reclaim() {
    let dir = tempfile::tempdir().unwrap();
    seed(
        dir.path(),
        "Alice",
        &[
            (5, "claim_stone", 1, 0),
            (6, "storage_chest", 1, 0),
            (1, "stone#17", 10, 0),
            (2, "sand", 10, 0),
            (3, "stone", 40, 0),
        ],
    );
    // Phase 1: Alice claims and builds, then goes away.
    let (feet, shrine, spots, untouched) = {
        let server = bm_server::start(config(dir.path())).unwrap();
        let mut alice = TestClient::login(&server, "Alice").await;
        let feet = alice.feet();
        let shrine = alice.shrine();
        alice.wait_for_chunk(feet.chunk()).await;
        let spots = spots_in_plot(&alice, feet);
        assert!(spots.len() >= 5, "room in the plot");
        let (coloured, natural, chest, dug, plain) = (spots[0], spots[1], spots[2], spots[3], spots[4]);

        assert!(alice.dig(1, feet).await.0);
        assert!(
            alice.edit(2, feet, EditAction::Place { slot: 5, mount: None }).await.0,
            "claimed"
        );
        let mut seq = 3;
        for (pos, slot) in [(coloured, 1), (natural, 2), (chest, 6)] {
            assert!(alice.dig(seq, pos).await.0);
            assert!(
                alice
                    .edit(seq + 1, pos, EditAction::Place { slot, mount: None })
                    .await
                    .0
            );
            seq += 2;
        }
        assert!(alice.dig(seq, dug).await.0);
        assert_eq!(state_at(&alice, coloured).colour, Colour(17));

        alice.send(ClientMsg::OpenStorage { pos: Some(chest) }).await;
        alice.storage(|v| v.pos == chest && !v.closed).await;
        for (from, to, n) in [(3, 0, 30), (1, 1, 5)] {
            alice
                .send(ClientMsg::StorageMove {
                    pos: chest,
                    from: SlotRef::Inventory(from),
                    to: SlotRef::Storage(to),
                    count: n,
                })
                .await;
        }
        alice
            .storage(|v| v.slots[0].is_some_and(|s| s.count == 30) && v.slots[1].is_some_and(|s| s.count == 5))
            .await;
        let untouched = [plain, plain.offset(0, -1, 0), coloured.offset(0, -1, 0)].map(|p| (p, state_at(&alice, p)));
        drop(alice);
        server.stop();
        (feet, shrine, [coloured, natural, chest, dug], untouched)
    };
    let [coloured, natural, chest, dug] = spots;

    // Her last visit was long ago.
    {
        let store = Store::open(&planet_file(dir.path())).unwrap();
        let data = store.player("alice").unwrap().unwrap();
        assert!(data.last_seen > 1_000_000, "leaving records the time");
        store
            .write(&WriteBatch {
                players: vec![(
                    "alice".into(),
                    PlayerData {
                        last_seen: 1_000_000,
                        ..data
                    },
                )],
                ..Default::default()
            })
            .unwrap();
    }

    // Phase 2: the server finds the claim lapsed and turns it, by itself.
    {
        let server = bm_server::start(config(dir.path())).unwrap();
        let mut bob = TestClient::register(&server, "Bob").await;
        if !bob.welcome.claims.is_empty() {
            bob.until(|m| matches!(m, ServerMsg::ClaimRemoved { .. })).await;
        }
        bob.wait_for_chunk(feet.chunk()).await;
        bob.settle().await;
        let ash = bob.block("ash");
        assert_eq!(
            state_at(&bob, coloured),
            BlockState::coloured(ash, Colour(17)),
            "its own colour"
        );
        assert_eq!(
            state_at(&bob, natural),
            natural_ash(&bob, "sand"),
            "nearest to its paint"
        );
        assert_eq!(state_at(&bob, chest), natural_ash(&bob, "storage_chest"));
        assert_eq!(state_at(&bob, feet), natural_ash(&bob, "claim_stone"), "the stone too");
        assert_eq!(state_at(&bob, dug), BlockState::AIR, "dug stays dug");
        for (p, was) in untouched {
            assert_eq!(state_at(&bob, p), was, "natural ground stays at {p:?}");
        }
        bob.wait_for_chunk(shrine.chunk()).await;
        assert_eq!(state_at(&bob, shrine).id, bob.block("warp_shrine"), "the shrine stands");
        // The land is free: Bob may build where the claim was.
        let carol = TestClient::register(&server, "Carol").await;
        assert!(carol.welcome.claims.is_empty());
        drop(bob);
        drop(carol);
        server.stop();
    }

    // Phase 3: after a restart Alice hears of it, and her things wait.
    let server = bm_server::start(config(dir.path())).unwrap();
    let mut alice = TestClient::login(&server, "Alice").await;
    alice.until(|m| matches!(m, ServerMsg::Reclaim { .. })).await;
    let lapsed = alice
        .notices
        .iter()
        .find(|n| n.starts_with("Your claim on Testworld lapsed after "))
        .cloned()
        .expect("told of the lapse");
    assert!(
        lapsed.ends_with("months away. Its stored items wait for you at any warp shrine."),
        "{lapsed}"
    );
    let stone = alice.item("stone");
    assert_eq!(
        alice.reclaim.clone().unwrap(),
        vec![ItemStack::new(stone, 30), ItemStack::coloured(stone, Colour(17), 5)]
    );
    assert_eq!(alice.count("stone"), 14);

    alice.wait_for_chunk(shrine.chunk()).await;
    alice.send(ClientMsg::CollectReclaim { shrine: feet }).await;
    alice
        .notice("Stand at a warp shrine to collect your reclaimed items")
        .await;
    assert_eq!(alice.count("stone"), 14, "nothing taken away from a shrine");

    alice.send(ClientMsg::CollectReclaim { shrine }).await;
    alice
        .until(|m| matches!(m, ServerMsg::Reclaim { items } if items.is_empty()))
        .await;
    alice.settle().await;
    assert_eq!(alice.count("stone"), 49);
    assert!(
        alice
            .inv
            .slots
            .iter()
            .flatten()
            .any(|s| s.colour == Colour(17) && s.count == 9),
        "coloured stone joins its colour"
    );
    alice.send(ClientMsg::CollectReclaim { shrine }).await;
    alice.notice("Nothing waits for you in reclaim").await;
    drop(alice);
    server.stop();

    // Read once, the notice is not told again.
    let server = bm_server::start(config(dir.path())).unwrap();
    let mut alice = TestClient::login(&server, "Alice").await;
    alice.settle().await;
    assert!(
        !alice.notices.iter().any(|n| n.contains("lapsed")),
        "{:?}",
        alice.notices
    );
    assert!(alice.reclaim.is_none(), "an empty reclaim is not sent");
    server.stop();
}

/// An owner online never lapses, however short the keep time; leaving starts
/// the clock, and the claim crumbles with everyone on the planet told.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_online_owner_never_lapses_and_leaving_starts_the_clock() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path(), "Alice", &[(5, "claim_stone", 1, 0), (1, "stone#33", 5, 0)]);
    let cfg = ServerConfig {
        claim_keep_seconds: Some(0),
        ..config(dir.path())
    };
    let server = bm_server::start(cfg).unwrap();
    let mut alice = TestClient::login(&server, "Alice").await;
    let mut bob = TestClient::register(&server, "Bob").await;
    let feet = alice.feet();
    alice.wait_for_chunk(feet.chunk()).await;
    bob.wait_for_chunk(feet.chunk()).await;
    let spot = spots_in_plot(&alice, feet)[0];
    assert!(alice.dig(1, feet).await.0);
    assert!(alice.edit(2, feet, EditAction::Place { slot: 5, mount: None }).await.0);
    assert!(alice.dig(3, spot).await.0);
    assert!(alice.edit(4, spot, EditAction::Place { slot: 1, mount: None }).await.0);

    // Several checks pass with her here.
    tokio::time::sleep(Duration::from_millis(2500)).await;
    let carol = TestClient::register(&server, "Carol").await;
    assert_eq!(carol.welcome.claims.len(), 1, "still claimed");
    drop(carol);
    bob.settle().await;
    assert_eq!(
        state_at(&bob, spot),
        BlockState::coloured(bob.block("stone"), Colour(33))
    );

    drop(alice);
    bob.until(|m| matches!(m, ServerMsg::ClaimRemoved { .. })).await;
    bob.heard(|l| l.text == "A claim on Testworld has crumbled to ash")
        .await;
    bob.settle().await;
    assert_eq!(state_at(&bob, spot), BlockState::coloured(bob.block("ash"), Colour(33)));
    assert_eq!(state_at(&bob, feet), natural_ash(&bob, "claim_stone"));

    let mut alice = TestClient::login(&server, "Alice").await;
    alice
        .notice("Your claim on Testworld lapsed after less than a month away.")
        .await;
    alice.settle().await;
    assert!(alice.reclaim.is_none(), "nothing was stored");
    server.stop();
}

/// A chisel edit: `octants` cut (or filled back, with `restore`) with the
/// chisel in hotbar `slot`.
fn chisel(slot: u8, octants: u8, restore: bool) -> EditAction {
    EditAction::Chisel { slot, octants, restore }
}

/// The removed octants and family of the block a client holds at `p`.
fn cut_at(c: &TestClient, p: BlockPos) -> (u8, Option<ShapeFamily>) {
    let s = state_at(c, p).shape;
    (s.removed(), s.family())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn chisels_cut_and_fill_back_by_the_rules() {
    let dir = tempfile::tempdir().unwrap();
    seed(
        dir.path(),
        "Alice",
        &[
            (0, "square_chisel", 1, 0),
            (1, "precise_square_chisel", 1, 0),
            (2, "slope_chisel", 1, 0),
            (3, "timber", 10, 0),
            (4, "square_chisel", 1, 299),
            (5, "stone_hammer", 1, 0),
            (6, "bevel_chisel", 1, 0),
        ],
    );
    let server = bm_server::start(config(dir.path())).unwrap();
    let mut alice = TestClient::login(&server, "Alice").await;
    let feet = alice.feet();
    alice.wait_for_chunk(feet.chunk()).await;
    let timber = alice.block("timber");

    // Natural ground takes only slopes.
    let ground = alice.chunks[&feet.chunk()].get(feet.local());
    assert_eq!(alice.welcome.blocks[ground.0 as usize].chisel, Chiselling::Slope);
    let beside = feet.offset(1, 0, 0);
    assert!(!alice.edit(1, beside, chisel(0, 0x03, false)).await.0);
    alice.notice("Natural ground takes only slope chisels").await;
    assert!(alice.edit(2, beside, chisel(2, 0x30, false)).await.0);
    assert_eq!(cut_at(&alice, beside), (0x30, Some(ShapeFamily::Slope)));

    // A timber block standing on the ground beside her takes any family.
    let t = feet.offset(1, 1, 0);
    assert!(alice.edit(3, t, EditAction::Place { slot: 3, mount: None }).await.0);
    assert_eq!(state_at(&alice, t), BlockState::plain(timber));
    // A precise chisel moves exactly one octant.
    assert!(!alice.edit(4, t, chisel(1, 0x03, false)).await.0, "two octants");
    assert!(alice.edit(5, t, chisel(1, 0x80, false)).await.0);
    assert_eq!(cut_at(&alice, t), (0x80, Some(ShapeFamily::Square)));
    assert_eq!(stack(&alice, 1).unwrap().wear, 1, "one use per cut");
    // An ordinary one moves an edge's two, never one or a diagonal.
    assert!(!alice.edit(6, t, chisel(0, 0x10, false)).await.0, "one octant");
    assert!(!alice.edit(7, t, chisel(0, 0x81, false)).await.0, "not an edge");
    assert!(alice.edit(8, t, chisel(0, 0x30, false)).await.0);
    assert_eq!(cut_at(&alice, t), (0xb0, Some(ShapeFamily::Square)));
    assert!(!alice.edit(9, t, chisel(0, 0x30, false)).await.0, "already cut");
    // Families do not mix, and only chisels chisel.
    assert!(!alice.edit(10, t, chisel(6, 0x03, false)).await.0);
    alice
        .notice("This block is already cut by another kind of chisel")
        .await;
    assert!(!alice.edit(11, t, chisel(5, 0x03, false)).await.0, "a hammer");
    assert!(!alice.edit(12, t, chisel(HAND, 0x03, false)).await.0, "a bare hand");
    // Down to a slab, then a quarter; the last of it cannot be cut away.
    assert!(alice.edit(13, t, chisel(1, 0x40, false)).await.0);
    assert_eq!(cut_at(&alice, t).0, 0xf0, "a half slab");
    assert!(alice.edit(14, t, chisel(0, 0x03, false)).await.0);
    assert!(!alice.edit(15, t, chisel(0, 0x0c, false)).await.0);
    alice.notice("That would cut the whole block away").await;
    assert_eq!(cut_at(&alice, t).0, 0xf3);

    // Filling back: only what is cut, and all of it makes the block whole.
    assert!(!alice.edit(16, t, chisel(0, 0x0c, true)).await.0, "nothing cut there");
    for (seq, m) in [(17, 0x03), (18, 0x30), (19, 0xc0)] {
        assert!(alice.edit(seq, t, chisel(0, m, true)).await.0, "restore {m:#x}");
    }
    assert_eq!(state_at(&alice, t), BlockState::plain(timber), "whole again");
    assert_eq!(stack(&alice, 0).unwrap().wear, 5, "restoring wears it too");

    // A worn-out chisel breaks on its last cut.
    assert!(alice.edit(20, t, chisel(4, 0x30, false)).await.0);
    let name = alice.items.get(alice.item("square_chisel")).unwrap().display.clone();
    alice.notice(&format!("Your {name} broke")).await;
    assert_eq!(stack(&alice, 4), None, "the broken chisel is gone");

    // Breaking a cut block gives back the whole block, once.
    let before = alice.count("timber");
    let (ok, now) = alice.dig(21, t).await;
    assert!(ok && now.is_air());
    assert_eq!(alice.count("timber"), before + 1);

    // The cuts were saved with the chunk (restart and read them back).
    drop(alice);
    server.stop();
    let server = bm_server::start(config(dir.path())).unwrap();
    let mut alice = TestClient::login(&server, "Alice").await;
    alice.wait_for_chunk(beside.chunk()).await;
    assert_eq!(cut_at(&alice, beside), (0x30, Some(ShapeFamily::Slope)));
    server.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn chisels_respect_claims() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path(), "Alice", &[(5, "claim_stone", 1, 0)]);
    seed(dir.path(), "Bob", &[(0, "slope_chisel", 1, 0)]);
    let server = bm_server::start(config(dir.path())).unwrap();
    let mut alice = TestClient::login(&server, "Alice").await;
    let feet = alice.feet();
    alice.wait_for_chunk(feet.chunk()).await;
    assert!(alice.dig(1, feet).await.0);
    assert!(alice.edit(2, feet, EditAction::Place { slot: 5, mount: None }).await.0);
    // Natural ground beside her, in the claim stone's plot.
    let t = spots_in_plot(&alice, feet)[0];
    let mut bob = TestClient::login(&server, "Bob").await;
    bob.wait_for_chunk(t.chunk()).await;
    assert!(!bob.edit(1, t, chisel(0, 0x30, false)).await.0);
    bob.notice("This land belongs to Alice").await;
    assert!(state_at(&bob, t).shape.is_full());
    assert_eq!(stack(&bob, 0).unwrap().wear, 0, "a refused cut costs nothing");
    server.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn players_walk_up_half_slabs_on_the_server() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path(), "Alice", &[(0, "square_chisel", 1, 0), (3, "timber", 4, 0)]);
    let server = bm_server::start(config(dir.path())).unwrap();
    let mut alice = TestClient::login(&server, "Alice").await;
    let feet = alice.feet();
    alice.wait_for_chunk(feet.chunk()).await;
    // Two half slabs along +X at body height, on the plaza.
    let mut seq = 0;
    for dx in [1, 2] {
        let t = feet.offset(dx, 1, 0);
        for action in [
            EditAction::Place { slot: 3, mount: None },
            chisel(0, 0x30, false),
            chisel(0, 0xc0, false),
        ] {
            seq += 1;
            assert!(alice.edit(seq, t, action).await.0);
        }
        assert_eq!(cut_at(&alice, t).0, 0xf0);
    }
    let (_, _, start, _) = alice.latest_snapshot().await;
    let (yaw, _) = PlayerInput::from_angles(std::f64::consts::FRAC_PI_2, 0.0);
    let walk = PlayerInput {
        forward: 1,
        yaw,
        ..Default::default()
    };
    for _ in 0..24 {
        alice.send_inputs(1, walk);
        tokio::time::sleep(Duration::from_millis(16)).await;
    }
    for _ in 0..30 {
        alice.send_inputs(
            1,
            PlayerInput {
                yaw,
                ..Default::default()
            },
        );
        tokio::time::sleep(Duration::from_millis(16)).await;
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    let (_, _, end, _) = alice.latest_snapshot().await;
    assert!(end.pos.x > start.pos.x + 1.0, "walked onto the slabs: {:?}", end.pos);
    assert!(
        (end.pos.y - (start.pos.y + 0.5)).abs() < 1e-9 && end.on_ground,
        "standing on a half slab: {:?} from {:?}",
        end.pos,
        start.pos
    );
    server.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn grapples_fire_stick_wear_and_need_a_grapple_in_hand() {
    let dir = tempfile::tempdir().unwrap();
    seed(
        dir.path(),
        "Alice",
        &[
            (0, "copper_grapple", 1, 0),
            (1, "iron_grapple", 1, 999),
            (2, "timber", 4, 0),
        ],
    );
    let server = bm_server::start(config(dir.path())).unwrap();
    let mut alice = TestClient::login(&server, "Alice").await;
    let feet = alice.feet();
    alice.wait_for_chunk(feet.chunk()).await;
    let down = |slot: u8, fire: bool| PlayerInput {
        pitch: -bm_core::movement::PITCH_LIMIT,
        slot,
        fire,
        ..Default::default()
    };
    async fn play(c: &mut TestClient, input: PlayerInput, ticks: u32) {
        for _ in 0..ticks {
            c.send_inputs(1, input);
            tokio::time::sleep(Duration::from_millis(16)).await;
        }
    }

    // Fired at the ground underfoot: it sticks at once and wears by one.
    play(&mut alice, down(0, true), 1).await;
    play(&mut alice, down(0, false), 20).await;
    alice.settle().await;
    let (_, _, you, _) = alice.latest_snapshot().await;
    let hook = you.hook.expect("the hook is out");
    assert!(hook.stuck.is_some(), "stuck in the ground: {hook:?}");
    assert!(hook.pos.y <= you.pos.y + 1e-9, "below the feet: {hook:?}");
    assert_eq!(stack(&alice, 0).map(|s| s.wear), Some(1));

    // Firing again lets go, and that wears it too, as in Boundless.
    play(&mut alice, down(0, true), 1).await;
    play(&mut alice, down(0, false), 10).await;
    alice.settle().await;
    let (_, _, you, _) = alice.latest_snapshot().await;
    assert_eq!(you.hook, None);
    assert_eq!(stack(&alice, 0).map(|s| s.wear), Some(2));

    // Timber in hand holds no rope: it lets go for free and fires nothing.
    play(&mut alice, down(0, true), 1).await;
    play(&mut alice, down(0, false), 10).await;
    play(&mut alice, down(2, true), 1).await;
    play(&mut alice, down(2, false), 10).await;
    alice.settle().await;
    let (_, _, you, _) = alice.latest_snapshot().await;
    assert_eq!(you.hook, None);
    assert_eq!(stack(&alice, 2).map(|s| (s.count, s.wear)), Some((4, 0)));
    assert_eq!(
        stack(&alice, 0).map(|s| s.wear),
        Some(3),
        "putting it away costs nothing"
    );

    // The iron grapple's last shot breaks it.
    play(&mut alice, down(1, true), 1).await;
    play(&mut alice, down(1, false), 10).await;
    alice.notice("Your Iron Grapple broke").await;
    alice.settle().await;
    assert_eq!(stack(&alice, 1), None);
    let (_, _, you, _) = alice.latest_snapshot().await;
    assert_eq!(you.hook, None, "nothing left in hand to hold it");
    server.stop();
}

/// Claims the plot under the player's feet with the claim stone in hotbar
/// `slot`, and the plots around it and above, so the ground for a few
/// blocks each way is theirs.
async fn claim_around(c: &mut TestClient, seq: u32, slot: u8) {
    let feet = c.feet();
    c.claim = None;
    assert!(c.dig(seq, feet).await.0);
    assert!(
        c.edit(seq + 1, feet, EditAction::Place { slot, mount: None }).await.0,
        "claimed"
    );
    if c.claim.is_none() {
        c.until(|m| matches!(m, ServerMsg::Claim(_))).await;
    }
    let id = c.claim.as_ref().unwrap().id;
    let shape = c.shape();
    let home = PlotPos::of(feet, shape);
    let mut n = 1;
    for dy in 0..=1 {
        for (dx, dz) in [
            (0, 0),
            (1, 0),
            (-1, 0),
            (0, 1),
            (0, -1),
            (1, 1),
            (1, -1),
            (-1, 1),
            (-1, -1),
        ] {
            if (dx, dy, dz) == (0, 0, 0) {
                continue;
            }
            c.send(ClientMsg::ClaimPlot {
                claim: id,
                plot: home.offset(dx, dy, dz).wrap(shape),
            })
            .await;
            n += 1;
            c.until(|m| matches!(m, ServerMsg::Claim(v) if v.plots.len() == n))
                .await;
        }
    }
}

/// A one-wide, two-tall portal of conduits (hotbar `slot`) standing on `ground`.
async fn conduit_column(c: &mut TestClient, seq: u32, ground: BlockPos, slot: u8) {
    for (i, dy) in [1, 2].into_iter().enumerate() {
        assert!(
            c.edit(
                seq + i as u32,
                ground.offset(0, dy, 0),
                EditAction::Place { slot, mount: None }
            )
            .await
            .0,
            "conduit placed"
        );
    }
}

/// The next portal window matching `pred`.
async fn portal_view(c: &mut TestClient, mut pred: impl FnMut(&PortalView) -> bool) -> PortalView {
    match c.until(|m| matches!(m, ServerMsg::Portal(Some(v)) if pred(v))).await {
        ServerMsg::Portal(Some(v)) => v,
        _ => unreachable!(),
    }
}

/// Opens the portal whose lowest corner is `pos` and returns its window.
async fn open_portal(c: &mut TestClient, pos: BlockPos) -> PortalView {
    c.send(ClientMsg::OpenPortal { pos: Some(pos) }).await;
    portal_view(c, |v| v.corner == pos).await
}

/// After travel, edits tagged with inputs from before it are refused: move
/// the inputs on past them.
async fn after_travel(c: &mut TestClient) {
    c.send_inputs(40, PlayerInput::default());
    tokio::time::sleep(Duration::from_millis(800)).await;
    c.settle().await;
}

async fn portal_do(c: &mut TestClient, pos: BlockPos, action: PortalAction) {
    c.send(ClientMsg::Portal { pos, action }).await;
}

/// The tag of a portal token in the pack not among `known`.
fn new_token(c: &TestClient, known: &[u32]) -> u32 {
    let token = c.item("portal_token");
    c.inv
        .slots
        .iter()
        .flatten()
        .find(|s| s.item == token && !known.contains(&s.tag))
        .expect("a new token")
        .tag
}

/// The slot of the first portal token in the pack bound to `tag`.
fn token_slot(c: &TestClient, tag: u32) -> u8 {
    let token = c.item("portal_token");
    c.inv
        .slots
        .iter()
        .position(|s| s.is_some_and(|s| s.item == token && s.tag == tag))
        .expect("token in the pack") as u8
}

/// Walks with heading `yaw` until a portal moves the player: the planet
/// travelled to (None for the same one) and where.
async fn walk_through(c: &mut TestClient, yaw: u16) -> (Option<u16>, PlayerState) {
    let input = PlayerInput {
        forward: 1,
        yaw,
        ..Default::default()
    };
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        c.send_inputs(1, input);
        tokio::time::sleep(Duration::from_millis(16)).await;
        while let Ok(m) = c.msgs.try_recv() {
            c.apply(&m);
            let moved = match &m {
                ServerMsg::Teleport { spawn, .. } => Some((None, *spawn)),
                ServerMsg::Travel(t) => Some((Some(t.planet.id), t.spawn)),
                _ => None,
            };
            if let Some(r) = moved {
                // Stand still on the far side.
                c.send_inputs(
                    10,
                    PlayerInput {
                        yaw,
                        ..Default::default()
                    },
                );
                c.settle().await;
                return r;
            }
        }
        assert!(tokio::time::Instant::now() < deadline, "no portal took us");
    }
}

/// Headings: 0 faces -Z, half a turn +Z.
const SOUTH: u16 = 32768;
const NORTH: u16 = 0;

fn feet_block(s: &PlayerState) -> BlockPos {
    BlockPos::containing(s.pos.x, s.pos.y + 0.01, s.pos.z)
}

/// Two portals on a claim, a token, a link, fuel: walking in comes out at
/// the other, and each traveller pays at most once per portal per cooldown.
/// Misshapen frames, conduits off a claim and conduits that would reshape a
/// portal are refused.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn portals_link_fuel_and_carry_players_paying_once_per_cooldown() {
    let dir = tempfile::tempdir().unwrap();
    seed(
        dir.path(),
        "Alice",
        &[
            (5, "claim_stone", 1, 0),
            (6, "portal_conduit", 20, 0),
            (1, "wayshard", 20, 0),
        ],
    );
    let server = bm_server::start(config(dir.path())).unwrap();
    let mut alice = TestClient::login(&server, "Alice").await;
    let feet = alice.feet();
    alice.wait_for_chunk(feet.chunk()).await;
    let shape = alice.shape();

    // Not on her claim (she has none yet).
    assert!(
        !alice
            .edit(1, feet.offset(1, 1, 0), EditAction::Place { slot: 6, mount: None })
            .await
            .0
    );
    alice.notice("Portal conduits go only on your own claim").await;
    claim_around(&mut alice, 2, 5).await;

    let a = feet.offset(0, 0, 2);
    let b = feet.offset(3, 0, -3);
    conduit_column(&mut alice, 10, a, 6).await;
    conduit_column(&mut alice, 12, b, 6).await;

    // An L of conduits is no portal.
    let l = feet.offset(-3, 0, 3);
    conduit_column(&mut alice, 14, l, 6).await;
    assert!(
        alice
            .edit(16, l.offset(-1, 1, 0), EditAction::Place { slot: 6, mount: None })
            .await
            .0
    );
    alice
        .send(ClientMsg::OpenPortal {
            pos: Some(l.offset(0, 1, 0)),
        })
        .await;
    alice.until(|m| matches!(m, ServerMsg::Portal(None))).await;
    alice.notice(bm_core::portal::SHAPE_RULE).await;

    // The first open makes the portal.
    let (a_up, b_up) = (a.offset(0, 1, 0), b.offset(0, 1, 0));
    let v = open_portal(&mut alice, a_up).await;
    assert_eq!(
        (v.corner, v.width, v.height, v.link.clone(), v.open, v.fuel),
        (a_up, 1, 2, None, false, 0)
    );
    assert_eq!((v.capacity, v.trip_cost, v.cooldown_hours), (500, 1, 1.0));

    // A conduit against it would change its shape.
    assert!(
        !alice
            .edit(17, a.offset(1, 1, 0), EditAction::Place { slot: 6, mount: None })
            .await
            .0
    );
    alice.notice("That would change the shape of a portal").await;

    // A token of A, named for what it opens.
    portal_do(&mut alice, a_up, PortalAction::MakeToken).await;
    alice.until(|m| matches!(m, ServerMsg::TokenNames { .. })).await;
    alice.settle().await;
    let a_id = new_token(&alice, &[]);
    assert_eq!(
        alice.token_names[&a_id],
        format!("1 by 2, Testworld, near {}, {}", a_up.x, a_up.z)
    );

    // Linked at B with it, both ways; the token is used up.
    open_portal(&mut alice, b_up).await;
    let slot = token_slot(&alice, a_id);
    portal_do(&mut alice, b_up, PortalAction::Link { slot }).await;
    let v = portal_view(&mut alice, |v| v.link.is_some()).await;
    assert_eq!(v.link.unwrap(), format!("Testworld, near {}, {}", a_up.x, a_up.z));
    alice.settle().await;
    assert_eq!(alice.count("portal_token"), 0);
    // B's own token cannot link B.
    portal_do(&mut alice, b_up, PortalAction::MakeToken).await;
    alice.until(|m| matches!(m, ServerMsg::TokenNames { .. })).await;
    alice.settle().await;
    let b_id = new_token(&alice, &[]);
    let slot = token_slot(&alice, b_id);
    portal_do(&mut alice, b_up, PortalAction::Link { slot }).await;
    alice.notice("That token is for this portal").await;
    assert_eq!(alice.count("portal_token"), 1, "nothing consumed");
    let v = open_portal(&mut alice, a_up).await;
    assert_eq!(v.link.unwrap(), format!("Testworld, near {}, {}", b_up.x, b_up.z));
    assert!(!v.open, "no fuel, no way through");

    // Only wayshards fuel it; five go into each side, and both open.
    portal_do(&mut alice, a_up, PortalAction::AddFuel { slot: 6, count: 5 }).await;
    alice.notice("Only wayshards fuel a portal").await;
    portal_do(&mut alice, a_up, PortalAction::AddFuel { slot: 1, count: 5 }).await;
    let v = portal_view(&mut alice, |v| v.fuel == 5).await;
    assert!(v.open);
    portal_do(&mut alice, b_up, PortalAction::AddFuel { slot: 1, count: 5 }).await;
    alice.settle().await;
    assert_eq!(alice.count("wayshard"), 10);
    let surface = alice.block("portal_surface");
    for p in [a_up, a_up.offset(0, 1, 0), b_up, b_up.offset(0, 1, 0)] {
        assert_eq!(state_at(&alice, p).id, surface, "open at {p:?}");
    }

    // Through A, out of B's far face; A's tank pays one.
    let (planet, there) = walk_through(&mut alice, SOUTH).await;
    assert_eq!(planet, None, "same planet");
    assert_eq!(feet_block(&there), shape.wrap_block(b.offset(0, 1, 1)));
    assert_eq!(there.vel, glam::DVec3::ZERO);
    assert_eq!(open_portal(&mut alice, a_up).await.fuel, 4);
    // Back through B (B pays), and through A again within the hour: free.
    let (_, there) = walk_through(&mut alice, NORTH).await;
    assert_eq!(feet_block(&there), shape.wrap_block(a.offset(0, 1, -1)));
    let (_, there) = walk_through(&mut alice, SOUTH).await;
    assert_eq!(feet_block(&there), shape.wrap_block(b.offset(0, 1, 1)));
    assert_eq!(open_portal(&mut alice, a_up).await.fuel, 4, "charged once");
    assert_eq!(open_portal(&mut alice, b_up).await.fuel, 4);
    server.stop();
}

/// A portal with fuel stands; its tank and link survive a restart, as do
/// tokens' names; emptied, a conduit breaks and the portal is gone.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn portals_survive_a_restart_and_break_only_when_empty() {
    let dir = tempfile::tempdir().unwrap();
    seed(
        dir.path(),
        "Alice",
        &[
            (5, "claim_stone", 1, 0),
            (6, "portal_conduit", 20, 0),
            (1, "wayshard", 20, 0),
        ],
    );
    let (a, b, a_id, b_id) = {
        let server = bm_server::start(config(dir.path())).unwrap();
        let mut alice = TestClient::login(&server, "Alice").await;
        let feet = alice.feet();
        alice.wait_for_chunk(feet.chunk()).await;
        claim_around(&mut alice, 1, 5).await;
        let (a, b) = (feet.offset(0, 1, 2), feet.offset(3, 1, -3));
        conduit_column(&mut alice, 10, a.offset(0, -1, 0), 6).await;
        conduit_column(&mut alice, 12, b.offset(0, -1, 0), 6).await;
        open_portal(&mut alice, a).await;
        portal_do(&mut alice, a, PortalAction::MakeToken).await;
        alice.until(|m| matches!(m, ServerMsg::TokenNames { .. })).await;
        alice.settle().await;
        let a_id = new_token(&alice, &[]);
        open_portal(&mut alice, b).await;
        let slot = token_slot(&alice, a_id);
        portal_do(&mut alice, b, PortalAction::Link { slot }).await;
        portal_view(&mut alice, |v| v.link.is_some()).await;
        portal_do(&mut alice, a, PortalAction::AddFuel { slot: 1, count: 7 }).await;
        // A token of each kept in the pack.
        portal_do(&mut alice, b, PortalAction::MakeToken).await;
        alice.until(|m| matches!(m, ServerMsg::TokenNames { .. })).await;
        alice.settle().await;
        let b_id = new_token(&alice, &[]);
        portal_do(&mut alice, a, PortalAction::MakeToken).await;
        alice.settle().await;
        assert_eq!(alice.count("portal_token"), 2);
        assert_eq!(alice.count("wayshard"), 13);
        drop(alice);
        server.stop();
        (a, b, a_id, b_id)
    };

    let server = bm_server::start(config(dir.path())).unwrap();
    let mut alice = TestClient::login(&server, "Alice").await;
    alice
        .until(|m| matches!(m, ServerMsg::TokenNames { names } if names.len() == 2))
        .await;
    assert_eq!(
        alice.token_names[&b_id],
        format!("1 by 2, Testworld, near {}, {}", b.x, b.z)
    );
    alice.wait_for_chunk(a.chunk()).await;
    alice.wait_for_chunk(b.chunk()).await;
    alice.settle().await;
    let v = open_portal(&mut alice, a).await;
    assert_eq!((v.fuel, v.open), (7, true));
    assert_eq!(v.link.unwrap(), format!("Testworld, near {}, {}", b.x, b.z));
    assert_eq!(state_at(&alice, a).id, alice.block("portal_surface"), "still open");
    assert_eq!(state_at(&alice, b).id, alice.block("portal_conduit"), "B has no fuel");

    // Unlinked, it closes; with fuel inside it still cannot be broken.
    portal_do(&mut alice, a, PortalAction::Unlink).await;
    portal_view(&mut alice, |v| v.link.is_none() && !v.open).await;
    alice.settle().await;
    let conduit = alice.block("portal_conduit");
    assert_eq!(state_at(&alice, a).id, conduit);
    assert_eq!(open_portal(&mut alice, b).await.link, None, "both ways");
    assert!(!alice.dig(30, a).await.0);
    alice.notice("Empty the portal's tank before breaking it").await;
    assert_eq!(state_at(&alice, a).id, conduit);

    // The owner takes the fuel back; then it breaks, and A's token names a
    // portal that is gone.
    open_portal(&mut alice, a).await;
    portal_do(&mut alice, a, PortalAction::TakeFuel { count: 100 }).await;
    portal_view(&mut alice, |v| v.fuel == 0).await;
    alice.settle().await;
    assert_eq!(alice.count("wayshard"), 20);
    let conduits = alice.count("portal_conduit");
    assert!(alice.dig(31, a).await.0, "an empty portal breaks");
    alice.settle().await;
    assert_eq!(alice.count("portal_conduit"), conduits + 1);
    assert_eq!(alice.token_names[&a_id], "a portal that is gone");
    // The one conduit left is no portal.
    alice
        .send(ClientMsg::OpenPortal {
            pos: Some(a.offset(0, 1, 0)),
        })
        .await;
    alice.notice(bm_core::portal::SHAPE_RULE).await;
    server.stop();
}

/// A portal on one planet linked to one on another: walking in is the
/// journey to the other planet, arriving at the far portal, and costs two.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn portals_carry_players_between_planets() {
    let dir = tempfile::tempdir().unwrap();
    seed(
        dir.path(),
        "Alice",
        &[
            (5, "claim_stone", 1, 0),
            (7, "claim_stone", 1, 0),
            (6, "portal_conduit", 20, 0),
            (1, "wayshard", 20, 0),
        ],
    );
    let cfg = ServerConfig {
        shrine_guard: 0,
        ..universe(dir.path())
    };
    let server = bm_server::start(cfg).unwrap();
    let mut alice = TestClient::login(&server, "Alice").await;
    let home = alice.feet();
    alice.wait_for_chunk(home.chunk()).await;
    claim_around(&mut alice, 1, 5).await;
    let a = home.offset(0, 1, 2);
    conduit_column(&mut alice, 30, a.offset(0, -1, 0), 6).await;
    open_portal(&mut alice, a).await;

    // On Dryside, B, and a token of it.
    let shrine = alice.shrine();
    alice.send(ClientMsg::Warp { shrine, planet: 1 }).await;
    alice.until(|m| matches!(m, ServerMsg::Travel(_))).await;
    after_travel(&mut alice).await;
    let there = alice.feet();
    let far_shrine = alice.shrine();
    alice.wait_for_chunk(there.chunk()).await;
    claim_around(&mut alice, 40, 7).await;
    let b = there.offset(3, 1, -3);
    conduit_column(&mut alice, 60, b.offset(0, -1, 0), 6).await;
    open_portal(&mut alice, b).await;
    portal_do(&mut alice, b, PortalAction::MakeToken).await;
    alice.until(|m| matches!(m, ServerMsg::TokenNames { .. })).await;
    alice.settle().await;
    let b_id = new_token(&alice, &[]);
    assert_eq!(
        alice.token_names[&b_id],
        format!("1 by 2, Dryside, near {}, {}", b.x, b.z)
    );

    // Home again: A is linked with B's token, and fuelled for one trip.
    let shrine = alice.shrine();
    alice.send(ClientMsg::Warp { shrine, planet: 0 }).await;
    alice.until(|m| matches!(m, ServerMsg::Travel(_))).await;
    after_travel(&mut alice).await;
    alice.wait_for_chunk(a.chunk()).await;
    alice.settle().await;
    open_portal(&mut alice, a).await;
    let slot = token_slot(&alice, b_id);
    portal_do(&mut alice, a, PortalAction::Link { slot }).await;
    let v = portal_view(&mut alice, |v| v.link.is_some()).await;
    assert_eq!(v.link.unwrap(), format!("Dryside, near {}, {}", b.x, b.z));
    assert_eq!(v.trip_cost, 2);
    portal_do(&mut alice, a, PortalAction::AddFuel { slot: 1, count: 3 }).await;
    let v = portal_view(&mut alice, |v| v.fuel == 3).await;
    assert!(v.open);
    alice.settle().await;
    assert_eq!(state_at(&alice, a).id, alice.block("portal_surface"));

    // Nobody is on Dryside: its chunks load for the trip.
    let (planet, spawn) = walk_through(&mut alice, SOUTH).await;
    assert_eq!(planet, Some(1));
    assert_eq!(feet_block(&spawn), b.offset(0, 0, 1));
    alice.wait_for_chunk(b.chunk()).await;
    alice.settle().await;
    assert_eq!(state_at(&alice, b).id, alice.block("portal_conduit"), "B is unfuelled");

    // Back by shrine: two were taken, and one is too few for another trip.
    alice
        .send(ClientMsg::Warp {
            shrine: far_shrine,
            planet: 0,
        })
        .await;
    alice.until(|m| matches!(m, ServerMsg::Travel(_))).await;
    alice.wait_for_chunk(a.chunk()).await;
    alice.settle().await;
    let v = open_portal(&mut alice, a).await;
    assert_eq!((v.fuel, v.open), (1, false));
    assert_eq!(state_at(&alice, a).id, alice.block("portal_conduit"), "dormant");
    server.stop();
}

/// A lapsing claim takes its portals with it: unlinked and gone, open
/// surfaces turned to ash like the conduits, the tanks' fuel in reclaim.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_lapsed_claims_portals_go_and_their_fuel_waits_in_reclaim() {
    let dir = tempfile::tempdir().unwrap();
    seed(
        dir.path(),
        "Alice",
        &[
            (5, "claim_stone", 1, 0),
            (6, "portal_conduit", 20, 0),
            (1, "wayshard", 20, 0),
        ],
    );
    let (a, b) = {
        let server = bm_server::start(config(dir.path())).unwrap();
        let mut alice = TestClient::login(&server, "Alice").await;
        let feet = alice.feet();
        alice.wait_for_chunk(feet.chunk()).await;
        claim_around(&mut alice, 1, 5).await;
        let (a, b) = (feet.offset(0, 1, 2), feet.offset(3, 1, -3));
        conduit_column(&mut alice, 10, a.offset(0, -1, 0), 6).await;
        conduit_column(&mut alice, 12, b.offset(0, -1, 0), 6).await;
        open_portal(&mut alice, a).await;
        portal_do(&mut alice, a, PortalAction::MakeToken).await;
        alice.until(|m| matches!(m, ServerMsg::TokenNames { .. })).await;
        alice.settle().await;
        let a_id = new_token(&alice, &[]);
        open_portal(&mut alice, b).await;
        let slot = token_slot(&alice, a_id);
        portal_do(&mut alice, b, PortalAction::Link { slot }).await;
        portal_do(&mut alice, a, PortalAction::AddFuel { slot: 1, count: 6 }).await;
        portal_do(&mut alice, b, PortalAction::AddFuel { slot: 1, count: 4 }).await;
        alice.settle().await;
        assert_eq!(state_at(&alice, a).id, alice.block("portal_surface"));
        assert_eq!(alice.count("wayshard"), 10);
        drop(alice);
        server.stop();
        (a, b)
    };
    {
        let store = Store::open(&planet_file(dir.path())).unwrap();
        let data = store.player("alice").unwrap().unwrap();
        store
            .write(&WriteBatch {
                players: vec![(
                    "alice".into(),
                    PlayerData {
                        last_seen: 1_000_000,
                        ..data
                    },
                )],
                ..Default::default()
            })
            .unwrap();
    }
    {
        let server = bm_server::start(config(dir.path())).unwrap();
        let mut bob = TestClient::register(&server, "Bob").await;
        if !bob.welcome.claims.is_empty() {
            bob.until(|m| matches!(m, ServerMsg::ClaimRemoved { .. })).await;
        }
        bob.wait_for_chunk(a.chunk()).await;
        bob.wait_for_chunk(b.chunk()).await;
        bob.settle().await;
        let ash = bob.block("ash");
        for p in [a, a.offset(0, 1, 0), b, b.offset(0, 1, 0)] {
            assert_eq!(state_at(&bob, p).id, ash, "ash at {p:?}");
        }
        drop(bob);
        server.stop();
    }
    let server = bm_server::start(config(dir.path())).unwrap();
    let mut alice = TestClient::login(&server, "Alice").await;
    alice.until(|m| matches!(m, ServerMsg::Reclaim { .. })).await;
    let wayshard = alice.item("wayshard");
    assert_eq!(alice.reclaim.clone().unwrap(), vec![ItemStack::new(wayshard, 10)]);
    alice.settle().await;
    assert!(
        alice
            .notices
            .iter()
            .any(|n| n.ends_with("Its stored items wait for you at any warp shrine.")),
        "{:?}",
        alice.notices
    );
    server.stop();
}

/// Places the item in hotbar `slot` at `pos`, on the face `mount` for a prop.
async fn place(
    c: &mut TestClient,
    seq: u32,
    pos: BlockPos,
    slot: u8,
    mount: Option<bm_core::Mount>,
) -> (bool, BlockId) {
    c.edit(seq, pos, EditAction::Place { slot, mount }).await
}

/// Milestone 9, acceptance 2 to 4 and 8: a copper hammer breaks stone in
/// three swings on a tier 1 world, everyone who has the chunk hears the
/// damage, a swing sooner than the tool's cadence is refused with the true
/// damage, damage heals after a while alone, the wrong tool deals a
/// quarter, and every swing wears the tool.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn copper_breaks_stone_in_three_swings_at_its_cadence_and_damage_heals() {
    let dir = tempfile::tempdir().unwrap();
    seed(
        dir.path(),
        "Alice",
        &[
            (0, "copper_hammer", 1, 0),
            (1, "stone", 10, 0),
            (2, "copper_shovel", 1, 0),
        ],
    );
    let server = bm_server::start(config(dir.path())).unwrap();
    let mut alice = TestClient::login(&server, "Alice").await;
    let mut bob = TestClient::register(&server, "Bob").await;
    assert_eq!(alice.planet.tier, 1, "the test planet is tier 1");
    let feet = alice.feet();
    let s = feet.offset(1, 1, 0);
    alice.wait_for_chunk(s.chunk()).await;
    bob.wait_for_chunk(s.chunk()).await;
    let stone = alice.block("stone");
    assert_eq!(alice.swing_ticks(0), 35, "0.59 s at action speed 30");

    // Three swings: 400, 800, then it breaks.
    assert!(place(&mut alice, 1, s, 1, None).await.0);
    assert_eq!(alice.swing(2, s, 0).await, Swung::Damaged(400));
    bob.until(|m| matches!(m, ServerMsg::BlockHealth { pos, damage: 400 } if *pos == s))
        .await;
    alice.ready();
    assert_eq!(alice.swing(3, s, 0).await, Swung::Damaged(800));
    alice.ready();
    assert_eq!(alice.swing(4, s, 0).await, Swung::Broke);
    bob.until(|m| matches!(m, ServerMsg::BlockChanged { pos, .. } if *pos == s))
        .await;
    alice.settle().await;
    assert_eq!(alice.count("stone"), 10, "gathered");

    // The cadence is exact on the input timeline: 34 inputs after a swing
    // is too soon, 35 is not.
    assert!(place(&mut alice, 5, s, 1, None).await.0);
    alice.ready();
    assert_eq!(alice.swing(6, s, 0).await, Swung::Damaged(400));
    alice.send_inputs(34, PlayerInput::default());
    assert_eq!(alice.swing(7, s, 0).await, Swung::Refused(stone, 400), "too fast");
    alice.send_inputs(1, PlayerInput::default());
    assert_eq!(alice.swing(8, s, 0).await, Swung::Damaged(800));

    // Left alone for longer than the heal time, it is whole again.
    tokio::time::sleep(Duration::from_secs_f64(bm_core::mining::HEAL_TICKS as f64 / 60.0 + 0.5)).await;
    alice.ready();
    assert_eq!(alice.swing(9, s, 0).await, Swung::Damaged(400), "healed");
    // A shovel is the wrong tool for stone: a quarter of its 400.
    alice.ready();
    assert_eq!(alice.swing(10, s, 2).await, Swung::Damaged(500));
    alice.settle().await;
    assert_eq!(stack(&alice, 0).unwrap().wear, 6, "one per accepted swing");
    assert_eq!(stack(&alice, 2).unwrap().wear, 1);
    server.stop();
}

/// An item catalogue with a tool no recipe makes: an area tool for every
/// block (forging, which gives areas in Boundless, does not exist yet).
fn area_items(dir: &Path) -> std::path::PathBuf {
    let path = dir.join("test-items.toml");
    std::fs::write(
        &path,
        format!(
            "{}\n[[item]]\nname = \"test_maul\"\nstack = 1\ntool = {{ damage = 5000, action_speed = 30.0, durability = 700, aoe = \"3x3\" }}\n",
            include_str!("../../../assets/items.toml")
        ),
    )
    .unwrap();
    path
}

/// Acceptance 6: an area tool breaks its whole shape, the same damage on
/// every block, but skips a block on someone else's claim without refusing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_area_tool_breaks_its_shape_but_skips_another_players_claim() {
    let dir = tempfile::tempdir().unwrap();
    let items = area_items(dir.path());
    seed(dir.path(), "Alice", &[(0, "test_maul", 1, 0)]);
    seed(
        dir.path(),
        "Bob",
        &[(0, "copper_shovel", 1, 0), (1, "claim_stone", 1, 0)],
    );
    let server = bm_server::start(ServerConfig {
        items: Some(items),
        ..config(dir.path())
    })
    .unwrap();
    let mut alice = TestClient::login(&server, "Alice").await;
    let mut bob = TestClient::login(&server, "Bob").await;
    let shape = alice.shape();
    let feet = alice.feet();
    // A plot boundary across x near the feet (plots are 8 wide): the hit
    // block is the first of its plot, and Bob claims the plot before it.
    let bx = (feet.x - 4..=feet.x + 3)
        .find(|x| x.rem_euclid(bm_core::PLOT_SIZE) == 0)
        .unwrap();
    let hit = BlockPos::new(bx, feet.y, feet.z);
    let theirs = hit.offset(-1, 0, 0);
    for c in [&mut alice, &mut bob] {
        for dx in -1..=1 {
            c.wait_for_chunk(hit.offset(dx, 0, -1).chunk()).await;
            c.wait_for_chunk(hit.offset(dx, 0, 1).chunk()).await;
        }
    }
    assert!(bob.dig_with(1, theirs, 0).await.0);
    assert!(place(&mut bob, 2, theirs, 1, None).await.0);
    alice.until(|m| matches!(m, ServerMsg::Claim(_))).await;
    let claimed = PlotPos::of(theirs, shape);
    let before: Vec<(BlockPos, BlockState)> = (-1..=1)
        .flat_map(|dx| (-1..=1).map(move |dz| hit.offset(dx, 0, dz)))
        .map(|p| (p, state_at(&alice, p)))
        .collect();

    // Looking straight down, the square lies flat around the hit block.
    alice.send_inputs(
        1,
        PlayerInput {
            pitch: i16::MIN,
            ..Default::default()
        },
    );
    assert_eq!(alice.swing(3, hit, 0).await, Swung::Broke);
    alice.settle().await;
    let mut skipped = 0;
    for (p, was) in before {
        if PlotPos::of(p, shape) == claimed {
            assert_eq!(state_at(&alice, p), was, "{p:?} is on Bob's claim");
            skipped += 1;
        } else {
            assert_eq!(state_at(&alice, p), BlockState::AIR, "{p:?} is in the square");
        }
    }
    assert!(skipped >= 1);
    assert_eq!(state_at(&alice, theirs).id, alice.block("claim_stone"));
    assert!(alice.notices.is_empty(), "nothing refused: {:?}", alice.notices);
    assert_eq!(stack(&alice, 0).unwrap().wear, 1, "one swing, one use");
    server.stop();
}

/// Acceptance 9 and 10: a torch stands on a floor or hangs on a wall, only
/// against something solid; it breaks in two swings of anything; breaking
/// what it hangs on drops it into the breaker's pack; and others see the
/// light of a torch held in the hand.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn torches_stand_and_hang_on_solid_blocks_and_fall_with_them() {
    let dir = tempfile::tempdir().unwrap();
    seed(
        dir.path(),
        "Alice",
        &[(0, "copper_hammer", 1, 0), (1, "stone", 10, 0), (2, "torch", 10, 0)],
    );
    let server = bm_server::start(config(dir.path())).unwrap();
    let mut alice = TestClient::login(&server, "Alice").await;
    let mut bob = TestClient::register(&server, "Bob").await;
    let feet = alice.feet();
    for dx in -1..=4 {
        for dz in -3..=4 {
            alice.wait_for_chunk(feet.offset(dx, 1, dz).chunk()).await;
        }
    }
    use bm_core::Mount;

    // On the floor beside her.
    let floor = feet.offset(1, 1, 0);
    assert_eq!(
        place(&mut alice, 1, floor, 2, Some(Mount::Floor)).await,
        (true, alice.block("torch"))
    );
    assert_eq!(alice.count("torch"), 9);
    // Not on top of a torch, nor without saying which face.
    assert!(
        !place(&mut alice, 2, floor.offset(0, 1, 0), 2, Some(Mount::Floor))
            .await
            .0
    );
    alice.notice("A torch needs something solid to stand on").await;
    assert!(!place(&mut alice, 3, feet.offset(2, 1, 0), 2, None).await.0);
    // Not hanging on air.
    assert!(
        !place(&mut alice, 4, feet.offset(3, 1, -2), 2, Some(Mount::East))
            .await
            .0
    );
    alice.notice("A torch needs something solid to hang on").await;
    // On the side of a stone block: hung on the block to its north.
    let wall = feet.offset(1, 1, 2);
    assert!(place(&mut alice, 5, wall, 1, None).await.0);
    let hung = wall.offset(0, 0, 1);
    assert_eq!(
        place(&mut alice, 6, hung, 2, Some(Mount::North)).await,
        (true, alice.block("torch_north"))
    );
    assert_eq!(alice.count("torch"), 8);

    // Two swings of a bare hand take a torch.
    assert_eq!(alice.swing(7, floor, HAND).await, Swung::Damaged(1));
    alice.ready();
    assert_eq!(alice.swing(8, floor, HAND).await, Swung::Broke);
    alice.settle().await;
    assert_eq!(alice.count("torch"), 9);

    // Breaking the stone drops the torch hung on it into her pack.
    assert!(alice.dig_with(9, wall, 0).await.0);
    assert_eq!(state_at(&alice, hung), BlockState::AIR, "the torch fell");
    assert_eq!(alice.count("torch"), 10, "and is back in the pack");
    assert_eq!(alice.count("stone"), 10);

    // Holding a torch lights her for others; putting it away does not.
    let alice_id = alice.welcome.player_id;
    let light = alice.items.get(alice.item("torch")).unwrap().held_light;
    assert!(light > 0);
    for (slot, want) in [(2, light), (HAND, 0)] {
        alice.send_inputs(
            3,
            PlayerInput {
                slot,
                ..Default::default()
            },
        );
        tokio::time::sleep(Duration::from_millis(300)).await;
        let (_, _, _, others) = bob.latest_snapshot().await;
        let seen = others.iter().find(|o| o.id == alice_id).expect("bob sees alice");
        assert_eq!(seen.light, want, "holding slot {slot}");
    }
    server.stop();
}

/// Milestone 10: every swing costs energy; with too little left the swing is
/// refused (it does nothing but restart the pause), and the bar refills
/// after the pause.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn swings_cost_energy_and_a_tired_swing_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    // A tool no recipe makes: 300 energy a swing, so a full bar of 600 holds
    // two swings.
    let items = dir.path().join("test-items.toml");
    std::fs::write(
        &items,
        format!(
            "{}\n[[item]]\nname = \"heavy_test_hammer\"\nstack = 1\ntool = {{ damage = 100, action_speed = 150.0, durability = 700, energy = 300 }}\n",
            include_str!("../../../assets/items.toml")
        ),
    )
    .unwrap();
    seed(dir.path(), "Alice", &[(0, "heavy_test_hammer", 1, 0)]);
    let server = bm_server::start(ServerConfig {
        items: Some(items),
        ..config(dir.path())
    })
    .unwrap();
    let mut alice = TestClient::login(&server, "Alice").await;
    let feet = alice.feet();
    alice.wait_for_chunk(feet.chunk()).await;
    let ground = alice.chunks[&feet.chunk()].get(feet.local());

    assert_eq!(alice.swing(1, feet, 0).await, Swung::Damaged(100));
    alice.ready();
    assert_eq!(alice.swing(2, feet, 0).await, Swung::Damaged(200));
    alice.ready();
    assert_eq!(
        alice.swing(3, feet, 0).await,
        Swung::Refused(ground, 200),
        "too tired: nothing happens"
    );
    let state = snapshot_after(&mut alice).await;
    assert_eq!(state.energy(), 0);

    // The pause (3.5 s) and the refill (2 s), at the real input rate.
    for _ in 0..12 {
        alice.send_inputs(30, PlayerInput::default());
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    let state = snapshot_after(&mut alice).await;
    assert_eq!(state.energy(), bm_core::energy::MAX_ENERGY, "rested");
    alice.ready();
    assert!(
        !matches!(alice.swing(4, feet, 0).await, Swung::Refused(..)),
        "rested, it swings again"
    );
    server.stop();
}

// Milestone 12: skills and progression.

/// A character at `level` with `marks`, and nothing spent.
fn at_level(level: u32, marks: u32) -> Character {
    Character {
        xp: xp_for_level(level),
        marks,
        ..Default::default()
    }
}

/// Waits for the chunks of a few blocks around `feet`.
async fn wait_around(c: &mut TestClient, feet: BlockPos, r: i64) {
    for dx in [-r, 0, r] {
        for dz in [-r, 0, r] {
            for dy in [-1, 0, 4] {
                c.wait_for_chunk(c.shape().wrap_block(feet.offset(dx, dy, dz)).chunk())
                    .await;
            }
        }
    }
}

/// Milestone 12, acceptance 1 and 7: breaking gives a block's mine XP,
/// placing gives its build XP only inside your own claim, a hand craft its
/// recipe's XP at once and a station craft when collected; a level up says
/// what it gave and adds the marks; the character is saved with the
/// account.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn xp_comes_from_breaking_building_and_crafting_and_levels_up() {
    let dir = tempfile::tempdir().unwrap();
    seed_character(
        dir.path(),
        "Alice",
        &[
            (1, "stone", 10, 0),
            (2, "sand", 4, 0),
            (3, "crafting_table", 1, 0),
            (4, "trunk", 1, 0),
            (5, "claim_stone", 1, 0),
        ],
        Character {
            xp: 480,
            ..Default::default()
        },
    );
    let server = bm_server::start(config(dir.path())).unwrap();
    let mut alice = TestClient::login(&server, "Alice").await;
    assert_eq!(alice.welcome.character.xp, 480, "the character arrives in the welcome");
    assert_eq!(
        alice.welcome.skills.len(),
        bm_core::skills::SkillTree::builtin().skills.len()
    );
    let feet = alice.feet();
    wait_around(&mut alice, feet, 3).await;
    let shape = alice.shape();
    // The data: Boundless's numbers (tuning 2.8, 5.5).
    assert_eq!((alice.mine_xp(alice.block("stone")), alice.build_xp("stone")), (2, 8));

    // Placed outside any claim: no XP.
    let out = shape.wrap_block(feet.offset(2, 1, 0));
    assert!(place(&mut alice, 1, out, 1, None).await.0);
    alice.settle().await;
    assert_eq!(alice.character.xp, 480, "no build XP outside a claim");
    // Broken: its mine XP.
    assert!(alice.dig(2, out).await.0);
    alice.until_xp(482).await;
    // A hand craft gives its recipe's XP at once (timber's is 0).
    let timber = alice.recipe("timber");
    craft_at(&mut alice, timber, 1, None).await;
    alice.settle().await;
    assert_eq!((alice.count("timber"), alice.character.xp), (2, 482));

    // Her claim: the ground she digs and the stone she claims with.
    let ground = alice.chunks[&feet.chunk()].get(feet.local());
    let claimed = 482 + alice.mine_xp(ground) + alice.build_xp("claim_stone");
    claim_around(&mut alice, 10, 5).await;
    alice.until_xp(claimed).await;
    assert_eq!(alice.character.level(), 1);
    // Building inside it: build XP, and level 2 (500 XP).
    let inside = shape.wrap_block(feet.offset(1, 1, 0));
    assert!(place(&mut alice, 20, inside, 1, None).await.0);
    alice.until_xp(claimed + 8).await;
    alice.notice("You reached level 2: 4 skill points and 100 marks").await;
    assert_eq!((alice.character.level(), alice.character.marks), (2, 100));

    // A station craft gives its XP to whoever collects it.
    let table = shape.wrap_block(feet.offset(-1, 1, 1));
    assert!(place(&mut alice, 21, table, 3, None).await.0);
    let with_table = claimed + 8 + alice.build_xp("crafting_table");
    alice.until_xp(with_table).await;
    let sandstone = alice.recipe("sandstone");
    craft_at(&mut alice, sandstone, 1, Some(table)).await;
    look_at_station(&mut alice, table, |v| !v.output.is_empty()).await;
    alice.settle().await;
    assert_eq!(alice.character.xp, with_table, "not before it is collected");
    alice.send(ClientMsg::CollectStation { pos: table }).await;
    alice.until_xp(with_table + 2).await;
    assert_eq!(alice.count("sandstone"), 4);

    // Saved with the account.
    let character = alice.character.clone();
    drop(alice);
    server.stop();
    let server = bm_server::start(config(dir.path())).unwrap();
    let alice = TestClient::login(&server, "Alice").await;
    assert_eq!(alice.welcome.character, character);
    server.stop();
}

/// A loaded, solid top block a few blocks from `feet` whose centre is
/// between `lo` and `hi` from the eye.
fn block_at_distance(c: &TestClient, feet: BlockPos, lo: f64, hi: f64) -> BlockPos {
    let shape = c.shape();
    let eye = c.spawn.eye(&c.welcome.movement);
    for dx in -7i64..=7 {
        for dz in -7i64..=7 {
            for dy in (-3i64..=3).rev() {
                let p = shape.wrap_block(feet.offset(dx, dy, dz));
                let Some(chunk) = c.chunks.get(&p.chunk()) else {
                    continue;
                };
                let above = shape.wrap_block(p.offset(0, 1, 0));
                let open = c
                    .chunks
                    .get(&above.chunk())
                    .is_some_and(|a| a.get(above.local()).is_air());
                if chunk.get(p.local()).is_air() || !open {
                    continue;
                }
                let centre = glam::DVec3::new(p.x as f64 + 0.5, p.y as f64 + 0.5, p.z as f64 + 0.5);
                let d = shape.delta(eye, centre).length();
                if d > lo && d < hi {
                    return p;
                }
            }
        }
    }
    panic!("no block between {lo} and {hi} away");
}

/// Milestone 12, acceptance 3, 5 and 6: spending, refusing (comes later,
/// either/or, no points, locked), cleanse and reset costs, pages and the
/// switch cooldown (free at home); Might adds damage, Deftness quickens the
/// swing, Grasp adds reach, Fine Hands lets an ordinary chisel cut one
/// octant.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn skills_are_spent_by_the_rules_and_change_swings_reach_and_chisels() {
    let dir = tempfile::tempdir().unwrap();
    seed_character(
        dir.path(),
        "Alice",
        &[
            (0, "square_chisel", 1, 0),
            (3, "timber", 4, 0),
            (5, "claim_stone", 1, 0),
        ],
        at_level(30, 2000),
    );
    let cfg = ServerConfig {
        // An untrained Boundless reach, so Grasp shows.
        reach: 4.0,
        ..config(dir.path())
    };
    let server = bm_server::start(cfg).unwrap();
    let mut alice = TestClient::login(&server, "Alice").await;
    let feet = alice.feet();
    wait_around(&mut alice, feet, 7).await;
    let shape = alice.shape();
    let tree = bm_core::skills::SkillTree::builtin();

    // Refusals change nothing and say why.
    alice
        .skill_refused(SkillAction::Learn("intelligence".into()), "That skill comes later")
        .await;
    alice
        .skill_refused(SkillAction::Learn("nonsense".into()), "There is no such skill")
        .await;
    alice.learn("hammer_epic", 1).await;
    alice
        .skill_refused(
            SkillAction::Learn("shovel_axe_epic".into()),
            "That cannot go on the same page as Rockbreaker",
        )
        .await;
    let mut bob = TestClient::register(&server, "Bob").await;
    bob.skill_refused(SkillAction::Learn("power".into()), "You have no skill points left")
        .await;
    bob.skill_refused(SkillAction::Learn("hammer_mastery".into()), "That unlocks at level 2")
        .await;
    drop(bob);

    // Grasp: a block beyond an untrained reach comes within it.
    let far = block_at_distance(&alice, feet, 5.2, 6.5);
    assert!(
        matches!(alice.swing(1, far, HAND).await, Swung::Refused(..)),
        "out of reach"
    );
    alice.learn("control", 3).await;
    assert_eq!(alice.character.effects(&tree).reach(), 6.25);
    alice.ready();
    assert_eq!(alice.swing(2, far, HAND).await, Swung::Damaged(200), "in reach");

    // Might: the hand stone's 200 becomes 210 (1.05, rounded up).
    let ground = alice.chunks[&feet.chunk()].get(feet.local());
    let near = shape.wrap_block(feet.offset(1, 0, 0));
    assert_eq!(
        alice.chunks[&near.chunk()].get(near.local()),
        ground,
        "the same ground beside her"
    );
    alice.learn("power", 1).await;
    alice.ready();
    assert_eq!(alice.swing(3, near, HAND).await, Swung::Damaged(210));
    // Deftness: 8 ranks (+16 action speed) swing sooner, and the server
    // takes the sooner swing.
    alice.learn("dexterity", 8).await;
    let hand = alice.items.swing_tool(Some(alice.item("hand_stone"))).unwrap();
    let skilled = bm_core::mining::skilled_tool(&hand, &alice.character.effects(&tree));
    let (old, new) = (
        bm_core::mining::swing_ticks(hand.action_speed),
        bm_core::mining::swing_ticks(skilled.action_speed),
    );
    assert!(new < old, "{new} < {old}");
    // The swing before waited the old cadence; the next waits the new one.
    alice.ready();
    assert_eq!(alice.swing(4, near, HAND).await, Swung::Damaged(420));
    alice.send_inputs(new - 1, PlayerInput::default());
    assert_eq!(
        alice.swing(5, near, HAND).await,
        Swung::Refused(ground, 420),
        "not sooner than the new cadence"
    );
    alice.send_inputs(1, PlayerInput::default());
    assert_eq!(
        alice.swing(6, near, HAND).await,
        Swung::Damaged(630),
        "at the quicker cadence"
    );

    // Fine Hands: an ordinary chisel cuts one octant.
    let t = shape.wrap_block(feet.offset(-1, 1, 1));
    assert!(alice.edit(6, t, EditAction::Place { slot: 3, mount: None }).await.0);
    assert!(
        !alice.edit(7, t, chisel(0, 0x80, false)).await.0,
        "one octant needs the epic"
    );
    alice.learn("chisel_epic", 1).await;
    assert!(alice.edit(8, t, chisel(0, 0x80, false)).await.0);
    assert_eq!(cut_at(&alice, t), (0x80, Some(ShapeFamily::Square)));
    assert!(
        alice.edit(9, t, chisel(0, 0x03, false)).await.0,
        "and still its own cut"
    );

    // From level 20 a cleanse uses a cleanse point, a reset 800 marks.
    let c = alice.skill(SkillAction::Cleanse("power".into())).await;
    assert_eq!((c.page().rank("power"), c.cleanses_used), (0, 1));
    let c = alice.skill(SkillAction::ResetPage).await;
    assert!(c.page().ranks.is_empty());
    assert_eq!(c.marks, 1200);
    // A page for 600; switching away from home waits 15 minutes.
    let c = alice.skill(SkillAction::BuyPage).await;
    assert_eq!((c.pages.len(), c.marks), (2, 600));
    let c = alice.skill(SkillAction::BuyPage).await;
    assert_eq!((c.pages.len(), c.marks), (3, 0));
    alice.skill_refused(SkillAction::BuyPage, "You need 600 marks").await;
    alice.learn("power", 2).await;
    let c = alice.skill(SkillAction::SwitchPage(1)).await;
    assert_eq!(c.active, 1);
    assert!(c.effects(&tree).power == 0.0, "the new page is empty");
    alice
        .skill_refused(SkillAction::SwitchPage(0), "You can switch pages again in 15 minutes")
        .await;
    // At home it is free.
    claim_around(&mut alice, 20, 5).await;
    let c = alice.skill(SkillAction::SwitchPage(0)).await;
    assert_eq!((c.active, c.page().rank("power")), (0, 2));
    server.stop();
}

/// A world for replaying movement from the chunks a client holds, ruled as
/// the server's is (invariant 3).
struct HeldWorld<'a> {
    c: &'a TestClient,
    solid: Vec<bool>,
    water: BlockId,
}

impl HeldWorld<'_> {
    fn state(&self, p: BlockPos) -> Option<BlockState> {
        let shape = self.c.shape();
        if !shape.contains_y(p.y) {
            return None;
        }
        let p = shape.wrap_block(p);
        self.c.chunks.get(&p.chunk()).map(|ch| ch.state(p.local()))
    }
}

impl bm_core::PhysicsWorld for HeldWorld<'_> {
    fn solid(&self, p: BlockPos) -> bool {
        p.y < 0 || self.state(p).is_none_or(|s| self.solid[s.id.0 as usize])
    }

    fn liquid(&self, p: BlockPos) -> bool {
        self.state(p).is_some_and(|s| s.id == self.water)
    }

    fn shape(&self, p: BlockPos) -> bm_core::Shape {
        self.state(p).map_or(bm_core::Shape::FULL, |s| s.shape)
    }
}

/// The next snapshot that has simulated input `seq`.
async fn snapshot_at(c: &mut TestClient, seq: u32) -> PlayerState {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let (_, ack, you, _) = c.latest_snapshot().await;
        if ack == seq {
            return you;
        }
        assert!(
            ack < seq && tokio::time::Instant::now() < deadline,
            "ack {ack}, wanted {seq}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Milestone 12, acceptance 6 (Swiftness) and the contract: a skill change
/// tagged with an input changes the body from the next input on, so the
/// server's walk is exactly the step a client predicts with the new body
/// from that input (no correction).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_body_changes_at_the_input_a_skill_follows() {
    let dir = tempfile::tempdir().unwrap();
    seed_character(dir.path(), "Alice", &[], at_level(10, 0));
    let server = bm_server::start(config(dir.path())).unwrap();
    let mut alice = TestClient::login(&server, "Alice").await;
    assert_eq!(alice.welcome.spawn.body, bm_core::Body::default(), "untrained");
    let feet = alice.feet();
    wait_around(&mut alice, feet, 12).await;
    alice.send_inputs(5, PlayerInput::default());
    let seq = alice.seq;
    let start = snapshot_at(&mut alice, seq).await;
    let s0 = alice.seq;

    // Eight ranks of Swiftness, following input s0 + 20 (sent ahead of it).
    let at = s0 + 20;
    for _ in 0..8 {
        alice
            .send(ClientMsg::Skill {
                at,
                action: SkillAction::Learn("agility".into()),
            })
            .await;
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    let (yaw, _) = PlayerInput::from_angles(std::f64::consts::FRAC_PI_2, 0.0);
    let walk = PlayerInput {
        forward: 1,
        yaw,
        ..Default::default()
    };
    for _ in 0..60 {
        alice.send_inputs(1, walk);
        tokio::time::sleep(Duration::from_millis(16)).await;
    }
    let seq = alice.seq;
    let end = snapshot_at(&mut alice, seq).await;
    let c = alice
        .until(|m| matches!(m, ServerMsg::Character { character, at: a } if *a == at && character.page().rank("agility") == 8))
        .await;
    let ServerMsg::Character { character, .. } = c else {
        unreachable!()
    };
    let tree = bm_core::skills::SkillTree::builtin();
    let body = character.effects(&tree).body();
    assert!((body.run_bonus - 2.4).abs() < 1e-9 && (body.sprint_bonus - 4.8).abs() < 1e-9);
    assert_eq!(end.body, body);

    // Replay: untrained to input s0 + 20, trained after.
    let world = HeldWorld {
        c: &alice,
        solid: alice.welcome.blocks.iter().map(|b| b.solid).collect(),
        water: alice.block("water"),
    };
    let replay = |switch: bool| {
        let mut s = start;
        for seq in s0 + 1..=s0 + 60 {
            bm_core::movement::step(&mut s, &walk, &alice.welcome.movement, None, alice.shape(), &world);
            if switch && seq == at {
                s.body = body;
            }
        }
        s
    };
    let predicted = replay(true);
    assert_eq!(end, predicted, "the server stepped exactly as predicted");
    let untrained = replay(false);
    let d = |s: &PlayerState| alice.shape().delta(start.pos, s.pos).length();
    assert!(
        d(&end) > d(&untrained) + 1.0,
        "faster: {} vs {}",
        d(&end),
        d(&untrained)
    );
    server.stop();
}

/// Milestone 12, acceptance 6: crafting levels gate recipes, by name.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_gated_recipe_is_refused_until_its_crafting_skill_is_learned() {
    let dir = tempfile::tempdir().unwrap();
    seed_character(
        dir.path(),
        "Alice",
        &[
            (0, "workbench", 4, 0),
            (11, "copper_ingot", 3, 0),
            (12, "stick", 4, 0),
            (13, "sackcloth", 1, 0),
            (14, "glue", 1, 0),
        ],
        at_level(4, 0),
    );
    let server = bm_server::start(config(dir.path())).unwrap();
    let mut alice = TestClient::login(&server, "Alice").await;
    let feet = alice.feet();
    wait_around(&mut alice, feet, 3).await;
    let shape = alice.shape();
    let bench = [(1, 1), (2, 1), (1, 2), (2, 2)].map(|(dx, dz)| shape.wrap_block(feet.offset(dx, 1, dz)));
    for (i, p) in bench.iter().enumerate() {
        assert!(place(&mut alice, 1 + i as u32, *p, 0, None).await.0);
    }
    let working = look_at_station(&mut alice, bench[0], |_| true).await;
    assert_eq!(working.idle, None);

    let hammer = alice.recipe("copper_hammer");
    craft_at(&mut alice, hammer, 1, Some(bench[0])).await;
    alice.notice("Needs Toolmaking 1").await;
    alice.settle().await;
    assert_eq!(alice.count("copper_ingot"), 3, "nothing taken");
    alice.learn("tool_crafting", 1).await;
    craft_at(&mut alice, hammer, 1, Some(bench[0])).await;
    let queued = alice.station(|v| !v.jobs.is_empty()).await;
    assert_eq!(queued.jobs[0].recipe, hammer);
    alice.settle().await;
    assert_eq!(alice.count("copper_ingot"), 0);
    server.stop();
}

/// Nathan, 2026-10-21: plots are bought with marks (30 each), and opening a
/// portal needs the portal epic, as in Boundless.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn marks_buy_plots_and_portals_need_the_portal_epic() {
    let dir = tempfile::tempdir().unwrap();
    seed_character(
        dir.path(),
        "Alice",
        &[(5, "claim_stone", 1, 0), (6, "portal_conduit", 4, 0)],
        at_level(10, 1000),
    );
    let cfg = ServerConfig {
        plot_allowance: 1,
        portal_epic_required: true,
        ..config(dir.path())
    };
    assert!(ServerConfig::default().portal_epic_required, "required by default");
    let server = bm_server::start(cfg).unwrap();
    let mut alice = TestClient::login(&server, "Alice").await;
    let feet = alice.feet();
    wait_around(&mut alice, feet, 3).await;
    let shape = alice.shape();

    assert!(alice.dig(1, feet).await.0);
    assert!(alice.edit(2, feet, EditAction::Place { slot: 5, mount: None }).await.0);
    if alice.claim.is_none() {
        alice.until(|m| matches!(m, ServerMsg::Claim(_))).await;
    }
    let id = alice.claim.as_ref().unwrap().id;
    let home = PlotPos::of(feet, shape);
    let above = home.offset(0, 1, 0).wrap(shape);
    alice.send(ClientMsg::ClaimPlot { claim: id, plot: above }).await;
    alice.notice("You have claimed all 1 of your plots").await;
    alice
        .skill_refused(SkillAction::BuyPlots(0), "You can buy 1 to 80 plots at once")
        .await;
    alice
        .skill_refused(SkillAction::BuyPlots(40), "You need 1200 marks")
        .await;
    let c = alice.skill(SkillAction::BuyPlots(1)).await;
    assert_eq!((c.plots_bought, c.marks), (1, 970));
    alice.send(ClientMsg::ClaimPlot { claim: id, plot: above }).await;
    alice
        .until(|m| matches!(m, ServerMsg::Claim(v) if v.plots.len() == 2))
        .await;

    // A portal on her claim, which she cannot open without the epic.
    let ground = (-2i64..=2)
        .flat_map(|dx| (-2i64..=2).map(move |dz| (dx, dz)))
        .filter(|&d| d != (0, 0))
        .map(|(dx, dz)| shape.wrap_block(feet.offset(dx, 0, dz)))
        .find(|g| {
            let p = PlotPos::of(*g, shape);
            (p.x, p.z) == (home.x, home.z)
                && !state_at(&alice, *g).id.is_air()
                && state_at(&alice, g.offset(0, 1, 0)).id.is_air()
                && state_at(&alice, g.offset(0, 2, 0)).id.is_air()
        })
        .expect("a spot for a portal on the claim");
    conduit_column(&mut alice, 10, ground, 6).await;
    let corner = ground.offset(0, 1, 0);
    alice.send(ClientMsg::OpenPortal { pos: Some(corner) }).await;
    alice.notice("Opening portals needs the Waywise skill").await;
    alice.until(|m| matches!(m, ServerMsg::Portal(None))).await;
    alice.learn("portal_epic", 1).await;
    let view = open_portal(&mut alice, corner).await;
    assert_eq!(view.corner, corner);
    // The welcome's allowance counts the plots bought.
    drop(alice);
    let alice = TestClient::login(&server, "Alice").await;
    assert_eq!(alice.welcome.plot_allowance, 2);
    server.stop();
}

/// Milestone 12: the light-source epic lights its owner for everyone, but
/// not in daylight under open sky.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_light_epic_shines_out_of_daylight() {
    let dir = tempfile::tempdir().unwrap();
    seed_character(dir.path(), "Alice", &[(1, "stone", 10, 0)], at_level(10, 0));
    let server = bm_server::start(config(dir.path())).unwrap();
    let mut alice = TestClient::login(&server, "Alice").await;
    let mut bob = TestClient::register(&server, "Bob").await;
    let feet = alice.feet();
    wait_around(&mut alice, feet, 3).await;
    let shape = alice.shape();
    let her = alice.welcome.player_id;
    async fn light_of(bob: &mut TestClient, her: u32, want: u8) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            let (_, _, _, others) = bob.latest_snapshot().await;
            if others.iter().any(|o| o.id == her && o.light == want) {
                return;
            }
            assert!(tokio::time::Instant::now() < deadline, "her light never became {want}");
        }
    }
    alice.learn("light_epic", 1).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    light_of(&mut bob, her, 0).await;
    // A roof over her head: a pillar beside her, and a block on top.
    for (i, (dx, dy)) in [(1, 1), (1, 2), (1, 3), (0, 3)].into_iter().enumerate() {
        let p = shape.wrap_block(feet.offset(dx, dy, 0));
        assert!(place(&mut alice, 1 + i as u32, p, 1, None).await.0, "roof block {i}");
    }
    light_of(&mut bob, her, 10).await;
    server.stop();
}

/// The player's state from the first snapshot that has applied every input
/// sent so far (a snapshot read straight away may predate them on a busy
/// machine).
async fn snapshot_after(c: &mut TestClient) -> PlayerState {
    let want = c.seq;
    for _ in 0..600 {
        let (_, ack, state, _) = c.latest_snapshot().await;
        if ack >= want {
            return state;
        }
    }
    panic!("no snapshot acknowledged input {want}");
}

// Milestone 13: creatures and combat.

/// Test creatures: a target that barely moves and never attacks, and a
/// charger that charges at once; each drops one bone.
const TEST_CREATURES: &str = r#"
[[creature]]
name = "target"
behaviour = "herd"
attack = "none"
half_width = 0.6
height = 1.4
walk_speed = 0.01
run_speed = 0.01
notice = 16.0
give_up = 64.0
forget = 120.0
spawn_weight = 1
group = [1, 1]

[[creature.tiers]]
health = 1500
armour = 0
xp = 60
drops_min = 1
drops_max = 1
drops = [{ item = "bone", weight = 1, max = 1 }]

[[creature]]
name = "charger"
behaviour = "herd"
attack = { charge = { speed = 12.0, range = 15.0, radius = 1.8, windup = 0.2, knockback = 9.0, cooldown = 3.0 } }
half_width = 0.6
height = 1.4
walk_speed = 1.4
run_speed = 12.0
notice = 16.0
give_up = 64.0
forget = 120.0
spawn_weight = 1
group = [1, 1]

[[creature.tiers]]
health = 1500
armour = 0
xp = 60
damage = 100
drops_min = 1
drops_max = 1
drops = [{ item = "bone", weight = 1, max = 1 }]

[[creature]]
name = "bomber"
behaviour = "exploder"
attack = { explode = { fuse = 0.5, radius = 4.0, knockback = 4.0, curl = 4.0, curl_chance = 0.0 } }
half_width = 0.4
height = 0.8
walk_speed = 0.01
run_speed = 0.01
notice = 16.0
give_up = 64.0
forget = 120.0
spawn_weight = 1
group = [1, 1]

[[creature.tiers]]
health = 5000
armour = 0
xp = 60
damage = 300
drops_min = 1
drops_max = 1
drops = [{ item = "bone", weight = 1, max = 1 }]

[[creature]]
name = "flyer"
behaviour = "flyer"
attack = { barrage = { bolt_speed = 18.0, range = 28.0, cooldown = 1.0, hover = 4.0, bomb_below = 0.0, bomb_radius = 3.0, homing_below = 0.0, beam_tier = 9, lift_tier = 9, lift_cooldown = 30.0 } }
half_width = 0.7
height = 1.3
walk_speed = 2.0
run_speed = 6.0
notice = 24.0
give_up = 64.0
forget = 120.0
spawn_weight = 1
group = [1, 1]

[[creature.tiers]]
health = 5000
armour = 0
xp = 60
damage = 100
drops_min = 1
drops_max = 1
drops = [{ item = "bone", weight = 1, max = 1 }]
"#;

/// `config` with the test creatures placed beside the arrival point (`kind`,
/// offset along +X) and no natural spawning.
fn creature_config(dir: &Path, placed: &[(&str, f64)]) -> ServerConfig {
    let at: Vec<(&str, [f64; 3])> = placed.iter().map(|(k, dx)| (*k, [*dx, 0.0, 0.0])).collect();
    creature_config_at(dir, &at)
}

/// `creature_config` with each creature at its own offset.
fn creature_config_at(dir: &Path, placed: &[(&str, [f64; 3])]) -> ServerConfig {
    let path = dir.join("test-creatures.toml");
    std::fs::write(&path, TEST_CREATURES).unwrap();
    ServerConfig {
        creatures: Some(path),
        creature_spawning: false,
        placed_creatures: placed
            .iter()
            .map(|(kind, offset)| bm_server::config::PlacedCreature {
                kind: kind.to_string(),
                tier: 1,
                offset: *offset,
                exotic: 0,
            })
            .collect(),
        ..config(dir)
    }
}

impl TestClient {
    /// Waits until a creature of `kind` has come into view; its id.
    async fn creature_of(&mut self, kind: &str) -> u32 {
        let k = self.welcome.creatures.iter().position(|d| d.name == kind).unwrap() as u16;
        loop {
            if let Some(v) = self.creatures.values().find(|v| v.kind == k) {
                return v.id;
            }
            self.until(|m| matches!(m, ServerMsg::Creature(_))).await;
        }
    }

    /// Waits until creature `id`'s view matches `pred`.
    async fn creature_until(&mut self, id: u32, mut pred: impl FnMut(&CreatureView) -> bool) -> CreatureView {
        loop {
            if let Some(v) = self.creatures.get(&id).filter(|v| pred(v)) {
                return v.clone();
            }
            self.until(|m| matches!(m, ServerMsg::Creature(v) if v.id == id)).await;
        }
    }

    /// The next snapshot carrying creature `id`, with the player's state.
    async fn snapshot_with(&mut self, id: u32) -> (RemoteCreature, PlayerState) {
        for _ in 0..600 {
            let snap = tokio::time::timeout(Duration::from_secs(5), self.snaps.recv())
                .await
                .unwrap()
                .unwrap();
            let ServerDatagram::Snapshot { you, creatures, .. } = snap;
            if let Some(c) = creatures.iter().find(|c| c.id == id) {
                return (*c, you);
            }
        }
        panic!("no snapshot carried creature {id}");
    }

    /// Fires the sling in `slot` along `yaw` and `pitch` at the sling's
    /// cadence, following the last input sent.
    async fn shoot(&mut self, slot: u8, yaw: u16, pitch: i16) {
        self.ready();
        if self.seq == 0 {
            self.send_inputs(1, PlayerInput::default());
        }
        let at = self.seq;
        let sling = self
            .inv
            .get(slot as usize)
            .and_then(|s| self.items.get(s.item))
            .and_then(|d| d.sling());
        let ticks = sling.map_or(1, |l| bm_core::mining::swing_ticks(l.action_speed));
        self.last_swing = Some((at, ticks));
        self.send(ClientMsg::Shoot {
            at,
            slot,
            yaw,
            pitch,
            charge: 0,
        })
        .await;
    }

    /// Throws a bomb from the bag in `slot`, held `hold` ticks, at the bag's
    /// cadence, following the last input sent.
    async fn throw(&mut self, slot: u8, yaw: u16, pitch: i16, hold: u16) {
        self.ready();
        if self.seq == 0 {
            self.send_inputs(1, PlayerInput::default());
        }
        let at = self.seq;
        let bag = self
            .inv
            .get(slot as usize)
            .and_then(|s| self.items.get(s.item))
            .and_then(|d| d.bomb());
        let ticks = bag.map_or(1, |b| bm_core::mining::swing_ticks(b.action_speed));
        self.last_swing = Some((at, ticks));
        self.send(ClientMsg::Throw {
            at,
            slot,
            yaw,
            pitch,
            hold,
        })
        .await;
    }

    /// Hits heard on `target`.
    fn hits_on(&self, target: Target) -> Vec<u32> {
        self.hits.iter().filter(|h| h.0 == target).map(|h| h.1).collect()
    }
}

/// Yaw and pitch from `eye` at `at`, lifted for a shot's drop at `speed`.
fn aim(shape: bm_core::PlanetShape, eye: glam::DVec3, at: glam::DVec3, speed: f64, falloff: f64) -> (u16, i16) {
    let d = shape.delta(eye, at);
    let flat = (d.x * d.x + d.z * d.z).sqrt();
    let t = flat / speed;
    let lift = 0.5 * bm_core::combat::SHOT_GRAVITY * falloff * t * t;
    PlayerInput::from_angles(d.x.atan2(-d.z), (d.y + lift).atan2(flat))
}

/// Milestone 13, acceptance 3, 5, 8: a stone sling kills a tier 1 creature
/// (the server flies every shot and judges the hit), every snapshot carries
/// it, the kill gives its XP, its body gives this player's drops once, and
/// the bone makes real glue at a furnace (1 sap + 1 bone).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_sling_kills_a_creature_for_xp_and_bone_and_bone_makes_glue() {
    let dir = tempfile::tempdir().unwrap();
    seed(
        dir.path(),
        "Alice",
        &[
            (0, "stone_sling", 1, 0),
            (1, "sap", 1, 0),
            (2, "furnace", 1, 0),
            (3, "furnace_crucible", 1, 0),
            (4, "coal", 1, 0),
        ],
    );
    let server = bm_server::start(creature_config(dir.path(), &[("target", 5.0)])).unwrap();
    let mut alice = TestClient::login(&server, "Alice").await;
    let shape = alice.shape();
    let feet = alice.feet();
    alice.wait_for_chunk(feet.chunk()).await;
    assert!(
        alice.welcome.creatures.iter().any(|d| d.name == "target"),
        "the catalogue arrives in the welcome"
    );
    let id = alice.creature_of("target").await;
    let view = alice.creatures[&id].clone();
    assert_eq!((view.tier, view.damage, view.dead, view.angry), (1, 0, false, false));

    // Snapshots carry it, beside where it was placed.
    let (c, you) = alice.snapshot_with(id).await;
    let at = glam::DVec3::new(c.pos[0] as f64, c.pos[1] as f64, c.pos[2] as f64);
    let d = shape.delta(you.pos, at);
    assert!(
        (d.x - 5.0).abs() < 1.0 && d.z.abs() < 1.0,
        "placed 5 blocks along +X: {d}"
    );

    // Five stone sling shots of 300 kill its 1500.
    let stone = alice.items.get(alice.item("stone_sling")).unwrap().sling().unwrap();
    let eye = you.pos + glam::DVec3::new(0.0, alice.welcome.movement.eye_height, 0.0);
    let (yaw, pitch) = aim(
        shape,
        eye,
        at + glam::DVec3::new(0.0, 0.7, 0.0),
        stone.shot_speed as f64,
        stone.falloff as f64,
    );
    for k in 1..=5 {
        alice.shoot(0, yaw, pitch).await;
        alice
            .until(|m| matches!(m, ServerMsg::Hit { target: Target::Creature(c), .. } if *c == id))
            .await;
        assert_eq!(alice.hits_on(Target::Creature(id)), vec![300; k], "shot {k}");
    }
    assert!(alice.shots.is_empty(), "never told of your own shots");
    let dead = alice.creature_until(id, |v| v.dead).await;
    assert_eq!(dead.damage, 1500);
    alice.until_xp(60).await;
    alice.settle().await;
    assert_eq!(alice.inv.get(0).unwrap().wear, 5, "one durability a shot");

    // The body: this player's drops, once.
    alice.send(ClientMsg::Loot { creature: id }).await;
    let looted = alice.creature_until(id, |v| v.looted).await;
    assert!(looted.dead);
    alice.settle().await;
    assert_eq!(alice.count("bone"), 1);
    alice.send(ClientMsg::Loot { creature: id }).await;
    alice.settle().await;
    assert_eq!(alice.count("bone"), 1, "a body gives each player one roll");

    // Real glue: 1 sap and 1 bone at a furnace, with one coal of heat.
    let base = shape.wrap_block(feet.offset(-1, 1, 1));
    let top = shape.wrap_block(feet.offset(-1, 2, 1));
    alice.wait_for_chunk(top.chunk()).await;
    assert!(place(&mut alice, 1, base, 2, None).await.0);
    assert!(place(&mut alice, 2, top, 3, None).await.0);
    let glue = alice.recipe("glue");
    let r = &alice.welcome.recipes[glue as usize];
    assert_eq!(r.inputs, vec![(alice.item("sap"), 1), (alice.item("bone"), 1)]);
    look_at_station(&mut alice, base, |v| v.idle.is_none()).await;
    craft_at(&mut alice, glue, 1, Some(base)).await;
    alice.station(|v| !v.jobs.is_empty()).await;
    fuel(&mut alice, base, 4, 1, true).await;
    // The craft takes 20 s once the coal burns.
    alice
        .station(|v| v.jobs.first().is_some_and(|j| j.progress > 0.0))
        .await;
    tokio::time::sleep(Duration::from_secs(15)).await;
    let done = look_at_station(&mut alice, base, |v| !v.output.is_empty()).await;
    assert_eq!((done.output[0].item, done.output[0].count), (alice.item("glue"), 1));
    alice.send(ClientMsg::CollectStation { pos: base }).await;
    alice.station(|v| v.output.is_empty()).await;
    alice.settle().await;
    assert_eq!(
        (alice.count("glue"), alice.count("bone"), alice.count("sap")),
        (1, 0, 0)
    );
    server.stop();
}

/// Milestone 13, acceptance 1 and 7: saved health loads; a creature's hit
/// at 0 health defeats the player, who gets Boundless's XP debt and
/// `Defeated`; Return brings them back on safe ground nearby at half health.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_defeated_player_owes_xp_and_returns_on_safe_ground_at_half_health() {
    let dir = tempfile::tempdir().unwrap();
    seed_hurt(dir.path(), "Alice", &[(0, "stone_sling", 1, 0)], at_level(10, 0), 550);
    let server = bm_server::start(creature_config(dir.path(), &[("charger", 6.0)])).unwrap();
    let mut alice = TestClient::login(&server, "Alice").await;
    assert_eq!(alice.spawn.hurt, 550, "health lost is saved");
    let me = alice.welcome.player_id;
    let feet = alice.feet();
    alice.wait_for_chunk(feet.chunk()).await;
    let id = alice.creature_of("charger").await;

    // A sling fired near a herd beast angers it; it rears and charges.
    alice.shoot(0, 0, bm_core::movement::PITCH_LIMIT).await;
    alice.creature_until(id, |v| v.angry).await;
    alice.until(|m| matches!(m, ServerMsg::Defeated { .. })).await;
    let want = bm_core::skills::defeat_debt(xp_for_level(10), false);
    assert_eq!(want, 750, "10% of what level 11 needs");
    assert_eq!(alice.defeated, Some((want, "Charger".to_string())));
    assert_eq!(alice.hits_on(Target::Player(me)), vec![100]);
    alice
        .until(|m| matches!(m, ServerMsg::Character { character, .. } if character.debt == 750))
        .await;
    // It stops attacking a defeated player.
    alice.creature_until(id, |v| !v.angry).await;

    // Lying where they fell (inputs move nothing).
    alice.send_inputs(
        60,
        PlayerInput {
            forward: 1,
            ..Default::default()
        },
    );
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let down = snapshot_after(&mut alice).await;
    assert!(down.down);
    assert_eq!(down.health(), 0);

    alice.send(ClientMsg::Return).await;
    let spawn = match alice.until(|m| matches!(m, ServerMsg::Teleport { .. })).await {
        ServerMsg::Teleport { spawn, .. } => spawn,
        _ => unreachable!(),
    };
    assert!(!spawn.down);
    assert_eq!(spawn.health(), 300, "half health");
    let shape = alice.shape();
    let d = shape.delta(down.pos, spawn.pos);
    let flat = (d.x * d.x + d.z * d.z).sqrt();
    assert!((7.0..=25.5).contains(&flat), "near where they fell: {flat}");
    // On safe ground: solid below, room (not water) where they stand.
    let at = BlockPos::containing(spawn.pos.x, spawn.pos.y, spawn.pos.z);
    for dy in [-1, 0, 1] {
        alice
            .wait_for_chunk(shape.wrap_block(at.offset(0, dy, 0)).chunk())
            .await;
    }
    let block = |c: &TestClient, p: BlockPos| {
        let p = shape.wrap_block(p);
        c.chunks[&p.chunk()].get(p.local())
    };
    let defs = &alice.welcome.blocks;
    assert!(
        defs[block(&alice, at.offset(0, -1, 0)).0 as usize].solid,
        "solid underfoot"
    );
    for y in 0..2 {
        let b = &defs[block(&alice, at.offset(0, y, 0)).0 as usize];
        assert!(!b.solid && b.name != "water", "room to stand");
    }
    // Moving again from there, at half health or more.
    alice.send_inputs(30, PlayerInput::default());
    let up = snapshot_after(&mut alice).await;
    assert!(!up.down);
    assert!(up.health() >= 300);
    server.stop();
}

/// Milestone 13, the agreed rules: a shot costs the sling's energy at its
/// input and one durability; one sooner than the cadence, or too tired,
/// fires nothing and wears nothing; others nearby are told the shot flies
/// and ends, the shooter is not.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shots_cost_energy_and_wear_and_others_see_them_fly() {
    let dir = tempfile::tempdir().unwrap();
    // A sling no recipe makes: 15000 damage, so 300 energy a shot (damage /
    // 50) and two shots a full bar.
    let items = dir.path().join("test-items.toml");
    std::fs::write(
        &items,
        format!(
            "{}\n[[item]]\nname = \"test_sling\"\nstack = 1\nsling = {{ damage = 15000, action_speed = 30.0, shot_speed = 30.0, range = 15.0, falloff = 0.5, durability = 10 }}\n",
            include_str!("../../../assets/items.toml")
        ),
    )
    .unwrap();
    seed(dir.path(), "Alice", &[(0, "test_sling", 1, 0)]);
    let server = bm_server::start(ServerConfig {
        items: Some(items),
        creature_spawning: false,
        ..config(dir.path())
    })
    .unwrap();
    let mut alice = TestClient::login(&server, "Alice").await;
    let mut bob = TestClient::register(&server, "Bob").await;
    let me = alice.welcome.player_id;
    let feet = alice.feet();
    alice.wait_for_chunk(feet.chunk()).await;
    bob.wait_for_chunk(feet.chunk()).await;
    let up = bm_core::movement::PITCH_LIMIT;
    let wear = |c: &TestClient| c.inv.get(0).unwrap().wear;

    alice.shoot(0, 0, up).await;
    let s = snapshot_after(&mut alice).await;
    assert_eq!(s.energy(), bm_core::energy::MAX_ENERGY - 300);
    let first = match bob.until(|m| matches!(m, ServerMsg::Shot(_))).await {
        ServerMsg::Shot(v) => v,
        _ => unreachable!(),
    };
    assert_eq!(first.from, Target::Player(me));
    assert_eq!((first.range, first.falloff), (15.0, 0.5));
    bob.until(|m| matches!(m, ServerMsg::ShotEnd { id } if *id == first.id))
        .await;
    alice.settle().await;
    assert_eq!(wear(&alice), 1);

    // Sooner than the cadence: nothing.
    let at = alice.seq;
    alice
        .send(ClientMsg::Shoot {
            at,
            slot: 0,
            yaw: 0,
            pitch: up,
            charge: 0,
        })
        .await;
    alice.settle().await;
    assert_eq!(wear(&alice), 1, "too soon: no wear");

    alice.shoot(0, 0, up).await;
    let s = snapshot_after(&mut alice).await;
    assert_eq!(s.energy(), 0);
    alice.settle().await;
    assert_eq!(wear(&alice), 2);
    // Too tired: nothing fires, nothing wears.
    alice.shoot(0, 0, up).await;
    snapshot_after(&mut alice).await;
    alice.settle().await;
    assert_eq!(wear(&alice), 2, "too tired: no wear");
    bob.settle().await;
    assert_eq!(bob.shots.len(), 2, "two shots flew");
    assert!(alice.shots.is_empty(), "never told of your own shots");
    // An empty hand shoots nothing.
    alice.shoot(5, 0, up).await;
    snapshot_after(&mut alice).await;
    bob.settle().await;
    assert_eq!(bob.shots.len(), 2);
    server.stop();
}

/// Milestone 13, acceptance 4: tools (the hand stone too) hit creatures with
/// their damage, on the swing cadence shared with slings and block swings,
/// within reach only; each strike wears the tool.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tools_strike_creatures_in_reach_on_the_swing_cadence() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path(), "Alice", &[]);
    let server = bm_server::start(creature_config(dir.path(), &[("target", 2.0), ("target", 20.0)])).unwrap();
    let mut alice = TestClient::login(&server, "Alice").await;
    let feet = alice.feet();
    alice.wait_for_chunk(feet.chunk()).await;
    let k = alice.welcome.creatures.iter().position(|d| d.name == "target").unwrap() as u16;
    while alice.creatures.values().filter(|v| v.kind == k).count() < 2 {
        alice.until(|m| matches!(m, ServerMsg::Creature(_))).await;
    }
    let ids: Vec<u32> = alice.creatures.keys().copied().collect();
    let (a, you) = alice.snapshot_with(ids[0]).await;
    let dx = alice
        .shape()
        .delta(
            you.pos,
            glam::DVec3::new(a.pos[0] as f64, a.pos[1] as f64, a.pos[2] as f64),
        )
        .x;
    let (near, far) = if dx < 10.0 { (ids[0], ids[1]) } else { (ids[1], ids[0]) };
    let hand_ticks = alice.swing_ticks(HAND);
    let strike = |at: u32, creature: u32| ClientMsg::Strike {
        at,
        creature,
        slot: HAND,
    };

    alice.send_inputs(1, PlayerInput::default());
    let at = alice.seq;
    alice.send(strike(at, near)).await;
    alice
        .until(|m| matches!(m, ServerMsg::Hit { target: Target::Creature(c), .. } if *c == near))
        .await;
    assert_eq!(
        alice.hits_on(Target::Creature(near)),
        vec![200],
        "the hand stone deals 200"
    );
    // Too soon: nothing.
    alice.send(strike(at + 1, near)).await;
    alice.send_inputs(hand_ticks - 1, PlayerInput::default());
    let _ = snapshot_after(&mut alice).await;
    alice.settle().await;
    assert_eq!(
        alice.hits_on(Target::Creature(near)).len(),
        1,
        "sooner than the cadence"
    );
    // Out of reach: nothing.
    alice.send_inputs(1, PlayerInput::default());
    alice.send(strike(alice.seq, far)).await;
    let _ = snapshot_after(&mut alice).await;
    alice.settle().await;
    assert!(alice.hits_on(Target::Creature(far)).is_empty(), "out of reach");
    // In time, it lands again.
    alice.send_inputs(hand_ticks, PlayerInput::default());
    alice.send(strike(alice.seq, near)).await;
    alice
        .until(|m| matches!(m, ServerMsg::Creature(v) if v.id == near && v.damage == 400))
        .await;
    alice.settle().await;
    assert_eq!(alice.inv.get(HAND as usize).unwrap().wear, 2, "a strike wears the tool");
    server.stop();
}

// Milestone 13 part 2: bombs, the popper, the drifter.

/// Where a creature stands now, from the next snapshot that carries it, and
/// the player's state.
async fn creature_at(c: &mut TestClient, id: u32) -> (glam::DVec3, PlayerState) {
    let (r, you) = c.snapshot_with(id).await;
    (glam::DVec3::new(r.pos[0] as f64, r.pos[1] as f64, r.pos[2] as f64), you)
}

/// Milestone 13 part 2, acceptance 2: a thrown copper bomb kills a creature
/// for its XP; each throw on the bag's cadence costs its energy and one use,
/// one sooner does nothing; others see the bomb fly, the thrower does not;
/// everyone near hears the blast, which never hurts a player.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_thrown_bomb_kills_a_creature_on_the_bags_cadence() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path(), "Alice", &[(0, "copper_bombs", 1, 0)]);
    let server = bm_server::start(creature_config(dir.path(), &[("target", 5.0)])).unwrap();
    let mut alice = TestClient::login(&server, "Alice").await;
    let mut bob = TestClient::register(&server, "Bob").await;
    let me = alice.welcome.player_id;
    let shape = alice.shape();
    let feet = alice.feet();
    alice.wait_for_chunk(feet.chunk()).await;
    bob.wait_for_chunk(feet.chunk()).await;
    let id = alice.creature_of("target").await;
    let (at, you) = creature_at(&mut alice, id).await;
    let bag = alice.items.get(alice.item("copper_bombs")).unwrap().bomb().unwrap();
    assert_eq!((bag.damage, bag.energy, bag.durability), (900, 90, 200));
    let eye = you.pos + glam::DVec3::new(0.0, alice.welcome.movement.eye_height, 0.0);
    let (yaw, pitch) = aim(
        shape,
        eye,
        at + glam::DVec3::new(0.0, 0.7, 0.0),
        bag.throw_speed as f64,
        0.5,
    );
    let full = bm_core::combat::THROW_FULL_TICKS as u16;
    let wear = |c: &TestClient| c.inv.get(0).unwrap().wear;

    alice.throw(0, yaw, pitch, full).await;
    let s = snapshot_after(&mut alice).await;
    assert_eq!(s.energy(), bm_core::energy::MAX_ENERGY - 90);
    alice
        .until(|m| matches!(m, ServerMsg::Hit { target: Target::Creature(c), .. } if *c == id))
        .await;
    let first = alice.hits_on(Target::Creature(id))[0];
    assert!(first >= 900, "a direct hit: {first}");
    alice.settle().await;
    assert_eq!(wear(&alice), 1, "one use a throw");
    assert_eq!(alice.blasts.len(), 1, "the thrower hears the blast");
    let flew = match bob.until(|m| matches!(m, ServerMsg::Shot(_))).await {
        ServerMsg::Shot(v) => v,
        _ => unreachable!(),
    };
    assert_eq!(
        (flew.from, flew.kind),
        (Target::Player(me), bm_protocol::ShotKind::Bomb)
    );
    bob.until(|m| matches!(m, ServerMsg::Blast { .. })).await;
    assert!(alice.shots.is_empty(), "never told of your own bomb");

    // Sooner than the cadence: nothing.
    let at_now = alice.seq;
    alice
        .send(ClientMsg::Throw {
            at: at_now,
            slot: 0,
            yaw,
            pitch,
            hold: full,
        })
        .await;
    alice.settle().await;
    assert_eq!(wear(&alice), 1, "too soon: no throw, no wear");
    assert_eq!(alice.blasts.len(), 1);

    // The second kills it.
    alice.throw(0, yaw, pitch, full).await;
    alice.creature_until(id, |v| v.dead).await;
    alice.until_xp(60).await;
    alice.settle().await;
    assert_eq!(wear(&alice), 2);
    bob.settle().await;
    assert!(alice.hits_on(Target::Player(me)).is_empty(), "bombs never hurt players");
    assert!(bob.hits.is_empty(), "nor anyone near");
    server.stop();
}

/// Milestone 13 part 2, acceptance 2: a blast breaks the blocks around it
/// that are not on someone else's claim, and they drop nothing and give no
/// XP.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_blast_breaks_unclaimed_blocks_for_nothing() {
    let dir = tempfile::tempdir().unwrap();
    // A bag no recipe makes: 6000 damage (600 energy, a full bar), so its
    // blast breaks even rock next to it.
    let items = dir.path().join("test-items.toml");
    std::fs::write(
        &items,
        format!(
            "{}\n[[item]]\nname = \"test_bombs\"\nstack = 1\nbomb = {{ damage = 6000, radius = 3.0, knockback = 10.0 }}\n",
            include_str!("../../../assets/items.toml")
        ),
    )
    .unwrap();
    seed(dir.path(), "Alice", &[(0, "test_bombs", 1, 0)]);
    seed(
        dir.path(),
        "Bob",
        &[(0, "copper_shovel", 1, 0), (1, "claim_stone", 1, 0)],
    );
    let server = bm_server::start(ServerConfig {
        items: Some(items),
        creature_spawning: false,
        ..config(dir.path())
    })
    .unwrap();
    let mut alice = TestClient::login(&server, "Alice").await;
    let mut bob = TestClient::login(&server, "Bob").await;
    let me = alice.welcome.player_id;
    let shape = alice.shape();
    let feet = alice.feet();
    // A plot boundary across x near the feet: Bob claims the plot before it.
    let bx = (feet.x - 4..=feet.x + 3)
        .find(|x| x.rem_euclid(bm_core::PLOT_SIZE) == 0)
        .unwrap();
    let hit = BlockPos::new(bx, feet.y, feet.z);
    let theirs = hit.offset(-1, 0, 0);
    for c in [&mut alice, &mut bob] {
        for dx in [-4, 0, 4] {
            for dy in [-4, 0, 4] {
                for dz in [-4, 0, 4] {
                    let p = hit.offset(dx, dy, dz);
                    if shape.contains_y(p.y) {
                        c.wait_for_chunk(shape.wrap_block(p).chunk()).await;
                    }
                }
            }
        }
    }
    assert!(bob.dig_with(1, theirs, 0).await.0);
    assert!(place(&mut bob, 2, theirs, 1, None).await.0);
    alice.until(|m| matches!(m, ServerMsg::Claim(_))).await;
    let claimed = PlotPos::of(theirs, shape);
    let near: Vec<BlockPos> = (-3..=3)
        .flat_map(|dx| (-3..=3).flat_map(move |dy| (-3..=3).map(move |dz| hit.offset(dx, dy, dz))))
        .filter(|p| shape.contains_y(p.y))
        .collect();
    let before: Vec<(BlockPos, BlockState)> = near.iter().map(|p| (*p, state_at(&alice, *p))).collect();
    let items_before: Vec<_> = (1..alice.inv.slots.len()).map(|i| stack(&alice, i)).collect();

    // Thrown onto the top of the boundary block.
    let eye = alice.spawn.pos + glam::DVec3::new(0.0, alice.welcome.movement.eye_height, 0.0);
    let top = glam::DVec3::new(hit.x as f64 + 0.5, hit.y as f64 + 1.0, hit.z as f64 + 0.5);
    let (yaw, pitch) = aim(shape, eye, top, 25.0, 0.5);
    alice
        .throw(0, yaw, pitch, bm_core::combat::THROW_FULL_TICKS as u16)
        .await;
    alice.until(|m| matches!(m, ServerMsg::Blast { .. })).await;
    alice.settle().await;
    let mut broken = 0;
    for (p, was) in before {
        let now = state_at(&alice, p);
        if PlotPos::of(p, shape) == claimed {
            assert_eq!(now, was, "{p:?} is on Bob's claim");
        } else if now != was {
            assert_eq!(now, BlockState::AIR, "{p:?} was broken");
            broken += 1;
        }
    }
    assert!(broken >= 1, "the blast broke something");
    assert_eq!(state_at(&alice, theirs).id, alice.block("claim_stone"));
    let items_after: Vec<_> = (1..alice.inv.slots.len()).map(|i| stack(&alice, i)).collect();
    assert_eq!(items_after, items_before, "blasted blocks drop nothing");
    assert_eq!(alice.character.xp, 0, "and give no XP");
    assert_eq!(stack(&alice, 0).unwrap().wear, 1);
    assert!(alice.hits_on(Target::Player(me)).is_empty());
    server.stop();
}

/// Milestone 13 part 2, acceptance 1: a popper hurt by a player whistles,
/// leaps and explodes: the blast hurts a player in the open, and not behind
/// a wall; its body stays.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_poppers_blast_hurts_a_player_but_not_through_a_wall() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path(), "Alice", &[(0, "timber", 4, 0)]);
    let server = bm_server::start(creature_config_at(
        dir.path(),
        &[("bomber", [0.0, 0.0, 3.0]), ("bomber", [3.0, 0.0, 0.0])],
    ))
    .unwrap();
    let mut alice = TestClient::login(&server, "Alice").await;
    let me = alice.welcome.player_id;
    let shape = alice.shape();
    let feet = alice.feet();
    alice.wait_for_chunk(feet.chunk()).await;
    let k = alice.welcome.creatures.iter().position(|d| d.name == "bomber").unwrap() as u16;
    while alice.creatures.values().filter(|v| v.kind == k).count() < 2 {
        alice.until(|m| matches!(m, ServerMsg::Creature(_))).await;
    }
    let ids: Vec<u32> = alice.creatures.keys().copied().collect();
    let (a, you) = creature_at(&mut alice, ids[0]).await;
    let (walled, open) = if shape.delta(you.pos, a).z > 1.5 {
        (ids[0], ids[1])
    } else {
        (ids[1], ids[0])
    };

    // A wall two blocks high between her and the one along +Z.
    for (seq, dy) in [(1, 1), (2, 2)] {
        let p = feet.offset(0, dy, 1);
        alice.wait_for_chunk(shape.wrap_block(p).chunk()).await;
        assert!(place(&mut alice, seq, p, 0, None).await.0, "wall block {dy}");
    }
    let hand = alice.swing_ticks(HAND);
    alice.send_inputs(1, PlayerInput::default());
    alice
        .send(ClientMsg::Strike {
            at: alice.seq,
            creature: walled,
            slot: HAND,
        })
        .await;
    alice.creature_until(walled, |v| v.angry).await;
    let gone = alice.creature_until(walled, |v| v.dead).await;
    assert_eq!(gone.damage, 5000, "it blew itself up");
    alice.settle().await;
    assert_eq!(alice.blasts.len(), 1);
    assert!(alice.hits_on(Target::Player(me)).is_empty(), "the wall stopped it");

    // In the open, it hurts.
    alice.send_inputs(hand, PlayerInput::default());
    alice
        .send(ClientMsg::Strike {
            at: alice.seq,
            creature: open,
            slot: HAND,
        })
        .await;
    alice
        .until(|m| matches!(m, ServerMsg::Hit { target: Target::Player(p), .. } if *p == me))
        .await;
    let hurt = alice.hits_on(Target::Player(me))[0];
    assert!((150..=300).contains(&hurt), "blast damage near its middle: {hurt}");
    alice.creature_until(open, |v| v.dead).await;
    assert_eq!(alice.blasts.len(), 2);
    // The body stays to be looted.
    alice.send(ClientMsg::Loot { creature: open }).await;
    alice.creature_until(open, |v| v.looted).await;
    server.stop();
}

/// Milestone 13 part 2, acceptance 3: an angered drifter takes off and its
/// bolts hit a player.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_drifters_bolt_hits_a_player() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path(), "Alice", &[]);
    let server = bm_server::start(creature_config(dir.path(), &[("flyer", 4.0)])).unwrap();
    let mut alice = TestClient::login(&server, "Alice").await;
    let me = alice.welcome.player_id;
    let feet = alice.feet();
    alice.wait_for_chunk(feet.chunk()).await;
    let id = alice.creature_of("flyer").await;
    alice.send_inputs(1, PlayerInput::default());
    alice
        .send(ClientMsg::Strike {
            at: alice.seq,
            creature: id,
            slot: HAND,
        })
        .await;
    alice.creature_until(id, |v| v.angry).await;
    alice
        .until(|m| matches!(m, ServerMsg::Hit { target: Target::Player(p), .. } if *p == me))
        .await;
    assert_eq!(alice.hits_on(Target::Player(me))[0], 100);
    let bolt = alice.shots.iter().find(|s| s.from == Target::Creature(id)).unwrap();
    assert_eq!((bolt.kind, bolt.falloff), (bm_protocol::ShotKind::Bolt, 0.0));
    // It flies (snapshots from before it took off may still be queued).
    let mut flying = false;
    for _ in 0..600 {
        let (r, _) = alice.snapshot_with(id).await;
        if r.pose == bm_protocol::Pose::Flying {
            flying = true;
            break;
        }
    }
    assert!(flying);
    server.stop();
}

// ------------------------------------------------------------ milestone 14

/// Milestone 14, acceptance 1: every planet has named regions; the first
/// region entered is discovered once, for 50 XP, and stays discovered after
/// logging back in.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn regions_are_named_and_discovered_once_for_xp() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = ServerConfig {
        discovery_xp: true,
        ..config(dir.path())
    };
    let server = bm_server::start(cfg.clone()).unwrap();
    let mut alice = TestClient::register(&server, "Alice").await;
    alice.until(|m| matches!(m, ServerMsg::Regions { .. })).await;
    let (seed, names, found) = alice.regions.clone().unwrap();
    let shape = alice.shape();
    let map = bm_core::regions::RegionMap::new(shape, seed);
    assert_eq!(names.len(), map.len(), "a name for every region");
    assert_eq!(map.len(), 1, "a 512-block test planet is one region");
    assert!(found.is_empty(), "nothing discovered before arriving");
    let here = map.locate(alice.spawn.pos.x, alice.spawn.pos.z) as u16;
    alice.until(|m| matches!(m, ServerMsg::Discovered { .. })).await;
    assert_eq!(alice.discovered, vec![(here, 1, 50)]);
    alice.until_xp(50).await;
    alice.settle().await;
    drop(alice);
    tokio::time::sleep(Duration::from_millis(500)).await;
    let mut again = TestClient::login(&server, "Alice").await;
    again.until(|m| matches!(m, ServerMsg::Regions { .. })).await;
    assert_eq!(again.regions.as_ref().unwrap().2, vec![here], "remembered");
    again.settle().await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    again.settle().await;
    assert!(again.discovered.is_empty(), "discovered only once");
    assert_eq!(again.character.xp, 50);
    server.stop();
}

/// Meteor waves for the tests: one harmless creature that walks up to the
/// player and dies to one stone sling shot.
const METEOR_CREATURES: &str = r#"
[[creature]]
name = "mote"
behaviour = "herd"
attack = "none"
half_width = 0.4
height = 0.9
walk_speed = 2.0
run_speed = 3.0
notice = 16.0
give_up = 96.0
forget = 120.0
spawn_weight = 1
group = [1, 1]

[[creature.tiers]]
health = 300
armour = 0
xp = 10
drops_min = 1
drops_max = 1
drops = [{ item = "bone", weight = 1, max = 1 }]
"#;

fn meteor_config(dir: &Path) -> ServerConfig {
    let path = dir.join("meteor-creatures.toml");
    std::fs::write(&path, METEOR_CREATURES).unwrap();
    ServerConfig {
        planets: vec![PlanetSpec::new("Wildtest", "lush", 7, 16)],
        creatures: Some(path),
        creature_spawning: false,
        placed_meteor: Some([7.0, 0.0]),
        ..config(dir)
    }
}

impl TestClient {
    /// Waits until meteor `id`'s view matches `pred`.
    async fn meteor_until(
        &mut self,
        mut pred: impl FnMut(&bm_protocol::MeteorView) -> bool,
    ) -> bm_protocol::MeteorView {
        loop {
            if let Some(v) = self.meteors.values().find(|v| pred(v)) {
                return *v;
            }
            self.until(|m| matches!(m, ServerMsg::Meteor(_))).await;
        }
    }

    /// Strikes every living creature that comes within reach with the tool
    /// in hotbar `melee` (they walk up to the player) until `done` says the
    /// fight is over. Idle inputs pass time, so energy comes back.
    async fn fight_until(&mut self, melee: u8, mut done: impl FnMut(&TestClient) -> bool) {
        let shape = self.shape();
        for _ in 0..600 {
            // About as many inputs as the 300 ms `settle` waits (60 a second),
            // so the server never falls behind them.
            self.send_inputs(18, PlayerInput::default());
            self.settle().await;
            if done(self) {
                return;
            }
            let living: Vec<u32> = self.creatures.values().filter(|v| !v.dead).map(|v| v.id).collect();
            if living.is_empty() {
                continue;
            }
            // From the newest snapshot, not the oldest queued.
            while self.snaps.try_recv().is_ok() {}
            let snap = tokio::time::timeout(Duration::from_secs(5), self.snaps.recv())
                .await
                .unwrap()
                .unwrap();
            let ServerDatagram::Snapshot { you, creatures, .. } = snap;
            let eye = you.pos + glam::DVec3::new(0.0, self.welcome.movement.eye_height, 0.0);
            let near = creatures.iter().filter(|c| living.contains(&c.id)).find(|c| {
                let at = glam::DVec3::new(c.pos[0] as f64, c.pos[1] as f64 + 0.45, c.pos[2] as f64);
                shape.delta(eye, at).length() < 7.0
            });
            let Some(c) = near else { continue };
            if self.last_swing.is_some_and(|(at, ticks)| self.seq < at + ticks) {
                continue;
            }
            let at = self.seq;
            self.last_swing = Some((at, self.swing_ticks(melee)));
            self.send(ClientMsg::Strike {
                at,
                creature: c.id,
                slot: melee,
            })
            .await;
        }
        panic!("the fight did not end");
    }
}

/// Milestone 14, acceptance 2 to 7: a meteor falls on a tier 3 planet, its
/// shell is guarded while its waves fight, lives are given from the first
/// kill, clearing every wave opens the heart, each player takes their own
/// share once (wayshards first), and a restart keeps the opened meteor with
/// its looters remembered.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_meteor_falls_fights_and_pays_each_player_once() {
    let dir = tempfile::tempdir().unwrap();
    seed(
        dir.path(),
        "Alice",
        &[(0, "stone_sling", 1, 0), (1, "iron_hammer", 1, 0)],
    );
    let cfg = meteor_config(dir.path());
    let server = bm_server::start(cfg.clone()).unwrap();
    let mut alice = TestClient::login(&server, "Alice").await;
    alice.wait_for_chunk(alice.feet().chunk()).await;
    let falling = alice
        .meteor_until(|v| matches!(v.phase, bm_protocol::MeteorPhase::Falling { .. }))
        .await;
    assert_eq!(falling.level, 1, "one player: level 1");
    let fighting = alice
        .meteor_until(|v| matches!(v.phase, bm_protocol::MeteorPhase::Fighting { .. }))
        .await;
    let bm_protocol::MeteorPhase::Fighting { wave, waves, left, .. } = fighting.phase else {
        unreachable!()
    };
    assert_eq!((wave, waves, left), (1, 3, 3), "level 1: three waves of three");
    let site = fighting.site;
    assert_eq!(state_at(&alice, site).id, alice.block("meteor_heart"));
    // The shell next to the heart, on the player's side.
    let shell = alice.shape().wrap_block(site.offset(-1, 0, 0));
    assert_eq!(state_at(&alice, shell).id, alice.block("meteor_shell"));
    alice.ready();
    let refused = alice.swing(900, shell, 1).await;
    assert!(matches!(refused, Swung::Refused(..)), "{refused:?}");
    alice
        .notice("The meteorite is guarded until its creatures are beaten")
        .await;
    alice.send(ClientMsg::LootHeart { pos: site }).await;
    alice.notice("The heart is sealed until its creatures are beaten").await;

    // Every wave, shot down; a life from the first kill.
    alice
        .fight_until(1, |c| {
            c.meteors
                .values()
                .any(|v| matches!(v.phase, bm_protocol::MeteorPhase::Cleared { .. }))
        })
        .await;
    alice
        .notice("The meteorite is beaten: break it open and take your share from its heart")
        .await;
    assert_eq!(alice.life, None, "lives end when it is cleared");

    // The heart: wayshards first, once.
    let before = alice.count("wayshard");
    alice.send(ClientMsg::LootHeart { pos: site }).await;
    alice.meteor_until(|v| v.looted).await;
    alice.settle().await;
    assert_eq!(alice.count("wayshard") - before, 7, "4 + 3 x level 1 on tier 3");
    assert!(
        alice
            .notices
            .iter()
            .any(|n| n.starts_with("From the meteor heart: 7 Wayshard")),
        "{:?}",
        alice.notices
    );
    alice.send(ClientMsg::LootHeart { pos: site }).await;
    alice.notice("You have taken your share of this meteor").await;
    alice.settle().await;
    assert_eq!(alice.count("wayshard") - before, 7, "once only");

    // The shell breaks now.
    alice.ready();
    let swung = alice.swing(901, shell, 1).await;
    assert!(!matches!(swung, Swung::Refused(..)), "{swung:?}");
    alice.settle().await;
    drop(alice);
    server.stop();

    // A restart: the opened meteor is still there, and still looted.
    let server = bm_server::start(ServerConfig {
        placed_meteor: None,
        ..cfg
    })
    .unwrap();
    let mut alice = TestClient::login(&server, "Alice").await;
    let v = alice.meteor_until(|v| v.site == site).await;
    assert!(
        matches!(v.phase, bm_protocol::MeteorPhase::Cleared { .. }),
        "{:?}",
        v.phase
    );
    assert!(v.looted, "looters are remembered");
    server.stop();
}
