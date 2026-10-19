//! End to end: real server, real QUIC, headless players.

use bm_core::{
    BlockId, BlockPos, BlockRegistry, BlockState, Chunk, ChunkPos, Colour, Inventory, ItemRegistry, ItemStack,
    PlayerInput, PlayerState, PlotPos,
};
use bm_net::quinn::Connection;
use bm_net::{read_msg, write_msg};
use bm_protocol::{
    Channel, ChatLine, ClientDatagram, ClientMsg, EditAction, PROTOCOL_VERSION, ServerDatagram, ServerMsg, SlotRef,
    StationView, StorageView, Welcome,
};
use bm_server::config::{PlanetSpec, ServerConfig};
use bm_server::store::{PlayerData, SavedStack, Store, UNIVERSE_FILE, WriteBatch};
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;
use tokio::sync::mpsc;

const PASSWORD: &str = "correct horse";
/// Hand is an empty hotbar slot in every test.
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
    chunks: HashMap<ChunkPos, Chunk>,
    seq: u32,
    /// The planet the player is on now, and where they arrived on it.
    planet: bm_protocol::PlanetInfo,
    spawn: PlayerState,
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
        welcome,
        items,
        notices: Vec::new(),
        chat: Vec::new(),
        views: Default::default(),
        reclaim: None,
        chunks: HashMap::new(),
        seq: 0,
    })
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
        }) = self.snaps.try_recv()
        {
            (tick, ack, you, others) = (t, a, y, o);
        }
        (tick, ack, you, others)
    }

    async fn send(&mut self, msg: ClientMsg) {
        write_msg(&mut self.send, &msg).await.unwrap();
    }

    /// Holds break for `seconds` with hotbar `slot`, then breaks.
    async fn dig_with(&mut self, seq: u32, pos: BlockPos, slot: u8, seconds: f32) -> (bool, BlockId) {
        self.send(ClientMsg::StartBreak {
            at: self.seq,
            pos,
            slot,
        })
        .await;
        tokio::time::sleep(Duration::from_secs_f32(seconds)).await;
        self.edit(seq, pos, EditAction::Break).await
    }

    /// Holds break by hand for the block's hardness, then breaks it.
    async fn dig(&mut self, seq: u32, pos: BlockPos) -> (bool, BlockId) {
        let hardness = self
            .chunks
            .get(&pos.chunk())
            .map(|c| self.welcome.blocks[c.get(pos.local()).0 as usize].hardness)
            .unwrap_or(0.0);
        self.dig_with(seq, pos, HAND, hardness + 0.05).await
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
    let store = Store::open(&planet_file(dir)).unwrap();
    let account = bm_server::accounts::register(&store, name, PASSWORD).unwrap();
    let mut inventory = vec![None; bm_core::INVENTORY_SLOTS];
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
                },
            )],
            ..Default::default()
        })
        .unwrap();
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

    // Breaking without holding for the block's hardness is refused.
    assert!(
        !alice.edit(90, feet, EditAction::Break).await.0,
        "instant break refused"
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

    // Rules: bedrock cannot be broken; far blocks cannot be reached.
    let floor = BlockPos::new(feet.x, 0, feet.z);
    assert!(
        !alice.edit(2, floor, EditAction::Break).await.0,
        "out of reach / unbreakable"
    );
    let far = BlockPos::new(feet.x + 40, feet.y, feet.z);
    assert!(!alice.edit(3, far, EditAction::Break).await.0, "out of reach");
    // An empty slot places nothing.
    assert!(!alice.edit(6, feet, EditAction::Place { slot: HAND }).await.0);
    // Place it back, spending what was gathered. Nobody has sent an input
    // yet, so nobody has fallen: the hole is empty and can be filled.
    let (ok, placed) = alice
        .edit(
            4,
            feet,
            EditAction::Place {
                slot: gathered_slot as u8,
            },
        )
        .await;
    assert!(ok, "refill accepted");
    assert_eq!(alice.items.item_for_block(placed), Some(gathered));
    assert_eq!(alice.inv.count(gathered), 0, "placing spends");
    // But not where the players stand.
    let body = feet.offset(0, 1, 0);
    assert!(
        !alice.edit(5, body, EditAction::Place { slot: 1 }).await.0,
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
        alice.inv.slots.iter().flatten().map(|s| s.count).sum::<u16>(),
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
async fn full_inventories_refuse_breaks_and_tools_break_faster_and_wear_out() {
    let dir = tempfile::tempdir().unwrap();
    seed(
        dir.path(),
        "Alice",
        &[
            (0, "stone_hammer", 1, 0),
            (1, "stone", 10, 0),
            (3, "stone_hammer", 1, 149),
        ],
    );
    let full: Vec<(usize, &str, u16, u16)> = (0..40).map(|i| (i, "copper_ingot", 100, 0)).collect();
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
    // Making room lets it through.
    bob.send(ClientMsg::Discard { slot: 0 }).await;
    assert!(bob.dig(2, feet).await.0);
    drop(bob);

    let mut alice = TestClient::login(&server, "Alice").await;
    alice.wait_for_chunk(feet.chunk()).await;
    let stone = alice.block("stone");
    let hardness = alice.welcome.blocks[stone.0 as usize].hardness;
    let hammer_time = alice.items.break_seconds(
        &alice.welcome.blocks[stone.0 as usize],
        Some(alice.item("stone_hammer")),
    );
    assert!(hammer_time < hardness * 0.5);

    assert!(alice.edit(10, feet, EditAction::Place { slot: 1 }).await.0);
    // By hand, the hammer's time is far too short.
    assert!(
        !alice.dig_with(11, feet, HAND, hammer_time).await.0,
        "a bare hand is slower"
    );
    // With the hammer, an instant break is still too fast...
    assert!(
        !alice.dig_with(12, feet, 0, 0.0).await.0,
        "refused faster than the tool allows"
    );
    // ...but its own time is enough, and the stone is gathered.
    assert!(
        alice.dig_with(13, feet, 0, hammer_time + 0.05).await.0,
        "the hammer breaks stone faster"
    );
    assert_eq!(alice.count("stone"), 10);
    assert_eq!(stack(&alice, 0).unwrap().wear, 1, "a matching break wears the tool");

    // The worn-out hammer breaks on its last block.
    assert!(alice.edit(14, feet, EditAction::Place { slot: 1 }).await.0);
    assert!(alice.dig_with(15, feet, 3, hammer_time + 0.05).await.0);
    alice.notice("Your Stone Hammer broke").await;
    assert_eq!(stack(&alice, 3), None, "the broken tool is gone");
    server.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn crafting_by_hand_and_at_a_workbench() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path(), "Alice", &[(5, "trunk", 2, 0), (6, "stone", 3, 0)]);
    let server = bm_server::start(config(dir.path())).unwrap();
    let mut alice = TestClient::login(&server, "Alice").await;
    let feet = alice.feet();
    alice.wait_for_chunk(feet.chunk()).await;

    // Hand recipes are instant and all or nothing.
    let timber = alice.recipe("timber");
    alice
        .send(ClientMsg::Craft {
            recipe: timber,
            count: 3,
            station: None,
        })
        .await;
    alice.notice("You do not have the materials").await;
    assert_eq!(alice.count("trunk"), 2);
    alice
        .send(ClientMsg::Craft {
            recipe: timber,
            count: 2,
            station: None,
        })
        .await;
    alice.settle().await;
    assert_eq!((alice.count("trunk"), alice.count("timber")), (0, 8));
    let workbench = alice.recipe("workbench");
    alice
        .send(ClientMsg::Craft {
            recipe: workbench,
            count: 1,
            station: None,
        })
        .await;
    alice.settle().await;
    assert_eq!((alice.count("workbench"), alice.count("timber")), (1, 4));

    // A station recipe needs its station.
    let hammer = alice.recipe("stone_hammer");
    alice
        .send(ClientMsg::Craft {
            recipe: hammer,
            count: 1,
            station: None,
        })
        .await;
    alice.notice("That is made at a Workbench").await;

    // Set the workbench down where she stands, and queue a hammer.
    assert!(alice.dig(1, feet).await.0);
    let slot = (0..10)
        .find(|&i| stack(&alice, i).is_some_and(|s| s.item == alice.item("workbench")))
        .unwrap();
    assert!(alice.edit(2, feet, EditAction::Place { slot: slot as u8 }).await.0);
    alice.send(ClientMsg::WatchStation { pos: Some(feet) }).await;
    let empty = alice.station(|_| true).await;
    assert!(empty.jobs.is_empty() && empty.output.is_empty());
    alice
        .send(ClientMsg::Craft {
            recipe: hammer,
            count: 1,
            station: Some(feet),
        })
        .await;
    let queued = alice.station(|v| !v.jobs.is_empty()).await;
    assert_eq!(queued.jobs[0].recipe, hammer);
    alice.settle().await;
    assert_eq!(
        (alice.count("stone"), alice.count("timber")),
        (0, 2),
        "inputs taken at once"
    );

    // Progress arrives while it runs; it cannot be broken meanwhile.
    alice
        .station(|v| v.jobs.first().is_some_and(|j| j.progress > 0.1))
        .await;
    assert!(!alice.dig(3, feet).await.0, "a working station cannot be broken");
    alice.notice("Empty the Workbench before breaking it").await;

    let done = alice.station(|v| !v.output.is_empty()).await;
    assert!(done.jobs.is_empty());
    assert_eq!(done.output[0].item, alice.item("stone_hammer"));
    assert!(!alice.dig(4, feet).await.0, "nor while its output waits");

    alice.send(ClientMsg::CollectStation { pos: feet }).await;
    let emptied = alice.station(|v| v.output.is_empty()).await;
    assert!(emptied.jobs.is_empty());
    alice.settle().await;
    assert_eq!(alice.count("stone_hammer"), 1, "collected");
    assert!(alice.dig(5, feet).await.0, "an empty station breaks");
    assert_eq!(alice.count("workbench"), 1);
    server.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn station_jobs_continue_across_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path(), "Alice", &[(5, "workbench", 1, 0), (6, "stone", 8, 0)]);
    let feet = {
        let server = bm_server::start(config(dir.path())).unwrap();
        let mut alice = TestClient::login(&server, "Alice").await;
        let feet = alice.feet();
        alice.wait_for_chunk(feet.chunk()).await;
        assert!(alice.dig(1, feet).await.0);
        assert!(alice.edit(2, feet, EditAction::Place { slot: 5 }).await.0);
        let furnace = alice.recipe("furnace");
        alice.send(ClientMsg::WatchStation { pos: Some(feet) }).await;
        alice
            .send(ClientMsg::Craft {
                recipe: furnace,
                count: 1,
                station: Some(feet),
            })
            .await;
        alice.station(|v| !v.jobs.is_empty()).await;
        drop(alice);
        server.stop();
        feet
    };
    let server = bm_server::start(config(dir.path())).unwrap();
    let mut alice = TestClient::login(&server, "Alice").await;
    assert_eq!(alice.count("stone"), 0, "the inputs stay spent");
    alice.wait_for_chunk(feet.chunk()).await;
    alice.send(ClientMsg::WatchStation { pos: Some(feet) }).await;
    let resumed = alice.station(|_| true).await;
    assert_eq!(resumed.jobs.len(), 1, "the job survived");
    assert!(resumed.jobs[0].progress < 1.0);
    let done = alice.station(|v| !v.output.is_empty()).await;
    assert_eq!(done.output[0].item, alice.item("furnace"));
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
    assert!(alice.edit(2, feet, EditAction::Place { slot: 5 }).await.0);
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
    assert!(!bob.edit(1, feet, EditAction::Break).await.0);
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
            let state = PlayerState {
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
    assert!(alice.welcome.inventory.slots.iter().all(Option::is_none));
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
            state: Some(PlayerState {
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
/// the break of the first block waits for the input it follows, and the hold
/// on the next block, sent just after, must not overtake it (it did, and
/// every second block was refused and flashed back).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn digging_block_after_block_keeps_every_break() {
    let dir = tempfile::tempdir().unwrap();
    let server = bm_server::start(config(dir.path())).unwrap();
    let mut carol = TestClient::register(&server, "Carol").await;
    let first = carol.feet();
    let second = first.offset(0, -1, 0);
    carol.wait_for_chunk(first.chunk()).await;
    carol.wait_for_chunk(second.chunk()).await;
    let hardness =
        |c: &TestClient, p: BlockPos| c.welcome.blocks[c.chunks[&p.chunk()].get(p.local()).0 as usize].hardness;
    let (h1, h2) = (hardness(&carol, first), hardness(&carol, second));
    assert!(h1 > 0.0 && h2 > 0.0, "the ground takes time to break");

    // Inputs at the real rate, as the game sends them.
    async fn hold(c: &mut TestClient, seconds: f32) {
        for _ in 0..(seconds * 60.0).ceil() as u32 {
            c.send_inputs(1, PlayerInput::default());
            tokio::time::sleep(Duration::from_secs_f64(1.0 / 60.0)).await;
        }
    }
    carol
        .send(ClientMsg::StartBreak {
            at: carol.seq,
            pos: first,
            slot: HAND,
        })
        .await;
    hold(&mut carol, h1 + 0.1).await;
    // The break follows an input not sent yet; the next hold goes straight after.
    let at = carol.seq + 1;
    carol
        .send(ClientMsg::Edit {
            seq: 1,
            at,
            pos: first,
            action: EditAction::Break,
        })
        .await;
    carol
        .send(ClientMsg::StartBreak {
            at,
            pos: second,
            slot: HAND,
        })
        .await;
    hold(&mut carol, h2 + 0.1).await;
    carol
        .send(ClientMsg::Edit {
            seq: 2,
            at: carol.seq,
            pos: second,
            action: EditAction::Break,
        })
        .await;

    let mut results = Vec::new();
    while results.len() < 2 {
        if let ServerMsg::EditResult { seq, accepted, .. } =
            carol.until(|m| matches!(m, ServerMsg::EditResult { .. })).await
        {
            results.push((seq, accepted));
        }
    }
    assert_eq!(results, vec![(1, true), (2, true)], "both breaks kept");
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
            .edit(3, shrine.offset(0, 1, 0), EditAction::Place { slot: 1 })
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
    assert!(alice.edit(2, feet, EditAction::Place { slot: 5 }).await.0);
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
        assert!(alice.edit(2, feet, EditAction::Place { slot: 5 }).await.0);
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
    assert!(alice.edit(2, feet, EditAction::Place { slot: 5 }).await.0);
    assert!(alice.dig(3, beside).await.0);
    assert!(alice.edit(4, beside, EditAction::Place { slot: 6 }).await.0);
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
    assert!(bob.inv.slots.iter().all(Option::is_none), "nothing taken");

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
    assert!(alice.welcome.inventory.slots.iter().all(Option::is_none));

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
        assert!(alice.edit(2, feet, EditAction::Place { slot: 5 }).await.0, "claimed");
        let mut seq = 3;
        for (pos, slot) in [(coloured, 1), (natural, 2), (chest, 6)] {
            assert!(alice.dig(seq, pos).await.0);
            assert!(alice.edit(seq + 1, pos, EditAction::Place { slot }).await.0);
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
    assert!(alice.edit(2, feet, EditAction::Place { slot: 5 }).await.0);
    assert!(alice.dig(3, spot).await.0);
    assert!(alice.edit(4, spot, EditAction::Place { slot: 1 }).await.0);

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
