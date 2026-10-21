//! The sample library: loading it, scanning it in the background, and the folders it reads.

use super::*;

impl Workspace {
    /// Reads the library from disk, then brings it up to date, in the background.
    pub fn load_library(&mut self, cx: &mut Context<Self>) {
        let read = cx.background_spawn(async { Library::load() });
        cx.spawn(async move |this, cx| {
            let library = read.await;
            this.update(cx, |ws, cx| {
                ws.set_library(library, cx);
                ws.rescan(cx);
            })
            .ok();
        })
        .detach();
    }

    /// Takes a library in: what was selected stays selected, and the map is worked out again.
    pub fn set_library(&mut self, library: Library, cx: &mut Context<Self>) {
        let b = &mut self.browser;
        let path =
            |i: Option<usize>| i.and_then(|i| b.library.samples.get(i).map(|s| s.path.clone()));
        let (selected, auditioned) = (path(b.selected), path(b.audition.map(|a| a.0)));
        b.library = library;
        b.loaded = true;
        let find =
            |p: Option<PathBuf>| p.and_then(|p| b.library.samples.iter().position(|s| s.path == p));
        b.selected = find(selected);
        if let Some(a) = &mut b.audition {
            match find(auditioned) {
                Some(i) => a.0 = i,
                None => b.audition = None,
            }
        }
        b.hovered = None;
        b.area = None;
        b.like = None;
        // The map can take a while for a big library.
        let samples = b.library.samples.clone();
        let map = cx.background_spawn(async move { search::map(&samples) });
        cx.spawn(async move |this, cx| {
            let map = map.await;
            this.update(cx, |ws, cx| {
                ws.browser.map = map;
                cx.notify();
            })
            .ok();
        })
        .detach();
        self.find_samples();
        self.find_similar();
        cx.notify();
    }

    /// Analyses what's new or changed in the library's folders, in the background, with
    /// progress in the pane.
    pub fn rescan(&mut self, cx: &mut Context<Self>) {
        if self.browser.scan.is_some() {
            self.browser.rescan = true;
            return;
        }
        let mut library = self.browser.library.clone();
        let progress = Arc::new([AtomicUsize::new(0), AtomicUsize::new(0)]);
        self.browser.scan = Some(progress.clone());
        cx.notify();
        let heard = progress.clone();
        let job = cx.background_spawn(async move {
            let report = library.scan(&|done, of| {
                heard[0].store(done, Ordering::Relaxed);
                heard[1].store(of, Ordering::Relaxed);
            });
            (library, report)
        });
        cx.spawn(async move |this, cx| {
            let (library, report) = job.await;
            this.update(cx, |ws, cx| {
                ws.browser.scan = None;
                // Folders added meanwhile stay, and are scanned next.
                let folders = ws.browser.library.folders.clone();
                ws.set_library(Library { folders, ..library }, cx);
                if !report.failed.is_empty() {
                    let (path, e) = &report.failed[0];
                    ws.status = Some(format!(
                        "Couldn't read {} sample{} ({}: {e})",
                        report.failed.len(),
                        if report.failed.len() == 1 { "" } else { "s" },
                        path.file_name().unwrap_or_default().to_string_lossy()
                    ));
                }
                if std::mem::take(&mut ws.browser.rescan) {
                    ws.rescan(cx);
                }
            })
            .ok();
        })
        .detach();
        // Progress shows as it goes.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(150))
                    .await;
                let scanning = this.update(cx, |ws, cx| {
                    cx.notify();
                    ws.browser.scan.is_some()
                });
                if !matches!(scanning, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
    }

    /// Asks for folders to add to the library, and scans them.
    pub fn add_library_folder(&mut self, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: true,
            prompt: Some("Add to Library".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(dirs))) = picked.await else {
                return;
            };
            this.update(cx, |ws, cx| {
                for dir in dirs {
                    if let Err(e) = ws.browser.library.add_folder(&dir) {
                        ws.status = Some(e);
                    }
                }
                ws.rescan(cx);
            })
            .ok();
        })
        .detach();
    }

    pub fn remove_library_folder(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(dir) = self.browser.library.folders.get(index).cloned() else {
            return;
        };
        let mut library = self.browser.library.clone();
        library.remove_folder(&dir);
        let saved = library.clone();
        cx.background_spawn(async move {
            if let Err(e) = saved.save() {
                eprintln!("library: {e}");
            }
        })
        .detach();
        self.set_library(library, cx);
    }
}
