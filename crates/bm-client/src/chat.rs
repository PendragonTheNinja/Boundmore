//! The chat box: what was said, what is being typed, and the commands.
//!
//! Lines come from the server (`ServerMsg::Chat`, oldest first) on the one
//! reliable stream, so they are shown in the order they arrive. The box sits
//! bottom left, above the hotbar's height. Idle, it shows the last few lines,
//! each fading out a while after it came; open, it shows the whole kept
//! history in a scroll area above the input line.

use crate::menus::GOLD;
use bm_protocol::{Channel, ChatLine, ClientMsg, MAX_CHAT_LEN};
use egui::{Align2, Color32, CornerRadius, FontId, Id, Stroke, Ui, text::LayoutJob, vec2};
use std::collections::VecDeque;

/// Lines kept for scrolling back.
pub const HISTORY: usize = 200;
/// Lines shown while the box is idle.
pub const SHOWN: usize = 8;
/// Seconds a line stays up while idle, the last `FADE` of them fading.
pub const LINGER: f64 = 10.0;
pub const FADE: f64 = 1.5;

const WIDTH: f32 = 440.0;
const FONT: f32 = 15.0;

const LOCAL: Color32 = Color32::from_rgb(242, 240, 234);
const PLANET: Color32 = Color32::from_rgb(132, 220, 120);
const UNIVERSE: Color32 = GOLD;
const WHISPER: Color32 = Color32::from_rgb(198, 150, 255);
const SERVER: Color32 = Color32::from_rgb(160, 156, 150);

pub const HELP: &str = "Commands: /l local, /p planet, /u universe, /w name text to whisper, /r text to reply, /friend name and /unfriend name (O for friends), /settlement name to name the settlement you are warden of. \
                        Testing (servers started with --creative): /creative or F8, /give item count, /xp amount.";

/// What a line typed into the box asks for.
#[derive(Clone, Debug, PartialEq)]
pub enum Typed {
    /// Nothing to do (an empty line).
    Nothing,
    Send {
        channel: Channel,
        text: String,
    },
    /// Make this the channel plain lines go to.
    Switch(Channel),
    /// A line only this player sees (help, a mistake in a command).
    Local(String),
    /// Switch creative mode (a testing aid).
    Creative,
    /// A testing command for the server (`/give`, `/xp`), as typed.
    Command(String),
    /// Ask (`/friend name`) or end (`/unfriend name`) a friendship.
    Friend(String, bm_protocol::FriendAction),
}

/// Reads one typed line. Plain text goes to `default`. `/l`, `/p` and `/u`
/// alone switch the default channel; followed by text they send that one
/// line there. `/w name text` whispers (a name with spaces goes in quotes),
/// `/r text` replies to whoever last whispered.
pub fn parse(input: &str, default: &Channel, last_whisperer: Option<&str>) -> Typed {
    let input = input.trim();
    if input.is_empty() {
        return Typed::Nothing;
    }
    let Some(command) = input.strip_prefix('/') else {
        return send(default.clone(), input);
    };
    let (word, rest) = match command.split_once(char::is_whitespace) {
        Some((w, r)) => (w, r.trim()),
        None => (command, ""),
    };
    let channel = match word.to_lowercase().as_str() {
        "l" | "local" => Channel::Local,
        "p" | "planet" => Channel::Planet,
        "u" | "universe" => Channel::Universe,
        "w" | "whisper" | "msg" => {
            let Some((name, text)) = split_name(rest) else {
                return Typed::Local("Whisper with /w name text.".into());
            };
            return send(Channel::Whisper(name), text);
        }
        "creative" | "c" => return Typed::Creative,
        "friend" | "unfriend" => {
            if rest.is_empty() {
                return Typed::Local(format!("Name them: /{} name", word.to_lowercase()));
            }
            let action = if word.eq_ignore_ascii_case("friend") {
                bm_protocol::FriendAction::Ask
            } else {
                bm_protocol::FriendAction::Remove
            };
            return Typed::Friend(rest.to_string(), action);
        }
        "give" | "xp" | "settlement" => return Typed::Command(input.to_string()),
        "r" | "reply" => {
            let Some(name) = last_whisperer else {
                return Typed::Local("Nobody has whispered to you yet.".into());
            };
            if rest.is_empty() {
                return Typed::Local("Reply with /r text.".into());
            }
            return send(Channel::Whisper(name.to_string()), rest);
        }
        _ => return Typed::Local(HELP.into()),
    };
    if rest.is_empty() {
        Typed::Switch(channel)
    } else {
        send(channel, rest)
    }
}

fn send(channel: Channel, text: &str) -> Typed {
    let text: String = text.chars().take(MAX_CHAT_LEN).collect();
    Typed::Send { channel, text }
}

/// `name text` or `"name with spaces" text`; None without both.
fn split_name(rest: &str) -> Option<(String, &str)> {
    let (name, text) = if let Some(quoted) = rest.strip_prefix('"') {
        let (name, text) = quoted.split_once('"')?;
        (name.trim(), text.trim())
    } else {
        let (name, text) = rest.split_once(char::is_whitespace)?;
        (name, text.trim())
    };
    (!name.is_empty() && !text.is_empty()).then(|| (name.to_string(), text))
}

/// A short name for a channel, shown left of the input.
pub fn channel_label(c: &Channel) -> String {
    match c {
        Channel::Local => "Local".into(),
        Channel::Planet => "Planet".into(),
        Channel::Universe => "Universe".into(),
        Channel::Whisper(name) => format!("To {name}"),
    }
}

pub fn channel_colour(c: &Channel) -> Color32 {
    match c {
        Channel::Local => LOCAL,
        Channel::Planet => PLANET,
        Channel::Universe => UNIVERSE,
        Channel::Whisper(_) => WHISPER,
    }
}

struct Entry {
    line: ChatLine,
    arrived: f64,
}

/// The chat box's state, kept by the app across frames.
pub struct ChatBox {
    entries: VecDeque<Entry>,
    /// Who we are, to tell a whisper we sent from one sent to us.
    pub me: String,
    pub input: String,
    /// Where plain lines go.
    pub channel: Channel,
    pub last_whisperer: Option<String>,
    /// Typing: the mouse is free and game keys go to the input.
    pub open: bool,
    /// Set when opened by a key, so that key's own press and character are
    /// kept out of the input on the first frame.
    pub swallow: bool,
    /// `/creative` was typed: the app switches creative mode.
    creative_asked: bool,
}

impl Default for ChatBox {
    fn default() -> Self {
        Self {
            entries: VecDeque::new(),
            me: String::new(),
            input: String::new(),
            channel: Channel::Local,
            last_whisperer: None,
            open: false,
            swallow: false,
            creative_asked: false,
        }
    }
}

/// What the box asks the app to do this frame.
#[derive(Clone, Debug, PartialEq)]
pub enum ChatAction {
    Send(ClientMsg),
    Close,
    /// `/creative`: switch creative mode (the game predicts it).
    Creative,
}

impl ChatBox {
    /// Lines from the server, oldest first.
    pub fn receive(&mut self, lines: Vec<ChatLine>, now: f64) {
        for line in lines {
            if matches!(line.channel, Channel::Whisper(_)) && !line.from.is_empty() && line.from != self.me {
                self.last_whisperer = Some(line.from.clone());
            }
            self.push(line, now);
        }
    }

    /// A line only this player sees, in the server's grey.
    pub fn local(&mut self, text: String, now: f64) {
        let line = ChatLine {
            channel: Channel::Local,
            from: String::new(),
            text,
            time: 0,
        };
        self.push(line, now);
    }

    fn push(&mut self, line: ChatLine, arrived: f64) {
        self.entries.push_back(Entry { line, arrived });
        while self.entries.len() > HISTORY {
            self.entries.pop_front();
        }
    }

    /// The lines an idle box shows, oldest first, with their opacity.
    pub fn recent(&self, now: f64) -> Vec<(&ChatLine, f32)> {
        let skip = self.entries.len().saturating_sub(SHOWN);
        self.entries
            .iter()
            .skip(skip)
            .filter_map(|e| {
                let left = LINGER - (now - e.arrived);
                (left > 0.0).then(|| (&e.line, (left / FADE).clamp(0.0, 1.0) as f32))
            })
            .collect()
    }

    /// Carries out a typed line: what to send, if anything. Switching
    /// channels and local lines happen here.
    pub fn submit(&mut self, now: f64) -> Option<ClientMsg> {
        let typed = parse(&self.input, &self.channel, self.last_whisperer.as_deref());
        self.input.clear();
        match typed {
            Typed::Nothing => None,
            Typed::Send { channel, text } => Some(ClientMsg::Chat { channel, text }),
            Typed::Switch(c) => {
                self.channel = c;
                None
            }
            Typed::Local(text) => {
                self.local(text, now);
                None
            }
            Typed::Creative => {
                self.creative_asked = true;
                None
            }
            Typed::Command(text) => Some(ClientMsg::Command { text }),
            Typed::Friend(name, action) => Some(ClientMsg::Friend { name, action }),
        }
    }

    /// Opens the box for typing.
    pub fn open(&mut self, swallow_key: bool) {
        self.open = true;
        self.swallow = swallow_key;
    }

    /// Draws the box. While open it owns the keyboard: Enter sends and
    /// closes, Escape closes without sending.
    pub fn show(&mut self, ctx: &egui::Context, bottom_gap: f32, now: f64) -> Vec<ChatAction> {
        let mut actions = Vec::new();
        let area = egui::Area::new(Id::unique("chat"))
            .anchor(Align2::LEFT_BOTTOM, vec2(14.0, -bottom_gap))
            .order(egui::Order::Middle)
            .interactable(self.open);
        if !self.open {
            let recent = self.recent(now);
            if recent.is_empty() {
                return actions;
            }
            area.show(ctx, |ui| {
                ui.set_width(WIDTH);
                ui.spacing_mut().item_spacing.y = 2.0;
                for (line, alpha) in recent {
                    egui::Frame::new()
                        .fill(Color32::from_black_alpha((140.0 * alpha) as u8))
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(egui::Margin::symmetric(8, 2))
                        .show(ui, |ui| line_label(ui, line, &self.me, alpha));
                }
            });
            return actions;
        }
        area.show(ctx, |ui| {
            egui::Frame::new()
                .fill(Color32::from_rgba_premultiplied(10, 9, 15, 205))
                .stroke(Stroke::new(1.0, Color32::from_rgba_unmultiplied(150, 116, 62, 160)))
                .corner_radius(CornerRadius::same(10))
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.set_width(WIDTH);
                    ui.spacing_mut().item_spacing.y = 3.0;
                    egui::ScrollArea::vertical()
                        .max_height(250.0)
                        .stick_to_bottom(true)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            for e in &self.entries {
                                line_label(ui, &e.line, &self.me, 1.0);
                            }
                        });
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        let colour = channel_colour(&self.channel);
                        egui::Frame::new()
                            .stroke(Stroke::new(1.0, colour.gamma_multiply(0.8)))
                            .corner_radius(CornerRadius::same(5))
                            .inner_margin(egui::Margin::symmetric(6, 1))
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new(channel_label(&self.channel))
                                        .color(colour)
                                        .size(13.0),
                                );
                            });
                        let id = Id::unique("chat-input");
                        ui.memory_mut(|m| m.request_focus(id));
                        ui.add(
                            egui::TextEdit::singleline(&mut self.input)
                                .id(id)
                                .char_limit(MAX_CHAT_LEN)
                                .desired_width(f32::INFINITY)
                                .hint_text("Say something, or /help")
                                .margin(vec2(6.0, 4.0)),
                        );
                    });
                });
        });
        let (enter, escape) = ctx.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
        if escape {
            actions.push(ChatAction::Close);
        } else if enter {
            if let Some(msg) = self.submit(now) {
                actions.push(ChatAction::Send(msg));
            }
            if std::mem::take(&mut self.creative_asked) {
                actions.push(ChatAction::Creative);
            }
            actions.push(ChatAction::Close);
        }
        actions
    }

    /// Closes the box. What was typed stays for next time unless it was sent.
    pub fn close(&mut self) {
        self.open = false;
        self.swallow = false;
    }
}

/// One line: the speaker's name in bold, then what they said, in the
/// channel's colour; the server's own lines grey and italic.
fn line_label(ui: &mut Ui, line: &ChatLine, me: &str, alpha: f32) {
    let font = FontId::proportional(FONT);
    let colour = channel_colour(&line.channel).gamma_multiply(alpha);
    let mut job = LayoutJob::default();
    job.wrap.max_width = ui.available_width();
    let mut bold = LayoutJob::default();
    bold.wrap.max_width = job.wrap.max_width;
    let mut part = |text: &str, strong: bool, italics: bool, colour: Color32| {
        let format = egui::TextFormat {
            font_id: font.clone(),
            color: colour,
            italics,
            ..Default::default()
        };
        job.append(text, 0.0, format.clone());
        bold.append(
            text,
            0.0,
            egui::TextFormat {
                color: if strong { colour } else { Color32::TRANSPARENT },
                ..format
            },
        );
    };
    if line.from.is_empty() {
        part(&line.text, false, true, SERVER.gamma_multiply(alpha));
    } else {
        match &line.channel {
            Channel::Whisper(to) if line.from == me => {
                part("To ", false, false, colour);
                part(to, true, false, colour);
                part(": ", false, false, colour);
            }
            Channel::Whisper(_) => {
                part(&line.from, true, false, colour);
                part(" whispers: ", false, false, colour);
            }
            _ => {
                part(&line.from, true, false, colour);
                part(": ", false, false, colour);
            }
        }
        part(&line.text, false, false, colour);
    }
    // egui's default font has no bold face: the name is drawn a second time
    // a fraction of a pixel to the right, the rest of that copy invisible.
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    let bold = ui.fonts_mut(|f| f.layout_job(bold));
    let (rect, _) = ui.allocate_exact_size(galley.size(), egui::Sense::hover());
    let shadow = Color32::from_black_alpha((160.0 * alpha) as u8);
    let painter = ui.painter();
    painter.galley_with_override_text_color(rect.min + vec2(1.0, 1.0), galley.clone(), shadow);
    painter.galley(rect.min, galley, Color32::WHITE);
    painter.galley(rect.min + vec2(0.6, 0.0), bold, Color32::WHITE);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(channel: Channel, from: &str, text: &str) -> ChatLine {
        ChatLine {
            channel,
            from: from.into(),
            text: text.into(),
            time: 0,
        }
    }

    fn sent(channel: Channel, text: &str) -> Typed {
        Typed::Send {
            channel,
            text: text.into(),
        }
    }

    #[test]
    fn plain_lines_go_to_the_default_channel() {
        assert_eq!(
            parse("  hello there ", &Channel::Planet, None),
            sent(Channel::Planet, "hello there")
        );
        assert_eq!(parse("   ", &Channel::Planet, None), Typed::Nothing);
        let long = "x".repeat(MAX_CHAT_LEN + 50);
        let Typed::Send { text, .. } = parse(&long, &Channel::Local, None) else {
            panic!()
        };
        assert_eq!(text.chars().count(), MAX_CHAT_LEN);
    }

    #[test]
    fn channel_commands_switch_or_send_once() {
        assert_eq!(parse("/l", &Channel::Universe, None), Typed::Switch(Channel::Local));
        assert_eq!(parse("/P", &Channel::Local, None), Typed::Switch(Channel::Planet));
        assert_eq!(parse("/u", &Channel::Local, None), Typed::Switch(Channel::Universe));
        assert_eq!(
            parse("/u anyone selling copper?", &Channel::Local, None),
            sent(Channel::Universe, "anyone selling copper?")
        );
    }

    #[test]
    fn whispers_and_replies() {
        let w = |n: &str| Channel::Whisper(n.into());
        assert_eq!(
            parse("/w Wren meet at the shrine", &Channel::Local, None),
            sent(w("Wren"), "meet at the shrine")
        );
        assert_eq!(
            parse("/w \"Pen dragon\" hi", &Channel::Local, None),
            sent(w("Pen dragon"), "hi")
        );
        assert!(matches!(parse("/w Wren", &Channel::Local, None), Typed::Local(_)));
        assert!(matches!(parse("/w", &Channel::Local, None), Typed::Local(_)));
        assert_eq!(
            parse("/r on my way", &Channel::Local, Some("Wren")),
            sent(w("Wren"), "on my way")
        );
        assert!(matches!(parse("/r on my way", &Channel::Local, None), Typed::Local(_)));
        assert!(matches!(parse("/r", &Channel::Local, Some("Wren")), Typed::Local(_)));
    }

    #[test]
    fn unknown_commands_show_the_help() {
        assert_eq!(parse("/dance", &Channel::Local, None), Typed::Local(HELP.into()));
        assert_eq!(parse("/help", &Channel::Local, None), Typed::Local(HELP.into()));
        assert_eq!(parse("/creative", &Channel::Local, None), Typed::Creative);
        assert_eq!(
            parse("/give iron_ingot 20", &Channel::Local, None),
            Typed::Command("/give iron_ingot 20".into())
        );
        assert_eq!(
            parse("/friend Wren", &Channel::Local, None),
            Typed::Friend("Wren".into(), bm_protocol::FriendAction::Ask)
        );
        assert!(!HELP.contains('\u{2014}'), "no em dashes in player text");
    }

    #[test]
    fn submitting_switches_sends_and_answers_locally() {
        let mut c = ChatBox {
            me: "Pendragon".into(),
            ..Default::default()
        };
        c.input = "/p".into();
        assert_eq!(c.submit(0.0), None);
        assert_eq!(c.channel, Channel::Planet);
        assert!(c.input.is_empty());
        c.input = "hello".into();
        assert_eq!(
            c.submit(0.0),
            Some(ClientMsg::Chat {
                channel: Channel::Planet,
                text: "hello".into()
            })
        );
        c.input = "/nope".into();
        assert_eq!(c.submit(1.0), None);
        assert_eq!(c.entries.len(), 1);
        assert_eq!(c.recent(1.0)[0].0.text, HELP);
    }

    #[test]
    fn the_last_whisperer_is_remembered_but_not_ourselves() {
        let mut c = ChatBox {
            me: "Pendragon".into(),
            ..Default::default()
        };
        c.receive(vec![line(Channel::Whisper("Pendragon".into()), "Wren", "psst")], 0.0);
        assert_eq!(c.last_whisperer.as_deref(), Some("Wren"));
        // Our own copy of a whisper we sent does not change who we reply to.
        c.receive(vec![line(Channel::Whisper("Ash".into()), "Pendragon", "hi Ash")], 0.0);
        assert_eq!(c.last_whisperer.as_deref(), Some("Wren"));
        c.receive(vec![line(Channel::Universe, "Ash", "hello all")], 0.0);
        assert_eq!(c.last_whisperer.as_deref(), Some("Wren"));
    }

    #[test]
    fn idle_lines_fade_and_history_is_capped() {
        let mut c = ChatBox::default();
        c.receive(vec![line(Channel::Local, "A", "first")], 0.0);
        c.receive(vec![line(Channel::Local, "B", "second")], 5.0);
        let r = c.recent(1.0);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].1, 1.0);
        // Fading in its last FADE seconds, then gone.
        let r = c.recent(LINGER - FADE / 2.0);
        assert_eq!(r[0].0.text, "first");
        assert!((r[0].1 - 0.5).abs() < 1e-6);
        let r = c.recent(LINGER + 0.1);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].0.text, "second");
        assert!(c.recent(LINGER + 5.0).is_empty());
        // The history on joining: fifty lines at once, the newest few shown.
        let many: Vec<ChatLine> = (0..50).map(|i| line(Channel::Universe, "C", &i.to_string())).collect();
        c.receive(many, 20.0);
        let r = c.recent(20.0);
        assert_eq!(r.len(), SHOWN);
        assert_eq!(r.last().unwrap().0.text, "49");
        // Only the last HISTORY lines are kept.
        for i in 0..300 {
            c.receive(vec![line(Channel::Planet, "D", &format!("l{i}"))], 30.0);
        }
        assert_eq!(c.entries.len(), HISTORY);
        assert_eq!(c.entries.front().unwrap().line.text, "l100");
    }
}
