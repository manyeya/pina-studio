//! The audio a song plays: files read once and kept, stretched versions made in the background, and what stands in while they are.

use super::*;

/// An audio file in memory, one or two channels.
// ponytail: whole files in RAM; stream from disk when sessions outgrow memory.
pub struct AudioData {
    pub(crate) channels: Vec<Vec<f32>>,
}

impl AudioData {
    /// Audio from its channels (one or two), all as long.
    pub fn new(channels: Vec<Vec<f32>>) -> Self {
        AudioData { channels }
    }

    pub fn channels(&self) -> &[Vec<f32>] {
        &self.channels
    }

    #[cfg(test)]
    pub(crate) fn mono(samples: Vec<f32>) -> Self {
        AudioData {
            channels: vec![samples],
        }
    }
}

/// A stretched copy of a file: the file, and how it's fitted.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub(crate) struct Stretched {
    pub(crate) file: PathBuf,
    /// The fit's numbers, as bits.
    pub(crate) ratio: u64,
    pub(crate) semitones: u64,
    pub(crate) flex: Flex,
    pub(crate) formants: bool,
    pub(crate) warps: Vec<(usize, usize)>,
}

impl Stretched {
    pub(crate) fn new(file: &Path, fit: &Fit) -> Stretched {
        Stretched {
            file: file.to_path_buf(),
            ratio: fit.ratio.to_bits(),
            semitones: fit.semitones.to_bits(),
            flex: fit.flex,
            formants: fit.formants,
            warps: fit.warps.clone(),
        }
    }

    pub(crate) fn fit(&self) -> Fit {
        Fit {
            ratio: f64::from_bits(self.ratio),
            semitones: f64::from_bits(self.semitones),
            flex: self.flex,
            formants: self.formants,
            warps: self.warps.clone(),
        }
    }
}

/// The audio files a project uses, each loaded once, with its waveform overview.
pub struct AudioFiles {
    pub(crate) dir: PathBuf,
    pub(crate) rate: f64,
    pub(crate) files: HashMap<PathBuf, (Arc<AudioData>, Arc<Peaks>)>,
    /// Stretched copies; those asked for since `begin`, which are kept when the arrangement's
    /// made (with the other copies of the files they're of, a few, to stand in while new ones
    /// are made).
    pub(crate) stretched: HashMap<Stretched, Arc<AudioData>>,
    pub(crate) asked: HashSet<Stretched>,
    /// For a player that can't wait: copies made off its thread.
    pub(crate) renders: Option<Arc<Renders>>,
    /// Where each file's hits are (see `stretch::onsets`), found when first asked for.
    pub(crate) hits: Mutex<HashMap<PathBuf, Arc<Vec<usize>>>>,
}

/// Stretching off the main thread, two at a time, the newest wanted first: what the arrangement
/// wants that isn't made yet, what's being made, and what's made.
#[derive(Default)]
pub(crate) struct Renders {
    pub(crate) state: Mutex<RenderState>,
    pub(crate) work: Condvar,
    /// Something new is made: the arrangement can be made again with it.
    pub(crate) fresh: AtomicBool,
}

#[derive(Default)]
pub(crate) struct RenderState {
    pub(crate) wanted: Vec<(Stretched, Arc<AudioData>)>,
    pub(crate) busy: HashSet<Stretched>,
    pub(crate) made: Vec<(Stretched, Arc<AudioData>)>,
    pub(crate) workers: usize,
}

/// A render thread: makes what's wanted till nothing has been for a while, or the files go.
pub(crate) fn render_thread(renders: Weak<Renders>, rate: f64) {
    loop {
        let Some(r) = renders.upgrade() else {
            return;
        };
        let job = {
            let mut s = r.state.lock().unwrap();
            loop {
                if let Some(i) = s.wanted.iter().rposition(|(k, _)| !s.busy.contains(k)) {
                    let (key, source) = s.wanted.remove(i);
                    s.busy.insert(key.clone());
                    break Some((key, source));
                }
                let (next, waited) = r.work.wait_timeout(s, Duration::from_secs(5)).unwrap();
                s = next;
                if waited.timed_out() && s.wanted.is_empty() {
                    s.workers -= 1;
                    break None;
                }
            }
        };
        let Some((key, source)) = job else {
            return;
        };
        let data = Arc::new(stretch::render(&source, &key.fit(), rate));
        let mut s = r.state.lock().unwrap();
        s.busy.remove(&key);
        s.made.push((key, data));
        r.fresh.store(true, Ordering::Relaxed);
    }
}

/// `data` read faster or slower to `len`, roughly: what plays while a stretch is made.
pub(crate) fn stand_in(data: &AudioData, len: usize) -> AudioData {
    let channels = data
        .channels
        .iter()
        .map(|c| {
            let step = c.len() as f64 / len.max(1) as f64;
            (0..len)
                .map(|i| {
                    let x = i as f64 * step;
                    let at = x as usize;
                    let (a, b) = (c.get(at).copied(), c.get(at + 1).copied());
                    let t = (x - at as f64) as f32;
                    a.map_or(0.0, |a| a + (b.unwrap_or(a) - a) * t)
                })
                .collect()
        })
        .collect();
    AudioData { channels }
}

impl AudioFiles {
    pub fn new(dir: &Path, rate: f64) -> Self {
        AudioFiles {
            dir: dir.to_path_buf(),
            rate,
            files: HashMap::new(),
            stretched: HashMap::new(),
            asked: HashSet::new(),
            renders: None,
            hits: Mutex::default(),
        }
    }

    /// A loaded file's length, in frames at the engine's rate.
    pub fn frames(&self, file: &Path) -> Option<usize> {
        self.files.get(file).map(|(d, _)| d.channels[0].len())
    }

    /// Where a loaded file's hits are, in frames at the engine's rate.
    pub fn hits(&self, file: &Path) -> Option<Arc<Vec<usize>>> {
        let mut hits = self.hits.lock().unwrap();
        if let Some(h) = hits.get(file) {
            return Some(h.clone());
        }
        let (data, _) = self.files.get(file)?;
        let found = Arc::new(stretch::onsets(&data.channels, self.rate));
        hits.insert(file.to_path_buf(), found.clone());
        Some(found)
    }

    /// Stretches off the caller's thread: until a copy is made, a stand-in plays.
    pub fn in_background(mut self) -> Self {
        self.renders = Some(Arc::default());
        self
    }

    /// Stretches still being made, or wanted.
    pub fn rendering(&self) -> bool {
        self.renders.as_ref().is_some_and(|r| {
            let s = r.state.lock().unwrap();
            !s.wanted.is_empty() || !s.busy.is_empty()
        })
    }

    /// A stretch has been made since the last call: the arrangement can use it.
    pub fn fresh(&self) -> bool {
        self.renders
            .as_ref()
            .is_some_and(|r| r.fresh.swap(false, Ordering::Relaxed))
    }

    /// An arrangement's being made: what it asks for is what's wanted.
    pub(crate) fn begin(&mut self) {
        self.asked.clear();
        if let Some(r) = &self.renders {
            r.state.lock().unwrap().wanted.clear();
            let made = std::mem::take(&mut r.state.lock().unwrap().made);
            self.stretched.extend(made);
        }
    }

    /// It's made: copies of files it doesn't play go, and (past a few) those it doesn't ask for.
    pub(crate) fn end(&mut self) {
        let files: HashSet<&PathBuf> = self.asked.iter().map(|k| &k.file).collect();
        self.stretched
            .retain(|k, _| self.asked.contains(k) || files.contains(&k.file));
        if self.stretched.len() > 64 {
            self.stretched.retain(|k, _| self.asked.contains(k));
        }
    }

    /// A file fitted as `fit` says (see `crate::audio::stretch`), made once for each.
    pub(crate) fn stretched(
        &mut self,
        file: &Path,
        mut fit: Fit,
    ) -> Result<Arc<AudioData>, String> {
        // Speed can't move pitch without moving time.
        if fit.flex == Flex::Speed {
            fit.semitones = 0.0;
        }
        if fit.ratio == 1.0 && fit.semitones == 0.0 && fit.warps.is_empty() {
            return self.get(file);
        }
        let key = Stretched::new(file, &fit);
        self.asked.insert(key.clone());
        if let Some(data) = self.stretched.get(&key) {
            return Ok(data.clone());
        }
        let source = self.get(file)?;
        let Some(r) = &self.renders else {
            let data = Arc::new(stretch::render(&source, &fit, self.rate));
            self.stretched.insert(key, data.clone());
            return Ok(data);
        };
        {
            let mut s = r.state.lock().unwrap();
            if !s.busy.contains(&key) && !s.wanted.iter().any(|w| w.0 == key) {
                s.wanted.push((key.clone(), source.clone()));
            }
            while s.workers < RENDER_THREADS {
                s.workers += 1;
                let (weak, rate) = (Arc::downgrade(r), self.rate);
                std::thread::spawn(move || render_thread(weak, rate));
            }
            r.work.notify_all();
        }
        // Meanwhile: the nearest copy made (else the file), read faster or slower to fit.
        let len = (source.channels[0].len() as f64 * fit.ratio).round() as usize;
        let alike = |k: &Stretched| {
            k.file == key.file
                && (k.semitones, k.flex, k.formants) == (key.semitones, key.flex, key.formants)
        };
        let nearest = self
            .stretched
            .iter()
            .filter(|(k, _)| alike(k))
            .min_by(|a, b| {
                let off = |k: &Stretched| (f64::from_bits(k.ratio) / fit.ratio).ln().abs();
                off(a.0).total_cmp(&off(b.0))
            })
            .map_or(source, |(_, d)| d.clone());
        Ok(Arc::new(stand_in(&nearest, len)))
    }

    pub(crate) fn get(&mut self, file: &Path) -> Result<Arc<AudioData>, String> {
        if let Some((data, _)) = self.files.get(file) {
            return Ok(data.clone());
        }
        let data = Arc::new(load_wav(&self.dir.join(file), self.rate)?);
        // ponytail: the overview shows the first channel.
        let peaks = Arc::new(Peaks::from_samples(&data.channels[0]));
        self.files.insert(file.to_path_buf(), (data.clone(), peaks));
        Ok(data)
    }

    /// The overview of a file the engine has loaded.
    pub fn peaks(&self, file: &Path) -> Option<Arc<Peaks>> {
        self.files.get(file).map(|(_, p)| p.clone())
    }
}

/// Loaded plugin bundles, by path.
pub type Modules = Libraries;
