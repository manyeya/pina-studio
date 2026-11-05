//! One block of audio: every lane played, its strip, sends and output, the auxes, the master.

use super::*;

impl Engine {
    pub(crate) fn block(&mut self, out: [&mut [f32]; 2], frames: usize) {
        let rate = self.rate as f32;
        self.receive();
        // Live notes go to the armed lane, and back to the main thread as played.
        // ponytail: they land at the start of the block, up to MAX_BLOCK samples late; use
        // CoreMIDI timestamps for sample accuracy.
        let mut live = [Note::default(); LIVE_NOTES];
        let mut live_len = 0;
        let mut armed = usize::MAX;
        // A kit armed: each live note's pad's lane.
        let mut pads = [usize::MAX; 128];
        if let Some(link) = &mut self.link {
            armed = link.shared.armed.load(Ordering::Relaxed);
            if link.shared.kit_armed.load(Ordering::Relaxed) {
                for (p, a) in pads.iter_mut().zip(&link.shared.pads) {
                    *p = a.load(Ordering::Relaxed);
                }
            }
            while live_len < LIVE_NOTES {
                let Ok(n) = link.live.pop() else { break };
                let velocity = n.velocity as f32 / 127.0;
                live[live_len] = Note {
                    offset: 0,
                    pitch: n.pitch,
                    velocity,
                };
                live_len += 1;
                let played = Played {
                    at: self.position,
                    pitch: n.pitch,
                    velocity: n.velocity,
                };
                let _ = link.played.push(played);
            }
            // Live moves go to the armed lane as its controllers, and back as played.
            while let Ok(kind) = link.moves.pop() {
                if armed != usize::MAX && self.moved_len < LIVE_NOTES {
                    self.moved[self.moved_len] = (armed, kind);
                    self.moved_len += 1;
                }
                let _ = link.heard.push((self.position, kind));
            }
        }
        let pending = std::mem::take(&mut self.pending_len);
        let controllers = std::mem::take(&mut self.moved_len);
        let [out_l, out_r] = out;
        out_l[..frames].fill(0.0);
        out_r[..frames].fill(0.0);
        // What's heard now is `position`; the engine reads ahead by the plugins' latency, which
        // delay compensation then makes up, so it's heard on time. Play just started, it reads
        // that lookahead first (`preroll`), the playhead waiting.
        let heard = self.position;
        let playing = self.playing;
        let rolling = match playing {
            true => (self.preroll as usize).min(frames),
            false => 0,
        };
        // Inside the cycle, reading ahead wraps round to its start, and a block whose reading
        // crosses its end reads the rest from the start.
        let cycle = self
            .arrangement
            .cycle
            .filter(|&(start, end)| playing && heard < end && end > start);
        let mut from = heard + self.arrangement.lookahead - if playing { self.preroll } else { 0 };
        if let Some((start, end)) = cycle
            && from >= end
        {
            from = start + (from - end) % (end - start);
        }
        let to = from + frames as u64;
        let split = cycle
            .filter(|&(_, end)| from < end && to > end)
            .map(|(start, end)| ((end - from) as usize, start));
        // Reading somewhere new (a locate, or the cycle going round) lets held notes go.
        let jumped = playing && from != self.next_read;
        self.next_read = match split {
            Some((left, start)) => start + (frames - left) as u64,
            None => to,
        };
        let release = std::mem::take(&mut self.release) || jumped;
        let rechase = std::mem::take(&mut self.rechase) && playing;
        let shared = self.link.as_ref().map(|l| &*l.shared);
        let arrangement = &mut *self.arrangement;
        // Where the song is in beats here, and its tempo, as the tempo map says.
        let (beats, tempo) = (arrangement.tempo.beats(from), arrangement.tempo.tempo(from));
        for bus in &mut arrangement.buses {
            bus[0][..frames].fill(0.0);
            bus[1][..frames].fill(0.0);
        }
        // Stacks' shared modulators, before the tracks they move. With no notes of their own,
        // an envelope on a stack never starts.
        for (aux, plan) in arrangement.auxes.iter().enumerate() {
            let Some(lane) = self.lanes[plan.slot].as_deref_mut() else {
                continue;
            };
            for (k, kind) in plan.mods.iter().enumerate() {
                let input = ModInput {
                    beats: playing.then_some(beats),
                    seconds: frames as f64 / self.rate,
                    tempo,
                    trigger: false,
                    gate: false,
                    follow: self.peaks.get(plan.follow[k]).copied().unwrap_or(0.0),
                };
                let mut over = ModOver::default();
                for (_, field, points) in plan.mod_auto.iter().filter(|a| a.0 as usize == k) {
                    if let Some(v) = value_at(points, from as f64) {
                        over.set(*field, v);
                    }
                }
                arrangement.stack_mods[aux][k] =
                    modulate::step_over(kind, &mut lane.mods[k], &input, &over);
            }
        }

        // Tracks that don't hear each other at once; then, in order, what they share out: their
        // sends, the buses and the mix, so it comes to the same whoever worked each out.
        // Silence between songs is no reason to keep waking threads.
        let idle = !playing && live_len == 0 && pending == 0 && self.took < 0.02;
        self.crew.rest(idle);
        let crew = &self.crew;
        let turn = Turn {
            frames,
            rate: self.rate,
            from,
            to,
            split,
            playing,
            release,
            rechase,
            beats,
            tempo,
            armed,
            pads: &pads,
            pending: &self.pending[..pending],
            controllers: &self.moved[..controllers],
            live: &live[..live_len],
            shared,
        };
        let lanes = Lanes::new(&mut self.lanes);
        let mut start = 0;
        for &end in &arrangement.rounds {
            let tracks = &arrangement.order[start..end];
            start = end;
            let peaks = &*self.peaks;
            let (plans, sidechains, stack_mods) = (
                &arrangement.plans,
                &arrangement.sidechains,
                &arrangement.stack_mods,
            );
            crew.run(tracks.len(), |i| {
                let plan = &plans[tracks[i]];
                // Each task is a different track, and each track has its own lane.
                if let Some(lane) = unsafe { lanes.get(plan.slot) } {
                    track(&turn, plan, lane, peaks, sidechains, stack_mods);
                }
            });
            for &t in tracks {
                let plan = &arrangement.plans[t];
                let Some(lane) = (unsafe { lanes.get(plan.slot) }) else {
                    continue;
                };
                let [l, r] = &mut *lane.buf;
                let (l, r) = (&mut l[..frames], &mut r[..frames]);
                let (moved, reduction) = (lane.moved, lane.reduction);
                // Gain ramps across the block, so automation moves smoothly.
                let later = if playing {
                    heard + frames as u64
                } else {
                    heard
                };
                // Pre-fader sends: muting still silences them.
                for (k, &(aux, _, _)) in plan.sends.iter().enumerate().filter(|s| s.1.2) {
                    let bus = &mut arrangement.buses[aux];
                    let moved = db(moved.sends.get(k).copied().unwrap_or(0.0));
                    let at = |s: u64| plan.send_at(k, s) * plan.audible_at(s) * moved;
                    let (a, step) = (at(heard), (at(later) - at(heard)) / frames as f32);
                    for i in 0..frames {
                        let level = a + step * i as f32;
                        bus[0][i] += l[i] * level;
                        bus[1][i] += r[i] * level;
                    }
                }
                let now = (moved.volume, moved.pan);
                let (m0, m1) = (plan.matrix(heard, lane.mod_last), plan.matrix(later, now));
                lane.mod_last = now;
                let step: [f32; 4] = std::array::from_fn(|k| (m1[k] - m0[k]) / frames as f32);
                for i in 0..frames {
                    let k = i as f32;
                    let m: [f32; 4] = std::array::from_fn(|j| m0[j] + step[j] * k);
                    let (a, b) = (l[i], r[i]);
                    l[i] = a * m[0] + b * m[1];
                    r[i] = a * m[2] + b * m[3];
                }
                if plan.pan_mode == PanMode::Binaural {
                    let place = Place {
                        azimuth: modulate::apply_pan(plan.pan_at(heard), moved.pan) * 180.0,
                        ..plan.place
                    };
                    lane.binaural
                        .process(&mut l[..frames], &mut r[..frames], place, rate);
                }
                let mut peak = [0f32; 2];
                for i in 0..frames {
                    peak = [peak[0].max(l[i].abs()), peak[1].max(r[i].abs())];
                }
                // What plugins hearing it as a sidechain get: it, after its fader.
                if plan.feeds {
                    let [sl, sr] = &mut arrangement.sidechains[t];
                    sl[..frames].copy_from_slice(l);
                    sr[..frames].copy_from_slice(r);
                }
                // Post-fader sends.
                for (k, &(aux, _, _)) in plan.sends.iter().enumerate().filter(|s| !s.1.2) {
                    let moved = db(moved.sends.get(k).copied().unwrap_or(0.0));
                    let at = |s: u64| plan.send_at(k, s) * moved;
                    let (a, step) = (at(heard), (at(later) - at(heard)) / frames as f32);
                    let bus = &mut arrangement.buses[aux];
                    for i in 0..frames {
                        let level = a + step * i as f32;
                        bus[0][i] += l[i] * level;
                        bus[1][i] += r[i] * level;
                    }
                }
                lane.post.process(l, r, plan.direct);
                let (dest_l, dest_r) = match plan.output {
                    Some(aux) => {
                        let [bl, br] = &mut arrangement.buses[aux];
                        (&mut bl[..frames], &mut br[..frames])
                    }
                    None => (&mut out_l[..frames], &mut out_r[..frames]),
                };
                for i in 0..frames {
                    dest_l[i] += l[i];
                    dest_r[i] += r[i];
                }
                if let Some(s) = shared {
                    s.eq_tap.write_ext(plan.slot, l, r);
                    s.eq_tap.write_lane(plan.slot, l, r);
                    s.report(plan.slot, peak, reduction);
                    if s.watches_poly(plan.slot) {
                        s.write_poly_scope(l, r);
                    }
                }
                if let Some(p) = self.peaks.get_mut(plan.slot) {
                    *p = peak[0].max(peak[1]);
                }
            }
        }

        let mut start = 0;
        for &end in &arrangement.aux_rounds {
            let (first, auxes) = (start, &arrangement.auxes[start..end]);
            start = end;
            let (buses, sidechains) = (&arrangement.buses, &arrangement.sidechains);
            crew.run(auxes.len(), |a| {
                // As for tracks: a round's lanes are all different.
                if let Some(lane) = unsafe { lanes.get(auxes[a].slot) } {
                    aux(&turn, &auxes[a], lane, &buses[first + a], sidechains);
                }
            });
            for plan in auxes {
                let Some(lane) = (unsafe { lanes.get(plan.slot) }) else {
                    continue;
                };
                let [l, r] = &mut *lane.buf;
                let (l, r) = (&mut l[..frames], &mut r[..frames]);
                let reduction = lane.reduction;
                let mut peak = [0f32; 2];
                for i in 0..frames {
                    let (a, b) = (l[i] * plan.gain[0], r[i] * plan.gain[1]);
                    out_l[i] += a;
                    out_r[i] += b;
                    peak = [peak[0].max(a.abs()), peak[1].max(b.abs())];
                }
                if let Some(s) = shared {
                    s.eq_tap.write_ext(plan.slot, l, r);
                    s.eq_tap.write_lane(plan.slot, l, r);
                    s.report(plan.slot, peak, reduction);
                    if s.watches_poly(plan.slot) {
                        s.write_poly_scope(l, r);
                    }
                }
            }
        }

        let eq_sc = sidechain(
            &arrangement.sidechains,
            arrangement.master_sidechains.last(),
            frames,
        );
        let mut lane = arrangement
            .master_lane
            .and_then(|s| self.lanes[s].as_deref_mut());
        let (mut reduction, mut i, mut j) = (0f32, 0, 0);
        for &fx in &arrangement.master_chain {
            let (l, r) = (&mut out_l[..frames], &mut out_r[..frames]);
            if fx == Fx::More {
                let dsp = arrangement.master_strip.more.get(j);
                if let (Some(s), Some(d)) =
                    (lane.as_mut().and_then(|lane| lane.more.get_mut(j)), dsp)
                {
                    reduction = reduction.max(s.run_alone(
                        d,
                        l,
                        r,
                        eq_sc,
                        shared,
                        (MASTER, j),
                        Clock { bpm: tempo, beats },
                    ));
                }
                j += 1;
                continue;
            }
            if fx != Fx::Plugin {
                let dsp = &arrangement.master_strip;
                let gr = self.master_strip.run(fx, dsp, l, r, shared, MASTER, eq_sc);
                reduction = reduction.max(gr);
                continue;
            }
            if let Some(p) = lane.as_mut().and_then(|lane| lane.inserts.get_mut(i)) {
                p.set_transport(playing, from, beats, tempo);
                let sides = arrangement.master_sidechains.get(i);
                let sc = sidechain(&arrangement.sidechains, sides, frames);
                p.process([l, r], frames, &[], &[], sc);
            }
            i += 1;
        }
        // The master's sound, for the instance list: the mix through its EQ and inserts.
        if let Some(s) = shared {
            s.eq_tap
                .write_lane(MASTER, &out_l[..frames], &out_r[..frames]);
        }
        // A preview plays over the mix, after the master's fader.
        if let Some(a) = self.audition.as_deref_mut().filter(|a| !a.done()) {
            let channels = &a.data.channels;
            let len = channels[0].len();
            // In time with the song: from where the song is in the loop.
            if a.synced && !a.started && playing && len > 1 {
                a.pos = (heard % len as u64) as f64;
            }
            a.started = true;
            for i in 0..frames {
                if a.synced && playing && a.pos as usize + 1 >= len {
                    a.pos -= (len - 1) as f64;
                }
                let at = a.pos as usize;
                if at + 1 >= len {
                    break;
                }
                let t = (a.pos - at as f64) as f32;
                for (c, out) in [&mut *out_l, &mut *out_r].into_iter().enumerate() {
                    let ch = &channels[c.min(channels.len() - 1)];
                    out[i] += (ch[at] + (ch[at + 1] - ch[at]) * t) * a.gain
                        / arrangement.master.max(1e-6);
                }
                a.pos += a.step;
            }
        }
        let mut peak = [0f32; 2];
        for (side, buf) in [&mut *out_l, &mut *out_r].into_iter().enumerate() {
            for s in buf[..frames].iter_mut() {
                *s *= arrangement.master;
                peak[side] = peak[side].max(s.abs());
            }
        }
        let metronome = shared.is_some_and(Shared::metronome);
        if playing && metronome && rolling < frames {
            click(
                &mut self.click,
                heard,
                &arrangement.tempo,
                (arrangement.beat, arrangement.beats_per_bar),
                self.rate,
                [&mut out_l[rolling..frames], &mut out_r[rolling..frames]],
            );
        }
        if playing {
            self.preroll -= rolling as u64;
            self.position = heard + (frames - rolling) as u64;
        }
        if let Some(s) = shared {
            s.report(MASTER, peak, reduction);
            s.position.store(self.position, Ordering::Relaxed);
            s.playing.store(self.playing, Ordering::Relaxed);
        }
    }
}

/// What every lane is told about a block.
struct Turn<'a> {
    frames: usize,
    rate: f64,
    from: u64,
    to: u64,
    /// Where a block's reading crosses the cycle's end, and where it goes on from.
    split: Option<(usize, u64)>,
    playing: bool,
    release: bool,
    rechase: bool,
    beats: f64,
    tempo: f64,
    armed: usize,
    pads: &'a [usize; 128],
    pending: &'a [(usize, Note)],
    controllers: &'a [(usize, ControlKind)],
    live: &'a [Note],
    shared: Option<&'a Shared>,
}

/// The lanes, for tasks working on different ones at once.
#[derive(Clone, Copy)]
struct Lanes {
    first: *mut Option<Box<Lane>>,
    len: usize,
}

unsafe impl Sync for Lanes {}

impl Lanes {
    fn new(lanes: &mut [Option<Box<Lane>>]) -> Lanes {
        let len = lanes.len();
        let first = lanes.as_mut_ptr();
        Lanes { first, len }
    }

    /// Nothing else may be using `slot` for as long as this is held.
    unsafe fn get<'a>(self, slot: usize) -> Option<&'a mut Lane> {
        assert!(slot < self.len);
        unsafe { (*self.first.add(slot)).as_deref_mut() }
    }
}

/// A track's own work: its notes, modulators, instrument and effects, up to its fader, into its
/// lane's buffer. This touches nothing but its lane, so any thread can do it.
fn track(
    turn: &Turn,
    plan: &Plan,
    lane: &mut Lane,
    peaks: &[f32],
    sidechains: &[[Vec<f32>; 2]],
    stack_mods: &[[f32; MAX_MODS]],
) {
    let &Turn {
        frames,
        rate,
        from,
        to,
        split,
        playing,
        release,
        rechase,
        beats,
        tempo,
        armed,
        pads,
        pending,
        controllers,
        live,
        shared,
    } = turn;
    let [l, r] = &mut *lane.buf;
    let (l, r) = (&mut l[..frames], &mut r[..frames]);
    l.fill(0.0);
    r.fill(0.0);
    let mut g = Gathered::new();
    if release {
        lane.held.release(0, |_| false, &mut g.notes, &mut g.n);
        lane.live.release(0, |_| false, &mut g.notes, &mut g.n);
        lane.midi_fx.stop();
    } else if rechase && lane.cell.is_none() {
        // An edit while playing: what the song still has sounding now rings on.
        let still = |p: u8| sounding(&plan.events, from, p);
        lane.held.release(0, still, &mut g.notes, &mut g.n);
    }
    // A launched cell takes over when its sample comes, held notes let go.
    if let Some((cell, at)) = lane.queued
        && at < to
    {
        lane.cell = cell.map(|c| (c, at));
        lane.queued = None;
        lane.held.release(0, |_| false, &mut g.notes, &mut g.n);
    }
    if playing {
        let into = [&mut *l, &mut *r];
        match lane
            .cell
            .and_then(|(c, start)| Some((plan.cells.get(c)?, start)))
        {
            // Looping: the part of the loop this block covers, wrapping round, from the
            // sample it starts on if that's in this block.
            Some((cell, start)) => {
                let lead = start.saturating_sub(from).min(frames as u64);
                let span = frames as u64 - lead;
                let p0 = from.max(start).saturating_sub(start) % cell.length;
                let first = (cell.length - p0).min(span);
                let held = &mut lane.held;
                let (e, k, c) = (&cell.events, &cell.controls, &cell.clips);
                let lead = lead as usize;
                gather(e, k, c, (p0, p0 + first), lead, &mut g, held, into);
                if first < span {
                    let into = [&mut *l, &mut *r];
                    let at = lead + first as usize;
                    gather(e, k, c, (0, span - first), at, &mut g, held, into);
                }
            }
            None => {
                let (e, k, c) = (&plan.events, &plan.controls, &plan.clips);
                let held = &mut lane.held;
                match split {
                    Some((left, start)) => {
                        let end = (from, from + left as u64);
                        gather(e, k, c, end, 0, &mut g, held, into);
                        // Round to the cycle's start: what's held lets go there.
                        held.release(left as u32, |_| false, &mut g.notes, &mut g.n);
                        let into = [&mut *l, &mut *r];
                        let rest = (start, start + (frames - left) as u64);
                        gather(e, k, c, rest, left, &mut g, held, into);
                    }
                    None => gather(e, k, c, (from, to), 0, &mut g, held, into),
                }
            }
        }
    }
    let now = pending
        .iter()
        .filter(|(s, _)| *s == plan.slot)
        .map(|(_, n)| n);
    let slot = plan.slot;
    let live = live
        .iter()
        .filter(|n| slot == armed || pads[n.pitch as usize & 127] == slot);
    for note in now.chain(live) {
        if g.n < BLOCK_NOTES {
            lane.live.apply(note.pitch, note.velocity > 0.0);
            g.notes[g.n] = *note;
            g.n += 1;
        }
    }
    for (_, kind) in controllers.iter().filter(|c| c.0 == slot) {
        if g.m < BLOCK_CONTROLS {
            g.controls[g.m] = Control {
                offset: 0,
                kind: *kind,
            };
            g.m += 1;
        }
    }
    // Notes from the song, a cell's loop and live playing can interleave.
    g.notes[..g.n].sort_by_key(|n| n.offset);
    g.controls[..g.m].sort_by_key(|c| c.offset);
    let (notes, controls) = (&g.notes[..g.n], &g.controls[..g.m]);
    // Through its MIDI effects, in time with the song while it plays.
    let (notes, controls) = match plan.midi_fx.is_empty() {
        true => (notes, controls),
        false => {
            let clock = crate::dsp::midi_fx::Clock {
                beats: playing.then_some(beats),
                tempo,
                rate,
            };
            lane.midi_fx
                .run_all(&plan.midi_fx, plan.key, (notes, controls), frames, clock)
        }
    };
    // Its modulators, this block: in time with the song while it plays, set off by its
    // notes, hearing the tracks they follow as they were last block.
    let mut moved = Offsets::default();
    if !plan.mods.is_empty() {
        let input = ModInput {
            beats: playing.then_some(beats),
            seconds: frames as f64 / rate,
            tempo,
            trigger: notes.iter().any(|n| n.velocity > 0.0),
            gate: lane.held.any() || lane.live.any(),
            follow: 0.0,
        };
        for (k, m) in plan.mods.iter().enumerate().take(MAX_MODS) {
            let heard = peaks.get(plan.follow[k]).copied().unwrap_or(0.0);
            let input = ModInput {
                follow: heard,
                ..input
            };
            // Its settings where their automation has them at the block's start.
            let mut over = ModOver::default();
            for (_, field, points) in plan.mod_auto.iter().filter(|a| a.0 as usize == k) {
                if let Some(v) = value_at(points, from as f64) {
                    over.set(*field, v);
                }
            }
            let v = match m.kind {
                // Its stack's, as worked out this block.
                ModKind::Shared { modulator } => plan
                    .output
                    .and_then(|aux| stack_mods.get(aux))
                    .and_then(|v| v.get(modulator as usize))
                    .copied()
                    .unwrap_or(0.0),
                _ => modulate::step_over(&m.kind, &mut lane.mods[k], &input, &over),
            };
            if let Some(s) = shared {
                s.report_mod(plan.slot, k, v);
            }
            for route in &m.routes {
                moved.add(route.target, route.depth * v);
            }
        }
    }
    match &mut lane.source {
        Source::Audio => {}
        Source::Synth(s) => s.process([&mut *l, &mut *r], frames, notes),
        Source::Drums(d) => d.process([&mut *l, &mut *r], frames, notes),
        Source::Sampler(s) => {
            s.set_tempo(tempo as f32);
            if let Some(settings) = &plan.sampler {
                s.follow(settings, Arc::as_ptr(settings) as usize);
                // Its automation where the block starts, under its modulators.
                let mut voice = settings.voice;
                for (knob, points) in &plan.voice_auto {
                    if let (Some(k), Some(v)) =
                        (VOICE.get(*knob as usize), value_at(points, from as f64))
                    {
                        (k.set)(&mut voice, v);
                        moved.voice = true;
                    }
                }
                s.modulate(&voice, &moved);
            }
            s.process([&mut *l, &mut *r], frames, notes, controls)
        }
        Source::Poly(s) => {
            if let Some(p) = &plan.poly {
                s.set_tempo(tempo as f32);
                let auto = plan.poly_auto.iter().filter_map(|(i, points)| {
                    let v = value_at(points, from as f64)?;
                    Some((*i as usize, v - p.pos(*i as usize)))
                });
                s.begin(p, auto, &moved);
                s.process(p, [&mut *l, &mut *r], frames, notes, controls);
                s.outward(p, &mut moved);
                if let Some(shared) = shared.filter(|sh| sh.watches_poly(slot)) {
                    shared.write_poly_live(&s.live(p));
                }
            }
        }
        Source::Plugin(p) => {
            plan.automate(Which::Instrument, p, (from, to), &moved);
            p.set_transport(playing, from, beats, tempo);
            p.process([&mut *l, &mut *r], frames, notes, controls, None);
        }
    }
    // What the EQ's dynamic bands can listen to: a track it names, as a plugin would.
    let eq_sc = sidechain(sidechains, plan.sidechains.last(), frames);
    let automated = (!plan.eq_auto.is_empty() && plan.dsp.eq.is_some())
        || (!plan.limit_auto.is_empty() && plan.dsp.limit.is_some())
        || !plan.tool_auto.is_empty();
    let held;
    let dsp = match moved.strip || automated {
        false => &plan.dsp,
        true => {
            let mut dsp = StripDsp {
                eq: plan.dsp.eq,
                made: plan.dsp.made.clone(),
                comp: plan.dsp.comp,
                sat: plan.dsp.sat,
                limit: plan.dsp.limit,
                unit: None,
                more: plan.dsp.more.clone(),
            };
            // The EQ's automation where the block starts, then its modulators.
            if let Some(e) = dsp.eq.as_mut() {
                for (band, field, points) in &plan.eq_auto {
                    let (Some(b), Some(v)) = (
                        e.bands.get_mut(*band as usize),
                        value_at(points, from as f64),
                    ) else {
                        continue;
                    };
                    match field {
                        EqField::Freq => b.freq = v.clamp(10.0, 30_000.0),
                        EqField::Gain => b.gain_db = v.clamp(-30.0, 30.0),
                        EqField::Q => b.q = v.clamp(0.025, 40.0),
                    }
                }
            }
            // The limiter's, likewise.
            if let Some(l) = dsp.limit.as_mut() {
                for (field, points) in &plan.limit_auto {
                    if let Some(v) = value_at(points, from as f64) {
                        field.set(l, v);
                    }
                }
            }
            // Its own compressor's and saturator's knobs, as any stacked effect's.
            if let Some(c) = dsp.comp.as_mut() {
                let mut u = Unit::Comp(*c);
                plan.move_tool(OWN_COMP, from, &moved, &mut u);
                if let Unit::Comp(now) = u {
                    *c = now;
                }
            }
            if let Some(s) = dsp.sat.as_mut() {
                let mut u = Unit::Sat(*s);
                plan.move_tool(OWN_SAT, from, &moved, &mut u);
                if let Unit::Sat(now) = u {
                    *s = now;
                }
            }
            dsp.eq.iter_mut().for_each(|e| moved.eq(e));
            dsp.comp.iter_mut().for_each(|c| moved.comp(c));
            dsp.sat.iter_mut().for_each(|s| moved.sat(s));
            dsp.limit.iter_mut().for_each(|l| moved.limit(l));
            held = dsp;
            &held
        }
    };
    let (mut reduction, mut i, mut j) = (0f32, 0, 0);
    for &fx in &plan.chain {
        if fx == Fx::More {
            if let (Some(s), Some(d)) = (lane.more.get_mut(j), dsp.more.get(j)) {
                let held = plan.moved_more(j, d, from, &moved);
                reduction = reduction.max(s.run_alone(
                    held.as_ref().unwrap_or(d),
                    l,
                    r,
                    eq_sc,
                    shared,
                    (plan.slot, j),
                    Clock { bpm: tempo, beats },
                ));
            }
            j += 1;
            continue;
        }
        if fx != Fx::Plugin {
            let gr = lane.strip.run(fx, dsp, l, r, shared, plan.slot, eq_sc);
            reduction = reduction.max(gr);
            continue;
        }
        if let Some(p) = lane.inserts.get_mut(i) {
            plan.automate(Which::Insert(i), p, (from, to), &moved);
            p.set_transport(playing, from, beats, tempo);
            let sc = sidechain(sidechains, plan.sidechains.get(i), frames);
            p.process([&mut *l, &mut *r], frames, &[], &[], sc);
        }
        i += 1;
    }
    lane.pre.process(l, r, plan.delay);
    lane.moved = moved;
    lane.reduction = reduction;
}

/// An aux's bus through its effects into its lane's buffer, on any thread as a track's is.
fn aux(
    turn: &Turn,
    plan: &AuxPlan,
    lane: &mut Lane,
    bus: &[Vec<f32>; 2],
    sidechains: &[[Vec<f32>; 2]],
) {
    let &Turn {
        frames,
        from,
        playing,
        beats,
        tempo,
        shared,
        ..
    } = turn;
    let [l, r] = &mut *lane.buf;
    let (l, r) = (&mut l[..frames], &mut r[..frames]);
    l.copy_from_slice(&bus[0][..frames]);
    r.copy_from_slice(&bus[1][..frames]);
    let eq_sc = sidechain(sidechains, plan.sidechains.last(), frames);
    let (mut reduction, mut i, mut j) = (0f32, 0, 0);
    for &fx in &plan.chain {
        if fx == Fx::More {
            if let (Some(s), Some(d)) = (lane.more.get_mut(j), plan.dsp.more.get(j)) {
                reduction = reduction.max(s.run_alone(
                    d,
                    l,
                    r,
                    eq_sc,
                    shared,
                    (plan.slot, j),
                    Clock { bpm: tempo, beats },
                ));
            }
            j += 1;
            continue;
        }
        if fx != Fx::Plugin {
            let gr = lane
                .strip
                .run(fx, &plan.dsp, l, r, shared, plan.slot, eq_sc);
            reduction = reduction.max(gr);
            continue;
        }
        if let Some(p) = lane.inserts.get_mut(i) {
            p.set_transport(playing, from, beats, tempo);
            let sc = sidechain(sidechains, plan.sidechains.get(i), frames);
            p.process([&mut *l, &mut *r], frames, &[], &[], sc);
        }
        i += 1;
    }
    lane.post.process(l, r, plan.delay);
    lane.reduction = reduction;
}
