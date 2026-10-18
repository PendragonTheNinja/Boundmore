//! End to end: real server, real QUIC, two headless players.

use bm_core::{BlockId, BlockPos, Chunk, ChunkPos, PlayerInput, PlayerState};
use bm_net::quinn::Connection;
use bm_net::{read_msg, write_msg};
use bm_protocol::{ClientDatagram, ClientMsg, EditAction, PROTOCOL_VERSION, ServerDatagram, ServerMsg, Welcome};
use bm_server::config::ServerConfig;
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;
use tokio::sync::mpsc;

struct TestClient {
    conn: Connection,
    send: bm_net::quinn::SendStream,
    msgs: mpsc::UnboundedReceiver<ServerMsg>,
    snaps: mpsc::UnboundedReceiver<ServerDatagram>,
    welcome: Welcome,
    chunks: HashMap<ChunkPos, Chunk>,
    seq: u32,
}

impl TestClient {
    async fn connect(server: &bm_server::RunningServer, name: &str) -> Self {
        let endpoint = bm_net::client_endpoint(&server.cert).unwrap();
        let addr = format!("127.0.0.1:{}", server.addr.port()).parse().unwrap();
        let conn = bm_net::connect(&endpoint, addr).await.unwrap();
        let (mut send, mut recv) = conn.open_bi().await.unwrap();
        write_msg(
            &mut send,
            &ClientMsg::Hello {
                protocol: PROTOCOL_VERSION,
                name: name.into(),
            },
        )
        .await
        .unwrap();
        let welcome = match read_msg::<ServerMsg>(&mut recv).await.unwrap() {
            Some(ServerMsg::Welcome(w)) => *w,
            other => panic!("expected welcome, got {other:?}"),
        };
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
        Self {
            conn,
            send,
            msgs,
            snaps,
            welcome,
            chunks: HashMap::new(),
            seq: 0,
        }
    }

    /// Reads reliable messages until `pred` matches one (chunks are kept).
    async fn until(&mut self, mut pred: impl FnMut(&ServerMsg) -> bool) -> ServerMsg {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
        loop {
            let m = tokio::time::timeout_at(deadline, self.msgs.recv())
                .await
                .expect("timed out waiting for a message")
                .expect("stream closed");
            match &m {
                ServerMsg::Chunk { pos, data } => {
                    self.chunks.insert(*pos, Chunk::decode(data).expect("valid chunk"));
                }
                ServerMsg::BlockChanged { pos, block } => {
                    if let Some(c) = self.chunks.get_mut(&pos.chunk()) {
                        c.set(pos.local(), *block);
                    }
                }
                _ => {}
            }
            if pred(&m) {
                return m;
            }
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

    async fn edit(&mut self, seq: u32, pos: BlockPos, action: EditAction) -> (bool, BlockId) {
        write_msg(&mut self.send, &ClientMsg::Edit { seq, pos, action })
            .await
            .unwrap();
        match self
            .until(|m| matches!(m, ServerMsg::EditResult { seq: s, .. } if *s == seq))
            .await
        {
            ServerMsg::EditResult { accepted, block, .. } => (accepted, block),
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
        view_height: 2,
        ..Default::default()
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_players_share_one_world() {
    let dir = tempfile::tempdir().unwrap();
    let server = bm_server::start(config(dir.path())).unwrap();

    let mut alice = TestClient::connect(&server, "Alice").await;
    let spawn = alice.welcome.spawn;
    assert!(alice.welcome.blocks.len() > 10, "catalogue arrives in the welcome");
    let feet = BlockPos::containing(spawn.pos.x, spawn.pos.y - 0.5, spawn.pos.z);
    alice.wait_for_chunk(feet.chunk()).await;
    let ground = alice.chunks[&feet.chunk()].get(feet.local());
    assert_ne!(ground, BlockId::AIR, "spawn stands on something");

    let mut bob = TestClient::connect(&server, "Bob").await;
    bob.wait_for_chunk(feet.chunk()).await;
    let joined = alice.until(|m| matches!(m, ServerMsg::PlayerJoined { .. })).await;
    assert!(matches!(joined, ServerMsg::PlayerJoined { ref name, .. } if name == "Bob"));

    // Alice digs the block she stands on; Bob sees it go.
    let (ok, block) = alice.edit(1, feet, EditAction::Break).await;
    assert!(ok, "break accepted");
    assert_eq!(block, BlockId::AIR);
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
    // Place it back.
    let stone = BlockId(alice.welcome.blocks.iter().position(|b| b.name == "stone").unwrap() as u16);
    // Nobody has sent an input yet, so nobody has fallen: the hole is empty
    // and can be filled. A player is only simulated by their own inputs.
    assert!(alice.edit(4, feet, EditAction::Place(stone)).await.0, "refill accepted");
    // But not where the players stand.
    let body = feet.offset(0, 1, 0);
    assert!(
        !alice.edit(5, body, EditAction::Place(stone)).await.0,
        "placing into a player is refused"
    );

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

    // Logging in again under the same name replaces the old session.
    let alice2 = TestClient::connect(&server, "Alice").await;
    assert_ne!(alice2.welcome.player_id, alice.welcome.player_id);
    bob.until(|m| matches!(m, ServerMsg::PlayerLeft { id } if *id == alice.welcome.player_id))
        .await;
    drop(alice);
    drop(bob);
    server.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn edits_and_positions_survive_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let (feet, moved_to) = {
        let server = bm_server::start(config(dir.path())).unwrap();
        let mut alice = TestClient::connect(&server, "Alice").await;
        let spawn = alice.welcome.spawn;
        let feet = BlockPos::containing(spawn.pos.x, spawn.pos.y - 0.5, spawn.pos.z);
        alice.wait_for_chunk(feet.chunk()).await;
        assert!(alice.edit(1, feet, EditAction::Break).await.0);
        tokio::time::sleep(Duration::from_millis(500)).await;
        let (_, _, state, _) = alice.latest_snapshot().await;
        drop(alice);
        server.stop();
        (feet, state.pos)
    };
    let server = bm_server::start(config(dir.path())).unwrap();
    let mut alice = TestClient::connect(&server, "Alice").await;
    assert!(
        (alice.welcome.spawn.pos - moved_to).length() < 1.0,
        "rejoined where she was ({:?} vs {:?})",
        alice.welcome.spawn.pos,
        moved_to
    );
    alice.wait_for_chunk(feet.chunk()).await;
    assert_eq!(
        alice.chunks[&feet.chunk()].get(feet.local()),
        BlockId::AIR,
        "the hole is still there"
    );
    server.stop();
}
