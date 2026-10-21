//! Saving, keeping unsaved edits to offer back, quitting, and the plugin scan at start.

use super::*;

/// How often unsaved edits are kept, for if Pina Studio stops before they're saved.
pub(crate) const KEEP_EVERY: Duration = Duration::from_secs(30);

impl Workspace {
    pub fn save(&mut self, cx: &mut Context<Self>) -> bool {
        let result = self
            .player
            .save_states(&mut self.session.project)
            .and_then(|()| self.session.save());
        if result.is_ok() {
            self.session.forget_kept();
        }
        self.status = Some(match &result {
            Ok(()) => format!("Saved {}", self.session.dir.display()),
            Err(e) => e.clone(),
        });
        cx.notify();
        result.is_ok()
    }

    /// Keeps unsaved edits in the project's recovery folder, for if Pina Studio stops before they're
    /// saved: it's offered them when the project next opens.
    pub fn keep_edits(&mut self) {
        if !self.session.edited || self.session.kept {
            return;
        }
        let mut project = self.session.project.clone();
        let states = Path::new(RECOVERY_DIR).join(STATES_DIR);
        let result = self
            .player
            .save_states_in(&mut project, &states)
            .and_then(|()| self.session.keep(&project));
        if let Err(e) = result {
            self.status = Some(format!("Couldn't keep unsaved edits: {e}"));
        }
    }

    /// Asks whether to restore edits kept from when Pina Studio last stopped without saving them.
    pub fn offer_kept(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(at) = self.session.kept_at() else {
            return;
        };
        let detail = format!(
            "Pina Studio closed before they were saved. They were kept {}.",
            ago(at)
        );
        // After the window is up, for the question to sit on it.
        cx.spawn_in(window, async move |this, cx| {
            let Ok(answer) = this.update_in(cx, |_, window, cx| {
                window.prompt(
                    PromptLevel::Warning,
                    "Restore your unsaved changes to this project?",
                    Some(&detail),
                    &["Restore", "Discard"],
                    cx,
                )
            }) else {
                return;
            };
            let Ok(choice) = answer.await else { return };
            this.update(cx, |ws, cx| {
                if choice == 0 {
                    ws.restore_kept(cx);
                } else {
                    ws.session.forget_kept();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Opens the kept edits in place of the saved project (undo goes back to it).
    pub fn restore_kept(&mut self, cx: &mut Context<Self>) {
        if let Err(e) = self.session.restore() {
            self.status = Some(format!("Couldn't restore unsaved changes: {e}"));
            cx.notify();
            return;
        }
        // Every plugin anew, from the settings kept with the edits.
        let rate = self.session.project.sample_rate;
        let _ = self.player.sync(&Project::new(rate));
        self.changed(cx);
        self.status = Some("Restored unsaved changes".into());
    }

    /// Quits, asking first if there are unsaved changes.
    pub(crate) fn quit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.session.edited {
            cx.quit();
            return;
        }
        let answer = window.prompt(
            PromptLevel::Warning,
            "Save changes to this project before quitting?",
            None,
            &["Save", "Don't Save", "Cancel"],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let Ok(choice) = answer.await else { return };
            this.update(cx, |this, cx| match choice {
                0 if this.save(cx) => cx.quit(),
                1 => {
                    this.session.forget_kept();
                    cx.quit()
                }
                _ => {}
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn should_close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.session.edited {
            self.quit(window, cx);
        }
        !self.session.edited
    }

    /// Finds the installed plugins in the background, for the plugin menus.
    pub fn start_scan(&mut self, cx: &mut Context<Self>) {
        let Ok(exe) = std::env::current_exe() else {
            self.plugins = Some(Vec::new());
            return;
        };
        let found = cx.background_spawn(async move {
            scan::scan(&exe, &scan::everything(), &scan::cache_file())
        });
        cx.spawn(async move |this, cx| {
            let plugins = found.await;
            this.update(cx, |ws, cx| {
                ws.plugins = Some(plugins);
                ws.catalog_plugins();
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}
