//! End to end: real server, real QUIC, headless players.

use bm_core::{
    BlockId, BlockPos, BlockRegistry, Chunk, ChunkPos, Inventory, ItemRegistry, ItemStack, PlayerInput, PlayerState,
    PlotPos,
};
use bm_net::quinn::Connection;
use bm_net::{read_msg, write_msg};
use bm_protocol::{
    ClientDatagram, ClientMsg, EditAction, PROTOCOL_VERSION, ServerDatagram, ServerMsg, StationView, Welcome,
};
use bm_server::config::ServerConfig;
use bm_server::store::{PlayerData, SavedStack, Store, WriteBatch};
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
    chunks: HashMap<ChunkPos, Chunk>,
    seq: u32,
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
        welcome,
        items,
        notices: Vec::new(),
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

    /// The block under the player's feet at spawn.
    fn feet(&self) -> BlockPos {
        let s = self.welcome.spawn.pos;
        BlockPos::containing(s.x, s.y - 0.5, s.z)
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
                let pos = self.welcome.planet.wrap_block(*pos);
                if let Some(c) = self.chunks.get_mut(&pos.chunk()) {
                    c.set(pos.local(), *block);
                }
            }
            ServerMsg::Slots { changes } => {
                for (slot, s) in changes {
                    self.inv.slots[*slot as usize] = *s;
                }
            }
            ServerMsg::Notice { text } => self.notices.push(text.clone()),
            _ => {}
        }
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
            mut tick,
            mut ack,
            mut you,
            mut others,
        } = tokio::time::timeout(Duration::from_secs(5), self.snaps.recv())
            .await
            .unwrap()
            .unwrap();
        while let Ok(ServerDatagram::Snapshot {
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
                (accepted, block)
            }
            _ => unreachable!(),
        }
    }

    async fn station(&mut self, mut pred: impl FnMut(&StationView) -> bool) -> StationView {
        match self.until(|m| matches!(m, ServerMsg::Station(v) if pred(v))).await {
            ServerMsg::Station(v) => v,
            _ => unreachable!(),
        }
    }
}

fn config(dir: &Path) -> ServerConfig {
    ServerConfig {
        bind: "127.0.0.1:0".parse().unwrap(),
        data_dir: dir.to_path_buf(),
        planet_name: "Testworld".into(),
        seed: 7,
        width_chunks: 16,
        height_chunks: 12,
        view_radius: 3,
        ..Default::default()
    }
}

fn planet_file(dir: &Path) -> std::path::PathBuf {
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
    let shape = alice.welcome.planet;
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
    use bm_server::store::PlanetMeta;
    let dir = tempfile::tempdir().unwrap();
    let cfg = config(dir.path());
    let pos = glam::DVec3::new(100.5, 300.0, 120.5);
    {
        // Exactly what save version 1 wrote.
        let db = redb::Database::create(planet_file(dir.path())).unwrap();
        let tx = db.begin_write().unwrap();
        {
            let meta = PlanetMeta {
                save_version: 1,
                generator_version: bm_worldgen::GENERATOR_VERSION,
                chunk_format: bm_core::CHUNK_FORMAT_VERSION,
                name: cfg.planet_name.clone(),
                shape: bm_core::PlanetShape::new(cfg.width_chunks, cfg.height_chunks),
                generation: bm_worldgen::PlanetGenConfig {
                    seed: cfg.seed,
                    ..Default::default()
                },
            };
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
    drop(alice);
    server.stop();
    let store = Store::open(&planet_file(dir.path())).unwrap();
    let meta = store
        .planet_meta(PlanetMeta {
            save_version: 0,
            generator_version: 0,
            chunk_format: 0,
            name: String::new(),
            shape: bm_core::PlanetShape::new(8, 1),
            generation: Default::default(),
        })
        .unwrap();
    assert_eq!(meta.save_version, bm_server::store::SAVE_VERSION);
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
