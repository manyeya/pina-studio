//! Taking every plugin's picture: each opened in turn in a process of its own (`pina
//! __picture`), how hard it works the computer timed, its window shown and photographed, then
//! closed; one that crashes or hangs is marked as not loading, and Pina Studio carries on. And a
//! plugin's picture taken when its window is opened in a song, if it has none yet.

use std::time::Duration;

use pina_engine::{
    plugin::picture::snapshot,
    song::project::{PluginRef, Which},
};

use super::*;
use crate::mix::strip::Channel;

/// How long a plugin's window has to draw itself before its picture is taken (opened in a song).
const SETTLE: Duration = Duration::from_millis(1500);

/// Pictures being taken: what's left (by device key), how many there were, the one being
/// taken now.
pub struct Indexing {
    pub todo: Vec<String>,
    pub total: usize,
    pub now: Option<String>,
}

impl Workspace {
    /// Takes the pictures of the plugins that have none (or of `keys`, again), one after
    /// another; again stops it.
    pub fn take_plugin_pictures(&mut self, keys: Option<Vec<String>>, cx: &mut Context<Self>) {
        if self.plugin_page.indexing.take().is_some() {
            self.status = Some("Stopped taking pictures.".into());
            cx.notify();
            return;
        }
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        let facts = &self.plugin_page.facts;
        let todo: Vec<String> = keys.unwrap_or_else(|| {
            self.catalog
                .devices
                .iter()
                .filter(|d| d.own.is_none())
                .filter(|d| facts.get(&d.key).is_none_or(|f| f.wants_picture()))
                .map(|d| d.key.clone())
                .rev()
                .collect()
        });
        if todo.is_empty() {
            self.status = Some("Every plugin has its picture.".into());
            cx.notify();
            return;
        }
        let _ = std::fs::create_dir_all(pictures_dir());
        self.plugin_page.indexing = Some(Indexing {
            total: todo.len(),
            todo,
            now: None,
        });
        cx.notify();
        cx.spawn(async move |this, cx| {
            loop {
                // The next plugin: its first format, and where its picture goes.
                let next = this.update(cx, |ws, cx| {
                    let indexing = ws.plugin_page.indexing.as_mut()?;
                    let key = indexing.todo.pop()?;
                    let d = ws.catalog.device(&key)?;
                    let dev = &ws.catalog.devices[d];
                    let r = dev.formats.first()?.plugin_ref();
                    indexing.now = Some(dev.name.clone());
                    cx.notify();
                    Some((key, r, dev.instrument()))
                });
                let Ok(Some((key, r, instrument))) = next else {
                    let more = this
                        .update(cx, |ws, _| {
                            ws.plugin_page
                                .indexing
                                .as_ref()
                                .is_some_and(|i| !i.todo.is_empty())
                        })
                        .unwrap_or(false);
                    if more {
                        continue;
                    }
                    break;
                };
                let name = picture_name(&key);
                let out = pictures_dir().join(&name);
                let exe = exe.clone();
                let taken = cx
                    .background_spawn(async move { snapshot(&exe, &r, instrument, &out) })
                    .await;
                let stop = this
                    .update(cx, |ws, cx| {
                        let fact = ws.plugin_page.facts.entry(key).or_default();
                        match taken {
                            Ok(s) => {
                                fact.load = Some(s.load);
                                fact.failed = None;
                                fact.windowless = s.windowless;
                                if !s.windowless {
                                    (fact.picture, fact.w, fact.h) = (Some(name), s.w, s.h);
                                }
                            }
                            Err(e) => fact.failed = Some(e),
                        }
                        save_facts(&ws.plugin_page.facts);
                        cx.notify();
                        ws.plugin_page.indexing.is_none()
                    })
                    .unwrap_or(true);
                if stop {
                    break;
                }
            }
            this.update(cx, |ws, cx| {
                if let Some(i) = ws.plugin_page.indexing.take() {
                    ws.status = Some(format!("Took {} plugins' pictures.", i.total));
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The device a plugin reference is, by its file and name.
    pub(crate) fn device_of(&self, r: &PluginRef) -> Option<usize> {
        self.catalog.devices.iter().position(|d| {
            d.formats
                .iter()
                .any(|p| p.path == r.path && p.class == r.class)
        })
    }

    /// A plugin's window just opened in a song: its picture taken once it's drawn, if it has
    /// none yet.
    pub(crate) fn picture_when_shown(&mut self, ch: Channel, which: Which, cx: &mut Context<Self>) {
        let Some(r) = self.slot_plugin(ch, which).cloned() else {
            return;
        };
        let Some(d) = self.device_of(&r) else {
            return;
        };
        let key = self.catalog.devices[d].key.clone();
        if self
            .plugin_page
            .facts
            .get(&key)
            .is_some_and(|f| f.picture.is_some())
        {
            return;
        }
        let id = self.owner_id(ch);
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SETTLE).await;
            this.update(cx, |ws, cx| {
                let name = picture_name(&key);
                let dir = pictures_dir();
                let _ = std::fs::create_dir_all(&dir);
                if let Ok((w, h)) = ws.player.editor_picture(id, which, &dir.join(&name)) {
                    let fact = ws.plugin_page.facts.entry(key).or_default();
                    (fact.picture, fact.w, fact.h) = (Some(name), w, h);
                    save_facts(&ws.plugin_page.facts);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }
}
