//! A running tempo serves its project on a socket, so `tempo do` and the MCP server edit what's
//! on screen (live, and undoable) instead of the file underneath it. One JSON request per line,
//! one JSON reply per line: `{"ok": …}` or `{"error": "…"}`.

use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::Instant,
};

use futures::StreamExt;
use gpui::{App, Context, WeakEntity};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tempo_engine::{
    commands::{self, ChannelRef, Command, PluginSource},
    player::Player,
    plugin::PluginApi,
    project::{Project, Which},
};

use crate::{
    strip::Channel,
    workspace::{AgentEdit, Workspace},
};

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "request", rename_all = "snake_case")]
pub enum Request {
    Edit {
        command: Command,
    },
    Project,
    Play,
    Stop,
    Locate {
        tick: u64,
    },
    Save,
    Undo,
    Redo,
    /// Launches a track's Live Loops cell for a scene, or stops it without one.
    Launch {
        track: usize,
        #[serde(default)]
        scene: Option<usize>,
    },
    /// A plugin's parameters: ids, names, values.
    Params {
        channel: ChannelRef,
        plugin: Which,
    },
    /// What a plugin can load: its programs and preset files.
    Presets {
        channel: ChannelRef,
        plugin: Which,
    },
    /// Saves a plugin's settings as a preset file.
    SavePreset {
        channel: ChannelRef,
        plugin: Which,
        name: String,
    },
}

/// The running plugins, for `commands::resolve`.
struct Running<'a> {
    player: &'a Player,
    ids: Vec<(ChannelRef, u64)>,
}

impl PluginSource for Running<'_> {
    fn plugin(
        &mut self,
        _project: &Project,
        channel: ChannelRef,
        which: Which,
    ) -> Result<&dyn PluginApi, String> {
        let id = self
            .ids
            .iter()
            .find(|(c, _)| *c == channel)
            .map(|(_, id)| *id)
            .ok_or("no such channel")?;
        let plugin = self.player.plugin(id, which).ok_or("no plugin there")?;
        Ok(&**plugin)
    }
}

pub type Reply = Result<Value, String>;

/// Where the app serves a project: in the temp folder (socket paths must be short), named by a
/// hash of the project's full path.
pub fn socket_path(dir: &Path) -> PathBuf {
    let dir = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    // FNV-1a: stable across builds, unlike std's hasher.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in dir.as_os_str().as_encoded_bytes() {
        h = (h ^ *b as u64).wrapping_mul(0x100_0000_01b3);
    }
    std::env::temp_dir().join(format!("tempo-{h:016x}.sock"))
}

/// Asks the app that has `dir` open, if one does.
pub fn ask(dir: &Path, request: &Request) -> Option<Reply> {
    let mut conn = UnixStream::connect(socket_path(dir)).ok()?;
    writeln!(conn, "{}", serde_json::to_string(request).unwrap()).ok()?;
    let mut line = String::new();
    BufReader::new(conn).read_line(&mut line).ok()?;
    let reply: Value = serde_json::from_str(&line).ok()?;
    Some(match (reply.get("ok"), reply.get("error")) {
        (Some(ok), _) => Ok(ok.clone()),
        (_, Some(e)) => Err(e.as_str().unwrap_or("error").to_string()),
        _ => Err(format!("an odd reply from the app: {line}")),
    })
}

/// Serves the workspace's project while the app runs.
pub fn serve(ws: WeakEntity<Workspace>, dir: &Path, cx: &mut App) {
    let path = socket_path(dir);
    let _ = std::fs::remove_file(&path); // left by a tempo that crashed
    let Ok(listener) = UnixListener::bind(&path) else {
        return;
    };
    let (tx, mut rx) = futures::channel::mpsc::unbounded::<(Request, mpsc::Sender<Reply>)>();
    thread::spawn(move || {
        for conn in listener.incoming().flatten() {
            let tx = tx.clone();
            thread::spawn(move || answer(conn, tx));
        }
    });
    cx.spawn(async move |cx| {
        while let Some((request, reply)) = rx.next().await {
            let r = ws.update(cx, |ws, cx| ws.handle(request, cx));
            let _ = reply.send(r.unwrap_or_else(|_| Err("the window has closed".into())));
        }
    })
    .detach();
}

/// Answers one connection's requests, a line each, passing them to the main thread.
fn answer(
    conn: UnixStream,
    tx: futures::channel::mpsc::UnboundedSender<(Request, mpsc::Sender<Reply>)>,
) {
    let Ok(mut out) = conn.try_clone() else {
        return;
    };
    for line in BufReader::new(conn).lines().map_while(Result::ok) {
        let reply = match serde_json::from_str::<Request>(&line) {
            Ok(request) => {
                let (reply_tx, reply_rx) = mpsc::channel();
                if tx.unbounded_send((request, reply_tx)).is_err() {
                    return;
                }
                reply_rx
                    .recv()
                    .unwrap_or_else(|_| Err("the app is closing".into()))
            }
            Err(e) => Err(e.to_string()),
        };
        let json = match reply {
            Ok(v) => json!({ "ok": v }),
            Err(e) => json!({ "error": e }),
        };
        if writeln!(out, "{json}").is_err() {
            return;
        }
    }
}

impl Workspace {
    /// Remembers what an agent did, and where: the track and region it named, or made.
    fn note_agent(&mut self, asked: &Value, answer: &Value) {
        let index = |v: &Value| v.as_u64().map(|n| n as usize);
        let made = answer.get("region").and_then(Value::as_array);
        let track = made
            .and_then(|r| index(&r[0]))
            .or_else(|| answer.get("track").and_then(index))
            .or_else(|| asked.get("track").and_then(index))
            .or_else(|| asked.pointer("/channel/track").and_then(index));
        let region = made
            .and_then(|r| r.get(1).and_then(index))
            .or_else(|| asked.get("region").and_then(index));
        let op = asked
            .get("op")
            .and_then(Value::as_str)
            .unwrap_or("edit")
            .replace('_', " ");
        let what = match track.and_then(|t| self.session.project.tracks.get(t)) {
            Some(t) => match region {
                Some(r) => format!("{op} · {}, region {}", t.name, r + 1),
                None => format!("{op} · {}", t.name),
            },
            None => op,
        };
        self.agent.push(AgentEdit {
            at: Instant::now(),
            what,
            track,
            region,
        });
        if self.agent.len() > 200 {
            self.agent.remove(0);
        }
    }

    /// A plugin running on a channel.
    fn running_plugin(&self, channel: ChannelRef, which: Which) -> Result<&dyn PluginApi, String> {
        let p = &self.session.project;
        let ch = match channel {
            ChannelRef::Track(i) if i < p.tracks.len() => Channel::Track(i),
            ChannelRef::Aux(i) if i < p.auxes.len() => Channel::Aux(i),
            ChannelRef::Master => Channel::Master,
            _ => return Err("no such channel".into()),
        };
        let plugin = self.player.plugin(self.owner_id(ch), which);
        Ok(&**plugin.ok_or("no plugin there")?)
    }

    pub fn handle(&mut self, request: Request, cx: &mut Context<Self>) -> Reply {
        let ok = Ok(json!({}));
        match request {
            Request::Edit { command } => {
                let p = &self.session.project;
                let ids = (0..p.tracks.len())
                    .map(|i| (ChannelRef::Track(i), Channel::Track(i)))
                    .chain((0..p.auxes.len()).map(|i| (ChannelRef::Aux(i), Channel::Aux(i))))
                    .chain([(ChannelRef::Master, Channel::Master)])
                    .map(|(r, c)| (r, self.owner_id(c)))
                    .collect();
                let mut running = Running {
                    player: &self.player,
                    ids,
                };
                let command = commands::resolve(p, command, &mut running)?;
                self.session.checkpoint();
                let dir = self.session.dir.clone();
                let asked = serde_json::to_value(&command).unwrap_or_default();
                match commands::apply(&mut self.session.project, &dir, command) {
                    Ok(mut v) => {
                        self.note_agent(&asked, &v);
                        self.changed(cx);
                        // The edit stands, but the engine couldn't do all of it.
                        if let (Some(problem), Some(map)) = (&self.status, v.as_object_mut()) {
                            map.insert("warning".into(), problem.clone().into());
                        }
                        Ok(v)
                    }
                    Err(e) => {
                        self.session.cancel();
                        Err(e)
                    }
                }
            }
            Request::Project => Ok(serde_json::to_value(&self.session.project).unwrap()),
            Request::Play => {
                self.player.play();
                ok
            }
            Request::Stop => {
                self.stop(cx);
                ok
            }
            Request::Locate { tick } => {
                let at = self.session.project.samples(tick, self.player.rate());
                self.player.locate(at);
                ok
            }
            Request::Save => match self.save(cx) {
                true => ok,
                false => Err(self.status.clone().unwrap_or_default()),
            },
            Request::Launch { track, scene } => {
                if track >= self.session.project.tracks.len() {
                    return Err(format!("no track {track}"));
                }
                self.launch(track, scene, cx);
                ok
            }
            Request::Params { channel, plugin } => {
                let found = self.running_plugin(channel, plugin)?;
                Ok(crate::cli::params_json(found))
            }
            Request::Presets { channel, plugin } => {
                let found = self.running_plugin(channel, plugin)?;
                Ok(crate::cli::presets_json(found))
            }
            Request::SavePreset {
                channel,
                plugin,
                name,
            } => {
                let found = self.running_plugin(channel, plugin)?;
                Ok(json!({ "file": found.save_preset(&name)? }))
            }
            Request::Undo | Request::Redo => {
                let done = match request {
                    Request::Undo => self.session.undo(),
                    _ => self.session.redo(),
                };
                if done {
                    self.changed(cx);
                }
                Ok(json!({ "done": done }))
            }
        }
    }
}
