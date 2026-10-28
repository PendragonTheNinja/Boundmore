//! Guilds in play (milestone 25 part 2, `docs/milestones/25-guilds.md`):
//! the guild control's window, every guild action, members told of each
//! change, claims given to a guild. The rules are `bm_core::guilds`; the
//! records `crate::guilds`.

use super::*;
use crate::guilds::Alignment;
use bm_core::guilds::{self as rules, NAME_MAX, Rank, TAG_MAX};
use bm_protocol::{GuildAction, GuildControlView, GuildView, GuildsView, HelixView, MemberView};

impl Game {
    /// What a member of a claim's guild may do there (none if not a
    /// member, or the claim is not given to a guild).
    pub(super) fn guild_rights_on(&self, claim: u32, account: &str) -> u8 {
        self.guilds
            .alignment(claim)
            .and_then(|a| self.guilds.get(a.guild))
            .and_then(|g| g.member(account))
            .map_or(0, |m| m.claim_rights())
    }

    /// Our guilds as the window shows them.
    fn guilds_view(&self, account: &str) -> GuildsView {
        let guilds = self
            .guilds
            .of(account)
            .into_iter()
            .filter_map(|id| self.guilds.get(id))
            .map(|g| {
                let me = g.member(account).expect("a member");
                let leader = me.rank >= Rank::Controller;
                GuildView {
                    id: g.id,
                    name: g.name.clone(),
                    tag: g.tag.clone(),
                    coin: g.coin,
                    members: g
                        .members
                        .iter()
                        .map(|(acct, m)| MemberView {
                            name: m.name.clone(),
                            rank: m.rank,
                            rights: m.claim_rights(),
                            powers: m.powers(),
                            online: self.player_by_account(acct).is_some(),
                        })
                        .collect(),
                    asked: if me.powers() & rules::MEMBERSHIP != 0 {
                        g.asked.values().cloned().collect()
                    } else {
                        Vec::new()
                    },
                    log: if leader {
                        g.log.iter().map(|e| (e.time, e.text.clone())).collect()
                    } else {
                        Vec::new()
                    },
                    endeavour: g.endeavour,
                    buffs: g
                        .active(unix_now())
                        .map(|b| (b.kind, b.tier, b.until, b.members.contains(account)))
                        .collect(),
                    claims: {
                        let mut c: Vec<(u32, bool)> = self
                            .guilds
                            .claims_of(g.id)
                            .map(|c| (c, self.guilds.alignment(c).is_some_and(|a| a.controlled)))
                            .collect();
                        c.sort_unstable();
                        c
                    },
                }
            })
            .collect();
        GuildsView {
            guilds,
            primary: self.guilds.primary(account),
        }
    }

    pub(super) fn send_guilds(&self, id: u32) {
        if let Some(p) = self.players.get(&id) {
            p.out.send(&ServerMsg::Guilds(self.guilds_view(&p.account)));
        }
    }

    /// Every member playing hears of a change to guild `gid` (and `also`,
    /// someone no longer in it).
    pub(super) fn guild_changed(&self, gid: u32, also: Option<&str>) {
        let mut accounts: Vec<String> = self
            .guilds
            .get(gid)
            .map(|g| g.members.keys().cloned().collect())
            .unwrap_or_default();
        accounts.extend(also.map(str::to_string));
        for a in accounts {
            if let Some(oid) = self.player_by_account(&a) {
                self.send_guilds(oid);
            }
        }
        self.refresh_controls();
    }

    /// `account`'s guildmates who are playing see them come and go.
    pub(super) fn tell_guilds_of(&self, account: &str) {
        for gid in self.guilds.of(account) {
            self.guild_changed(gid, None);
        }
    }

    // ------------------------------------------------------------ control

    fn control_view(&self, id: u32, pos: BlockPos) -> Result<GuildControlView, Refusal> {
        let (canon, block) = self.block_in_reach(id, pos)?;
        if block != self.guild_control {
            return Err("not a guild control".into());
        }
        let p = &self.players[&id];
        let planet = &self.planets[p.planet];
        let mine = planet.claims.at_block(canon).is_some_and(|c| c.owner == p.account);
        let gid = self.guilds.control(planet.id, canon);
        let guild = gid
            .and_then(|g| self.guilds.get(g))
            .map(|g| (g.id, g.name.clone(), g.tag.clone(), g.members.len() as u32));
        let asked = gid
            .and_then(|g| self.guilds.get(g))
            .is_some_and(|g| g.asked.contains_key(&p.account));
        Ok(GuildControlView {
            pos: canon,
            guild,
            mine,
            asked,
            closed: false,
        })
    }

    pub(super) fn open_guild_control(&mut self, id: u32, pos: Option<BlockPos>) {
        let Some(pos) = pos else {
            if let Some(p) = self.players.get_mut(&id) {
                p.guild_control = None;
            }
            return;
        };
        match self.control_view(id, pos) {
            Ok(v) => {
                let p = self.players.get_mut(&id).expect("checked");
                p.guild_control = Some(v.pos);
                p.out.send(&ServerMsg::GuildControl(v));
                self.send_guilds(id);
            }
            Err(r) => {
                if let Refusal::Told(text) = &r {
                    self.notice(id, text);
                }
                self.send_closed_control(id, pos);
            }
        }
    }

    fn send_closed_control(&self, id: u32, pos: BlockPos) {
        if let Some(p) = self.players.get(&id) {
            p.out.send(&ServerMsg::GuildControl(GuildControlView {
                pos,
                guild: None,
                mine: false,
                asked: false,
                closed: true,
            }));
        }
    }

    /// Open controls see what changed, or close when out of reach or gone.
    pub(super) fn refresh_controls(&self) {
        for (&id, p) in &self.players {
            let Some(pos) = p.guild_control else { continue };
            match self.control_view(id, pos) {
                Ok(v) => p.out.send(&ServerMsg::GuildControl(v)),
                Err(_) => self.send_closed_control(id, pos),
            }
        }
    }

    pub(super) fn check_open_controls(&mut self) {
        let open: Vec<(u32, BlockPos)> = self
            .players
            .iter()
            .filter_map(|(id, p)| p.guild_control.map(|s| (*id, s)))
            .collect();
        for (id, pos) in open {
            if self.control_view(id, pos).is_err() {
                if let Some(p) = self.players.get_mut(&id) {
                    p.guild_control = None;
                }
                self.send_closed_control(id, pos);
            }
        }
    }

    /// A guild control is gone: its setting with it.
    pub(super) fn control_broken(&mut self, pi: usize, canon: BlockPos) {
        let planet = self.planets[pi].id;
        if self.guilds.control(planet, canon).is_some() {
            self.guilds.set_control(planet, canon, None);
        }
    }

    // ------------------------------------------------------------ buffs

    /// Members' views that fell behind (endeavour earned), sent together.
    pub(super) fn send_unsent_guilds(&mut self) {
        for gid in std::mem::take(&mut self.guilds_unsent) {
            self.guild_changed(gid, None);
        }
    }

    /// What player `id`'s guilds' buffs add up to now.
    fn boost_of(&self, account: &str, now: u64) -> rules::Boost {
        let guilds = self.guilds.of(account);
        rules::boost(guilds.iter().filter_map(|g| self.guilds.get(*g)), account, now)
    }

    /// Each player's guild buffs as they start and end: the server's body
    /// and the client's prediction change from right after their latest
    /// input, as for a planet's air.
    pub(super) fn step_guild_buffs(&mut self) {
        let now = unix_now();
        let boosts: Vec<(u32, rules::Boost)> = self
            .players
            .iter()
            .map(|(id, p)| (*id, self.boost_of(&p.account, now)))
            .collect();
        for (id, b) in boosts {
            let Some(p) = self.players.get_mut(&id) else { continue };
            if p.boost == b {
                continue;
            }
            p.boost = b;
            let (run, grapple) = (b.run_tenths as f64 / 10.0, b.grapple as f64);
            p.state.body.guild_run_bonus = run;
            p.state.body.guild_grapple_bonus = grapple;
            p.state.body.guild_lava_percent = b.lava_percent;
            p.out.send(&ServerMsg::GuildBody {
                at: p.last_seq,
                run,
                grapple,
                lava_percent: b.lava_percent,
                place_percent: b.place_percent,
            });
        }
    }

    /// The guild a block stands for: the guild its claim is given to.
    fn guild_at(&self, pi: usize, canon: BlockPos) -> Option<u32> {
        let c = self.planets[pi].claims.at_block(canon)?;
        self.guilds.alignment(c.id).map(|a| a.guild)
    }

    fn helix_view(&self, id: u32, pos: BlockPos) -> Result<HelixView, Refusal> {
        let (canon, block) = self.block_in_reach(id, pos)?;
        let kind = *self.helixes.get(&block).ok_or("not a helix machine")?;
        let p = &self.players[&id];
        let now = unix_now();
        let g = self.guild_at(p.planet, canon).and_then(|g| self.guilds.get(g));
        let running = g.and_then(|g| g.active(now).find(|b| b.kind == kind).map(|b| (b.tier, b.until)));
        let may_start = g
            .and_then(|g| g.member(&p.account))
            .is_some_and(|m| m.powers() & rules::BUFFS != 0);
        Ok(HelixView {
            pos: canon,
            kind,
            guild: g.map(|g| (g.id, g.name.clone(), g.coin, g.endeavour)),
            running,
            may_start,
            closed: false,
        })
    }

    fn send_closed_helix(&self, id: u32, pos: BlockPos) {
        if let Some(p) = self.players.get(&id) {
            p.out.send(&ServerMsg::Helix(HelixView {
                pos,
                kind: rules::BuffKind::Builders,
                guild: None,
                running: None,
                may_start: false,
                closed: true,
            }));
        }
    }

    pub(super) fn open_helix(&mut self, id: u32, pos: Option<BlockPos>) {
        let Some(pos) = pos else {
            if let Some(p) = self.players.get_mut(&id) {
                p.helix = None;
            }
            return;
        };
        match self.helix_view(id, pos) {
            Ok(v) => {
                let p = self.players.get_mut(&id).expect("checked");
                p.helix = Some(v.pos);
                p.out.send(&ServerMsg::Helix(v));
            }
            Err(r) => {
                if let Refusal::Told(text) = &r {
                    self.notice(id, text);
                }
                self.send_closed_helix(id, pos);
            }
        }
    }

    /// Open helix windows see what changed, or close when out of reach.
    pub(super) fn refresh_helixes(&mut self) {
        let open: Vec<(u32, BlockPos)> = self
            .players
            .iter()
            .filter_map(|(id, p)| p.helix.map(|s| (*id, s)))
            .collect();
        for (id, pos) in open {
            match self.helix_view(id, pos) {
                Ok(v) => self.players[&id].out.send(&ServerMsg::Helix(v)),
                Err(_) => {
                    if let Some(p) = self.players.get_mut(&id) {
                        p.helix = None;
                    }
                    self.send_closed_helix(id, pos);
                }
            }
        }
    }

    /// `StartBuff` at the open helix.
    fn start_buff(&mut self, id: u32, pos: BlockPos, tier: u8, account: &str, now: u64) -> Result<(), String> {
        let v = self.helix_view(id, pos).map_err(told)?;
        let Some((gid, ..)) = v.guild else {
            return Err("A helix works on a claim given to a guild".into());
        };
        // Its members: those whose primary guild it is now.
        let members: std::collections::BTreeSet<String> = self
            .guilds
            .get(gid)
            .map(|g| {
                g.members
                    .keys()
                    .filter(|a| self.guilds.primary(a) == Some(gid))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        self.guilds
            .change(gid, |g| g.start_buff(account, v.kind, tier, members, now))
            .ok_or("There is no such guild")??;
        self.guild_changed(gid, None);
        self.refresh_helixes();
        self.step_guild_buffs();
        Ok(())
    }

    // ------------------------------------------------------------ actions

    pub(super) fn guild_action(&mut self, id: u32, action: GuildAction) {
        if let Err(text) = self.try_guild_action(id, action) {
            self.notice(id, &text);
        }
        self.send_guilds(id);
    }

    /// A member's account from their name as shown.
    fn member_account(&self, gid: u32, who: &str) -> Result<String, String> {
        let g = self.guilds.get(gid).ok_or("There is no such guild")?;
        let key = crate::accounts::key(who.trim());
        if g.members.contains_key(&key) || g.asked.contains_key(&key) {
            Ok(key)
        } else {
            Err(format!("{} is not in {}", who.trim(), g.name))
        }
    }

    /// Whether player `id` has a control of guild `gid` open (asking to
    /// join, and running a guild, happen at one, as in Boundless).
    fn at_control_of(&self, id: u32, gid: u32) -> bool {
        self.players
            .get(&id)
            .and_then(|p| p.guild_control)
            .and_then(|pos| self.control_view(id, pos).ok())
            .and_then(|v| v.guild)
            .is_some_and(|g| g.0 == gid)
    }

    fn try_guild_action(&mut self, id: u32, action: GuildAction) -> Result<(), String> {
        let runs = match &action {
            GuildAction::Accept { guild, .. }
            | GuildAction::Decline { guild, .. }
            | GuildAction::Remove { guild, .. }
            | GuildAction::SetRank { guild, .. }
            | GuildAction::SetRights { guild, .. }
            | GuildAction::Pay { guild, .. } => Some(*guild),
            _ => None,
        };
        if let Some(gid) = runs
            && !self.at_control_of(id, gid)
        {
            return Err("Run the guild at one of its guild controls".into());
        }
        let (account, name) = self
            .players
            .get(&id)
            .map(|p| (p.account.clone(), p.name.clone()))
            .ok_or("no such player")?;
        let now = unix_now();
        match action {
            GuildAction::Found { pos, name: gname, tag } => {
                let v = self.control_view(id, pos).map_err(told)?;
                if !v.mine {
                    return Err("Found a guild at a guild control on your own claim".into());
                }
                if v.guild.is_some() {
                    return Err("This guild control already belongs to a guild".into());
                }
                let gname = rules::clean(&gname, NAME_MAX)
                    .ok_or_else(|| format!("A guild name is 1 to {NAME_MAX} letters, numbers and spaces"))?;
                let tag = rules::clean(&tag, TAG_MAX)
                    .filter(|t| !t.contains(' '))
                    .ok_or_else(|| format!("A guild tag is 1 to {TAG_MAX} letters or numbers"))?;
                if self.guilds.name_taken(&gname) {
                    return Err(format!("There is already a guild called {gname}"));
                }
                if !self.guilds.can_join_more(&account) {
                    return Err("You are already in 5 guilds".into());
                }
                let gid = self.guilds.found(&gname, &tag, &account, &name, now);
                let planet = self.planets[self.players[&id].planet].id;
                self.guilds.set_control(planet, v.pos, Some(gid));
                self.notice(id, &format!("You founded {gname} [{tag}]"));
                self.guild_changed(gid, None);
            }
            GuildAction::SetControl { pos, guild } => {
                let v = self.control_view(id, pos).map_err(told)?;
                if !v.mine {
                    return Err("Only the claim's owner sets its guild control".into());
                }
                if let Some(gid) = guild
                    && self.guilds.get(gid).is_none_or(|g| g.member(&account).is_none())
                {
                    return Err("You can only set it to a guild you are in".into());
                }
                let planet = self.planets[self.players[&id].planet].id;
                self.guilds.set_control(planet, v.pos, guild);
                self.refresh_controls();
            }
            GuildAction::Ask { guild } => {
                if !self.at_control_of(id, guild) {
                    return Err("Ask to join at the guild's control".into());
                }
                if !self.guilds.can_join_more(&account) {
                    return Err("You are already in 5 guilds".into());
                }
                self.guilds
                    .change(guild, |g| g.ask(&account, &name))
                    .ok_or("There is no such guild")??;
                self.notice(id, "Asked to join");
                self.guild_changed(guild, None);
            }
            GuildAction::Accept { guild, who } => {
                let who = self.member_account(guild, &who)?;
                let count = self.guilds.count_of(&who);
                self.guilds
                    .change(guild, |g| g.accept(&account, &who, count, now))
                    .ok_or("There is no such guild")??;
                self.guilds.joined(&who, guild);
                if let Some(oid) = self.player_by_account(&who) {
                    let gname = self.guilds.get(guild).map(|g| g.name.clone()).unwrap_or_default();
                    self.notice(oid, &format!("You joined {gname}"));
                }
                self.guild_changed(guild, None);
            }
            GuildAction::Decline { guild, who } => {
                let who = self.member_account(guild, &who)?;
                self.guilds
                    .change(guild, |g| g.decline(&account, &who, now))
                    .ok_or("There is no such guild")??;
                self.guild_changed(guild, Some(&who));
            }
            GuildAction::Leave { guild } => {
                let ended = self
                    .guilds
                    .change(guild, |g| g.leave(&account, now))
                    .ok_or("There is no such guild")??;
                self.guilds.after_leaving(&account, guild);
                if ended {
                    self.guild_ended(guild);
                } else {
                    self.guild_changed(guild, Some(&account));
                }
            }
            GuildAction::Remove { guild, who } => {
                let who = self.member_account(guild, &who)?;
                self.guilds
                    .change(guild, |g| g.remove(&account, &who, now))
                    .ok_or("There is no such guild")??;
                self.guilds.after_leaving(&who, guild);
                self.guild_changed(guild, Some(&who));
            }
            GuildAction::SetRank { guild, who, rank } => {
                let who = self.member_account(guild, &who)?;
                self.guilds
                    .change(guild, |g| g.set_rank(&account, &who, rank, now))
                    .ok_or("There is no such guild")??;
                self.guild_changed(guild, None);
            }
            GuildAction::SetRights {
                guild,
                who,
                rights,
                powers,
            } => {
                let who = self.member_account(guild, &who)?;
                self.guilds
                    .change(guild, |g| g.set_rights(&account, &who, rights, powers, now))
                    .ok_or("There is no such guild")??;
                self.guild_changed(guild, None);
            }
            GuildAction::Donate { guild, coin } => {
                if self.guilds.get(guild).is_none_or(|g| g.member(&account).is_none()) {
                    return Err("You are not in this guild".into());
                }
                let ledger = self.ledgers.get_mut(&account).ok_or("no ledger")?;
                if coin == 0 || !ledger.spend(coin) {
                    return Err("You do not have that much coin".into());
                }
                self.ledgers_dirty.insert(account.clone());
                self.ledgers_unsent.insert(id);
                self.guilds.change(guild, |g| g.donated(&account, coin, now));
                self.guild_changed(guild, None);
            }
            GuildAction::Pay { guild, to, coin } => {
                let to = self.member_account(guild, &to)?;
                self.guilds
                    .change(guild, |g| g.pay(&account, &to, coin, now))
                    .ok_or("There is no such guild")??;
                self.pay_account(&to, coin);
                self.guild_changed(guild, None);
            }
            GuildAction::SetPrimary { guild } => {
                if self.guilds.get(guild).is_none_or(|g| g.member(&account).is_none()) {
                    return Err("You are not in this guild".into());
                }
                self.guilds.set_primary(&account, Some(guild));
            }
            GuildAction::StartBuff { pos, tier } => self.start_buff(id, pos, tier, &account, now)?,
            GuildAction::Align {
                claim,
                guild,
                controlled,
            } => {
                let c = self
                    .planets
                    .iter()
                    .find_map(|p| p.claims.get(claim))
                    .ok_or("There is no such claim")?;
                if c.owner != account {
                    return Err("Only a claim's owner gives it to a guild".into());
                }
                if let Some(gid) = guild
                    && self.guilds.get(gid).is_none_or(|g| g.member(&account).is_none())
                {
                    return Err("You can only give a claim to a guild you are in".into());
                }
                let before = self.guilds.alignment(claim).map(|a| a.guild);
                self.guilds
                    .align(claim, guild.map(|g| Alignment { guild: g, controlled }));
                let gname = |gs: &crate::guilds::Guilds, g: u32| gs.get(g).map(|g| g.name.clone()).unwrap_or_default();
                for g in [before, guild].into_iter().flatten() {
                    let text = match guild {
                        Some(n) if n == g => format!(
                            "{name} {} a claim to {}",
                            if controlled { "gave the running of" } else { "aligned" },
                            gname(&self.guilds, g)
                        ),
                        _ => format!("{name} took a claim back"),
                    };
                    self.guilds.change(g, |x| x.note(now, text));
                    self.guild_changed(g, None);
                }
                self.claim_rights_changed(claim);
            }
        }
        Ok(())
    }

    /// The last member left: the guild ends and its claims and controls go
    /// back to their owners.
    fn guild_ended(&mut self, gid: u32) {
        for claim in self.guilds.end(gid) {
            self.claim_rights_changed(claim);
        }
        self.refresh_controls();
    }
}

fn told(r: Refusal) -> String {
    match r {
        Refusal::Told(t) => t,
        Refusal::Quiet(q) => q.to_string(),
    }
}
