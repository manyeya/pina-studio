//! The models fitted (small nets, the embedding by LDA, the traits' statistics) and written as code.

use super::*;

/// A fitted model: standardisation, an optional hidden layer (ReLU), and a softmax.
#[derive(Clone)]
pub(crate) struct Net {
    pub(crate) mean: Vec<f32>,
    pub(crate) scale: Vec<f32>,
    pub(crate) hidden: Vec<Vec<f32>>,
    pub(crate) hidden_bias: Vec<f32>,
    pub(crate) weights: Vec<Vec<f32>>,
    pub(crate) bias: Vec<f32>,
}

impl Net {
    pub(crate) fn standardise(&self, x: &[f32]) -> Vec<f32> {
        x.iter()
            .zip(&self.mean)
            .zip(&self.scale)
            .map(|((v, mu), s)| ((v - mu) / s).clamp(-6.0, 6.0))
            .collect()
    }

    /// The hidden layer's output (or the input, with none), and the logits.
    pub(crate) fn forward(&self, z: &[f32]) -> (Vec<f32>, Vec<f32>) {
        let h: Vec<f32> = if self.hidden.is_empty() {
            z.to_vec()
        } else {
            self.hidden
                .iter()
                .zip(&self.hidden_bias)
                .map(|(w, b)| (b + w.iter().zip(z).map(|(a, v)| a * v).sum::<f32>()).max(0.0))
                .collect()
        };
        let logits = self
            .weights
            .iter()
            .zip(&self.bias)
            .map(|(w, b)| b + w.iter().zip(&h).map(|(a, v)| a * v).sum::<f32>())
            .collect();
        (h, logits)
    }

    pub(crate) fn probs(&self, x: &[f32]) -> Vec<f32> {
        let (_, l) = self.forward(&self.standardise(x));
        let top = l.iter().fold(f32::MIN, |a, &v| a.max(v));
        let e: Vec<f32> = l.iter().map(|v| (v - top).exp()).collect();
        let s: f32 = e.iter().sum();
        e.iter().map(|v| v / s).collect()
    }
}

/// Several nets fitted from different starts, their probabilities averaged.
pub(crate) struct Ensemble(Vec<Net>);

impl Ensemble {
    pub(crate) fn predict(&self, x: &[f32]) -> usize {
        let mut sum = vec![0f32; self.0[0].bias.len()];
        for n in &self.0 {
            for (s, p) in sum.iter_mut().zip(n.probs(x)) {
                *s += p;
            }
        }
        (0..sum.len())
            .max_by(|&a, &b| sum[a].total_cmp(&sum[b]))
            .unwrap()
    }
}

pub(crate) fn fit_ensemble(
    xs: &[Vec<f32>],
    ys: &[usize],
    classes: usize,
    hidden: usize,
) -> Ensemble {
    let count = if hidden == 0 { 1 } else { 5 };
    let nets: Vec<Net> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..count)
            .map(|seed| scope.spawn(move || fit_net(xs, ys, classes, hidden, seed as u64)))
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    Ensemble(nets)
}

/// Eigenvalues and eigenvectors (as columns) of a symmetric matrix, by Jacobi rotations.
pub(crate) fn jacobi(mut a: Vec<Vec<f64>>) -> (Vec<f64>, Vec<Vec<f64>>) {
    let n = a.len();
    let mut v: Vec<Vec<f64>> = (0..n)
        .map(|i| (0..n).map(|j| (i == j) as u8 as f64).collect())
        .collect();
    for _ in 0..60 {
        let off: f64 = (0..n)
            .flat_map(|i| (i + 1..n).map(move |j| (i, j)))
            .map(|(i, j)| a[i][j] * a[i][j])
            .sum();
        if off < 1e-18 {
            break;
        }
        for p in 0..n {
            for q in p + 1..n {
                if a[p][q].abs() < 1e-20 {
                    continue;
                }
                let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
                let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
                let t = if theta == 0.0 { 1.0 } else { t };
                let c = 1.0 / (t * t + 1.0).sqrt();
                let s = t * c;
                for row in a.iter_mut() {
                    let (kp, kq) = (row[p], row[q]);
                    row[p] = c * kp - s * kq;
                    row[q] = s * kp + c * kq;
                }
                let (row_p, row_q) = (a[p].clone(), a[q].clone());
                for (k, (pk, qk)) in row_p.into_iter().zip(row_q).enumerate() {
                    a[p][k] = c * pk - s * qk;
                    a[q][k] = s * pk + c * qk;
                }
                for row in v.iter_mut() {
                    let (kp, kq) = (row[p], row[q]);
                    row[p] = c * kp - s * kq;
                    row[q] = s * kp + c * kq;
                }
            }
        }
    }
    ((0..n).map(|i| a[i][i]).collect(), v)
}

/// A linear discriminant projection to `dims` dimensions (rows of the result): within-class
/// scatter whitened (regularised by `reg`), then the directions the classes' (balanced) means
/// spread along most.
pub(crate) fn lda(
    zs: &[Vec<f32>],
    ys: &[usize],
    classes: usize,
    dims: usize,
    reg: f64,
) -> Vec<Vec<f32>> {
    let d = zs[0].len();
    let mut means = vec![vec![0f64; d]; classes];
    let mut counts = vec![0f64; classes];
    for (z, &y) in zs.iter().zip(ys) {
        counts[y] += 1.0;
        for (m, v) in means[y].iter_mut().zip(z) {
            *m += *v as f64;
        }
    }
    for (m, n) in means.iter_mut().zip(&counts) {
        m.iter_mut().for_each(|v| *v /= n.max(1.0));
    }
    let grand: Vec<f64> = (0..d)
        .map(|j| means.iter().map(|m| m[j]).sum::<f64>() / classes as f64)
        .collect();
    let mut within = vec![vec![0f64; d]; d];
    for (z, &y) in zs.iter().zip(ys) {
        let diff: Vec<f64> = (0..d).map(|j| z[j] as f64 - means[y][j]).collect();
        // Each class counts the same, however many it has.
        let w = 1.0 / (counts[y] * classes as f64);
        for i in 0..d {
            for j in 0..d {
                within[i][j] += w * diff[i] * diff[j];
            }
        }
    }
    for (i, row) in within.iter_mut().enumerate() {
        row[i] += reg;
    }
    let mut between = vec![vec![0f64; d]; d];
    for m in &means {
        let diff: Vec<f64> = (0..d).map(|j| m[j] - grand[j]).collect();
        for i in 0..d {
            for j in 0..d {
                between[i][j] += diff[i] * diff[j] / classes as f64;
            }
        }
    }
    let (values, vectors) = jacobi(within);
    // Whitening: W = U Λ^-1/2.
    let w: Vec<Vec<f64>> = (0..d)
        .map(|i| {
            (0..d)
                .map(|k| vectors[i][k] / values[k].max(1e-9).sqrt())
                .collect()
        })
        .collect();
    let wbw: Vec<Vec<f64>> = (0..d)
        .map(|a| {
            (0..d)
                .map(|b| {
                    let mut s = 0.0;
                    for i in 0..d {
                        let wia = w[i][a];
                        if wia == 0.0 {
                            continue;
                        }
                        for j in 0..d {
                            s += wia * between[i][j] * w[j][b];
                        }
                    }
                    s
                })
                .collect()
        })
        .collect();
    let (values, vectors) = jacobi(wbw);
    let mut order: Vec<usize> = (0..d).collect();
    order.sort_by(|&a, &b| values[b].total_cmp(&values[a]));
    order
        .iter()
        .take(dims)
        .map(|&k| {
            (0..d)
                .map(|i| (0..d).map(|j| w[i][j] * vectors[j][k]).sum::<f64>() as f32)
                .collect()
        })
        .collect()
}

/// A small deterministic random source for initialising and shuffling.
pub(crate) struct Rng(u64);

impl Rng {
    pub(crate) fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }
}

/// Fits a classifier (classes balanced, L2-regularised) by Adam on mini-batches: a softmax
/// regression with `hidden` 0, else one hidden ReLU layer of that many units.
pub(crate) fn fit_net(
    xs: &[Vec<f32>],
    ys: &[usize],
    classes: usize,
    hidden: usize,
    seed: u64,
) -> Net {
    let n = xs.len();
    let d = xs[0].len();
    let mean: Vec<f32> = (0..d)
        .map(|j| xs.iter().map(|x| x[j]).sum::<f32>() / n as f32)
        .collect();
    let scale: Vec<f32> = (0..d)
        .map(|j| {
            ((xs.iter().map(|x| (x[j] - mean[j]).powi(2)).sum::<f32>() / n as f32).sqrt()).max(1e-3)
        })
        .collect();
    let mut rng = Rng(0x9e3779b97f4a7c15 ^ (seed.wrapping_add(1).wrapping_mul(0xbf58476d1ce4e5b9)));
    let width = if hidden == 0 { d } else { hidden };
    let init = |rng: &mut Rng, fan_in: usize| {
        (rng.next() * 2.0 - 1.0) * (6.0 / fan_in as f32).sqrt() * 0.5
    };
    let mut net = Net {
        mean,
        scale,
        hidden: (0..hidden)
            .map(|_| (0..d).map(|_| init(&mut rng, d)).collect())
            .collect(),
        hidden_bias: vec![0.01; hidden],
        weights: (0..classes)
            .map(|_| {
                (0..width)
                    .map(|_| {
                        if hidden == 0 {
                            0.0
                        } else {
                            init(&mut rng, width)
                        }
                    })
                    .collect()
            })
            .collect(),
        bias: vec![0.0; classes],
    };
    let z: Vec<Vec<f32>> = xs.iter().map(|x| net.standardise(x)).collect();
    let mut counts = vec![0usize; classes];
    ys.iter().for_each(|&y| counts[y] += 1);
    let weight: Vec<f32> = ys
        .iter()
        .map(|&y| (n as f32 / (classes as f32 * counts[y] as f32)).sqrt())
        .collect();
    // Adam state for every parameter, flattened in the order: hidden, hidden bias, weights, bias.
    let count = hidden * d + hidden + classes * width + classes;
    let (mut m1, mut m2) = (vec![0f32; count], vec![0f32; count]);
    let (lr, l2, b1, b2) = (
        if hidden == 0 { 0.03 } else { 0.004 },
        if hidden == 0 { 0.003 } else { 0.0005 },
        0.9f32,
        0.999f32,
    );
    let epochs = if hidden == 0 { 300 } else { 160 };
    let batch = 64;
    let mut order: Vec<usize> = (0..n).collect();
    let mut t = 0;
    for _ in 0..epochs {
        for i in (1..n).rev() {
            let j = (rng.next() * (i + 1) as f32) as usize % (i + 1);
            order.swap(i, j);
        }
        for chunk in order.chunks(batch) {
            let mut g = vec![0f32; count];
            let wsum: f32 = chunk.iter().map(|&i| weight[i]).sum();
            for &i in chunk {
                let (h, logits) = net.forward(&z[i]);
                let top = logits.iter().fold(f32::MIN, |a, &v| a.max(v));
                let exp: Vec<f32> = logits.iter().map(|l| (l - top).exp()).collect();
                let s: f32 = exp.iter().sum();
                let base = hidden * d + hidden;
                let mut dh = vec![0f32; width];
                for c in 0..classes {
                    let e = (exp[c] / s - (c == ys[i]) as u8 as f32) * weight[i] / wsum;
                    for k in 0..width {
                        g[base + c * width + k] += e * h[k];
                        dh[k] += e * net.weights[c][k];
                    }
                    g[base + classes * width + c] += e;
                }
                if hidden > 0 {
                    for k in 0..hidden {
                        if h[k] <= 0.0 {
                            continue;
                        }
                        for j in 0..d {
                            g[k * d + j] += dh[k] * z[i][j];
                        }
                        g[hidden * d + k] += dh[k];
                    }
                }
            }
            t += 1;
            let (c1, c2) = (1.0 - b1.powi(t), 1.0 - b2.powi(t));
            let mut idx = 0;
            let mut step = |p: &mut f32, grad: f32, decay: bool, idx: usize| {
                let grad = grad + if decay { l2 * *p } else { 0.0 };
                m1[idx] = b1 * m1[idx] + (1.0 - b1) * grad;
                m2[idx] = b2 * m2[idx] + (1.0 - b2) * grad * grad;
                *p -= lr * (m1[idx] / c1) / ((m2[idx] / c2).sqrt() + 1e-8);
            };
            for row in net.hidden.iter_mut() {
                for p in row.iter_mut() {
                    step(p, g[idx], true, idx);
                    idx += 1;
                }
            }
            for p in net.hidden_bias.iter_mut() {
                step(p, g[idx], false, idx);
                idx += 1;
            }
            for row in net.weights.iter_mut() {
                for p in row.iter_mut() {
                    step(p, g[idx], true, idx);
                    idx += 1;
                }
            }
            for p in net.bias.iter_mut() {
                step(p, g[idx], false, idx);
                idx += 1;
            }
        }
    }
    net
}

pub(crate) fn floats(v: &[f32]) -> String {
    let parts: Vec<String> = v.iter().map(|x| format!("{x:.5}")).collect();
    format!("&[{}]", parts.join(", "))
}

pub(crate) fn rows_code(rows: &[Vec<f32>]) -> String {
    if rows.is_empty() {
        return "&[]".into();
    }
    let rows: Vec<String> = rows
        .iter()
        .map(|r| format!("        {},", floats(r)))
        .collect();
    format!("&[\n{}\n    ]", rows.join("\n"))
}

pub(crate) fn net_code(name: &str, classes: &[Category], e: &Ensemble) -> String {
    let nets: Vec<String> = e.0.iter().map(|m| one_net(classes, m)).collect();
    format!("pub static {name}: &[Net] = &[\n{}];\n", nets.join(""))
}

pub(crate) fn one_net(classes: &[Category], m: &Net) -> String {
    let cats: Vec<String> = classes.iter().map(|c| format!("Category::{c:?}")).collect();
    format!(
        "    Net {{\n    classes: &[{}],\n    mean: {},\n    scale: {},\n    hidden: {},\n    hidden_bias: {},\n    weights: {},\n    bias: {},\n}},\n",
        cats.join(", "),
        floats(&m.mean),
        floats(&m.scale),
        rows_code(&m.hidden),
        floats(&m.hidden_bias),
        rows_code(&m.weights),
        floats(&m.bias)
    )
}

pub(crate) fn fit(rows: &[Row], out: &Path) {
    let train: Vec<&Row> = rows
        .iter()
        .filter(|r| !r.truth.held_out && r.feats.is_some())
        .collect();
    println!("fitting on {} files", train.len());
    // Loop or one-shot.
    let kinds: Vec<(&Vec<f32>, usize)> = train
        .iter()
        .filter_map(|r| {
            r.truth
                .kind
                .map(|k| (&r.feats.as_ref().unwrap().0, (k == Kind::Loop) as usize))
        })
        .collect();
    let kind = fit_ensemble(
        &kinds.iter().map(|k| k.0.clone()).collect::<Vec<_>>(),
        &kinds.iter().map(|k| k.1).collect::<Vec<_>>(),
        2,
        0,
    );
    // Categories, one model for one-shots and one for loops.
    let mut code = String::from(
        "//! The fitted constants the analysis uses: written by `examples/sample_eval.rs --fit` from\n//! MusicRadar's free SampleRadar packs (808 drums, 808 weight, essential drum kits, drum\n//! machines, analogue drums, trap, world percussion, lo-fi, synth chords, jazz keys, glitch\n//! vocals, synth impacts, hip-hop essentials, hip hop, bass guitar, MIDI orchestra, noise and\n//! crackle, soul and funk), the 90s Hip Hop pack held out.\n// Fitted numbers, some near a constant by chance.\n#![allow(clippy::approx_constant)]\n\nuse super::Category;\n\n/// A classifier over a standardised feature vector: a hidden ReLU layer (if it has one), then\n/// a softmax.\npub struct Net {\n    pub classes: &'static [Category],\n    pub mean: &'static [f32],\n    pub scale: &'static [f32],\n    pub hidden: &'static [&'static [f32]],\n    pub hidden_bias: &'static [f32],\n    pub weights: &'static [&'static [f32]],\n    pub bias: &'static [f32],\n}\n\n/// Loop (the second class) or one-shot.\n",
    );
    code += &net_code("KIND", &[], &kind);
    let mut one_shot_nets: Option<Ensemble> = None;
    for (kind, name) in [(Kind::OneShot, "ONE_SHOT"), (Kind::Loop, "LOOP")] {
        let labelled: Vec<(&Vec<f32>, Category)> = train
            .iter()
            .filter(|r| r.truth.kind.unwrap_or(r.blind.kind) == kind)
            .filter_map(|r| {
                let x = &r.feats.as_ref().unwrap().0;
                let first = *r.truth.cats.first()?;
                // A hat not named open or closed: open if it rings (decay, ln ms, past 150 ms).
                let c = if kind == Kind::OneShot
                    && r.truth.cats == [Category::HatClosed, Category::HatOpen]
                    && x[2] > 151f32.ln()
                {
                    Category::HatOpen
                } else {
                    first
                };
                Some((x, c))
            })
            .collect();
        let mut classes: Vec<Category> = Vec::new();
        for (_, c) in &labelled {
            if !classes.contains(c) {
                classes.push(*c);
            }
        }
        // Too few to learn from: left to names.
        classes.retain(|c| labelled.iter().filter(|(_, x)| x == c).count() >= 5);
        classes.sort_by_key(|c| Category::ALL.iter().position(|x| x == c));
        let used: Vec<&(&Vec<f32>, Category)> = labelled
            .iter()
            .filter(|(_, c)| classes.contains(c))
            .collect();
        let xs: Vec<Vec<f32>> = used.iter().map(|(x, _)| (*x).clone()).collect();
        let ys: Vec<usize> = used
            .iter()
            .map(|(_, c)| classes.iter().position(|x| x == c).unwrap())
            .collect();
        println!(
            "{name}: {} files, classes {:?}",
            xs.len(),
            classes.iter().map(|c| c.name()).collect::<Vec<_>>()
        );
        // Five nets of 16 hidden units: as well as any size tried on the held-out pack, and
        // averaging them steadies what one alone gets right by luck.
        let hidden = 16;
        let m = fit_ensemble(&xs, &ys, classes.len(), hidden);
        let predict = |m: &Ensemble, x: &[f32]| m.predict(x);
        let right = xs
            .iter()
            .zip(&ys)
            .filter(|(x, y)| predict(&m, x) == **y)
            .count();
        println!("  training accuracy {}", pct(right, xs.len()));
        // The held-out pack, from its audio alone (its kind as named).
        let held: Vec<&Row> = rows
            .iter()
            .filter(|r| {
                r.truth.held_out
                    && r.truth.kind == Some(kind)
                    && !r.truth.cats.is_empty()
                    && r.feats.is_some()
            })
            .collect();
        let (mut ok, mut drums_ok, mut drums) = (0, 0, 0);
        let mut confusion: HashMap<Category, HashMap<Category, usize>> = HashMap::new();
        for r in &held {
            let got = classes[predict(&m, &r.feats.as_ref().unwrap().0)];
            let hit = r.truth.cats.contains(&got);
            *confusion
                .entry(r.truth.cats[0])
                .or_default()
                .entry(got)
                .or_default() += 1;
            ok += hit as usize;
            if r.truth.cats[0].is_drum() && r.truth.cats[0] != Category::Drums {
                drums += 1;
                drums_ok += hit as usize;
            }
        }
        println!(
            "  held-out 90s pack: {}, its drums {}",
            pct(ok, held.len()),
            pct(drums_ok, drums)
        );
        if std::env::var("CONFUSION").is_ok() {
            for (truth, row) in &confusion {
                let mut row: Vec<_> = row.iter().collect();
                row.sort_by(|a, b| b.1.cmp(a.1));
                let row: Vec<String> = row
                    .iter()
                    .map(|(c, n)| format!("{} {n}", c.name()))
                    .collect();
                println!("    {:<11} {}", truth.name(), row.join(", "));
            }
        }
        if std::env::var("LOPO").is_ok() {
            // Each pack predicted by a model fitted on the others.
            let packs: Vec<&String> = {
                let mut p: Vec<&String> = used
                    .iter()
                    .map(|u| {
                        &train
                            .iter()
                            .find(|r| std::ptr::eq(&r.feats.as_ref().unwrap().0, u.0))
                            .unwrap()
                            .truth
                            .pack
                    })
                    .collect();
                p.sort();
                p.dedup();
                p
            };
            let pack_of: Vec<&String> = used
                .iter()
                .map(|u| {
                    &train
                        .iter()
                        .find(|r| std::ptr::eq(&r.feats.as_ref().unwrap().0, u.0))
                        .unwrap()
                        .truth
                        .pack
                })
                .collect();
            let (mut all_right, mut all_n) = (0, 0);
            let mut confused: HashMap<(usize, usize), usize> = HashMap::new();
            for pack in packs {
                let (tx, ty): (Vec<Vec<f32>>, Vec<usize>) = xs
                    .iter()
                    .zip(&ys)
                    .zip(&pack_of)
                    .filter(|(_, p)| **p != pack)
                    .map(|((x, y), _)| (x.clone(), *y))
                    .unzip();
                let m = fit_ensemble(&tx, &ty, classes.len(), hidden);
                let held: Vec<(&Vec<f32>, &usize)> = xs
                    .iter()
                    .zip(&ys)
                    .zip(&pack_of)
                    .filter(|(_, p)| **p == pack)
                    .map(|(xy, _)| xy)
                    .collect();
                let right = held.iter().filter(|(x, y)| predict(&m, x) == **y).count();
                for (x, y) in &held {
                    *confused.entry((**y, predict(&m, x))).or_default() += 1;
                }
                all_right += right;
                all_n += held.len();
                println!("  {pack} held out: {}", pct(right, held.len()));
            }
            println!("  all packs held out in turn: {}", pct(all_right, all_n));
            for (c, class) in classes.iter().enumerate() {
                let n: usize = confused
                    .iter()
                    .filter(|((t, _), _)| *t == c)
                    .map(|(_, v)| v)
                    .sum();
                let mut row: Vec<(usize, usize)> = confused
                    .iter()
                    .filter(|((t, _), _)| *t == c)
                    .map(|((_, p), v)| (*p, *v))
                    .collect();
                row.sort_by_key(|r| std::cmp::Reverse(r.1));
                let row: Vec<String> = row
                    .iter()
                    .take(5)
                    .map(|(p, v)| format!("{} {v}", classes[*p].name()))
                    .collect();
                println!("    {:<11} {n:>4}: {}", class.name(), row.join(", "));
            }
        }
        code += &format!(
            "\n/// Categories of {}.\n",
            if kind == Kind::OneShot {
                "one-shots"
            } else {
                "loops"
            }
        );
        code += &net_code(name, &classes, &m);
        if kind == Kind::OneShot {
            one_shot_nets = Some(m);
        }
    }
    // The embedding: a linear discriminant projection of the standardised feature vector, so
    // what's near is what's alike in the ways that tell sounds apart.
    let labelled: Vec<(&Vec<f32>, Category)> = train
        .iter()
        .filter_map(|r| {
            r.truth
                .cats
                .first()
                .map(|c| (&r.feats.as_ref().unwrap().0, *c))
        })
        .collect();
    let d = labelled[0].0.len();
    let n = labelled.len() as f32;
    let mean: Vec<f32> = (0..d)
        .map(|j| labelled.iter().map(|e| e.0[j]).sum::<f32>() / n)
        .collect();
    let scale: Vec<f32> = (0..d)
        .map(|j| {
            (labelled
                .iter()
                .map(|e| (e.0[j] - mean[j]).powi(2))
                .sum::<f32>()
                / n)
                .sqrt()
                .max(1e-3)
        })
        .collect();
    let nets = one_shot_nets.as_ref().unwrap();
    // The standardised vector, and the one-shot nets' hidden layers after it.
    let standard = |x: &[f32]| -> Vec<f32> {
        let mut z: Vec<f32> = x
            .iter()
            .zip(&mean)
            .zip(&scale)
            .map(|((v, m), s)| ((v - m) / s).clamp(-6.0, 6.0))
            .collect();
        z.extend(nets.0.iter().flat_map(|n| n.forward(&n.standardise(x)).0));
        z
    };
    let mut cats: Vec<Category> = labelled.iter().map(|l| l.1).collect();
    cats.sort_by_key(|c| Category::ALL.iter().position(|x| x == c));
    cats.dedup();
    let zs: Vec<Vec<f32>> = labelled.iter().map(|l| standard(l.0)).collect();
    let ys: Vec<usize> = labelled
        .iter()
        .map(|l| cats.iter().position(|c| *c == l.1).unwrap())
        .collect();
    let projection = lda(&zs, &ys, cats.len(), 32, 0.5);
    let embed = |x: &[f32]| -> Vec<f32> {
        let z = standard(x);
        let mut e: Vec<f32> = projection
            .iter()
            .map(|p| p.iter().zip(&z).map(|(a, b)| a * b).sum())
            .collect();
        let norm = e.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-9);
        e.iter_mut().for_each(|v| *v /= norm);
        e
    };
    // How it does on the held-out pack's one-shots, against the embedding the analysis has now.
    let family = |c: Category| match c {
        Category::HatOpen => Category::HatClosed,
        Category::Ride => Category::Crash,
        c => c,
    };
    // Within each pack, how often a one-shot's 5 nearest share its category.
    let knn = |group: &[&Row], embed: &dyn Fn(&Row) -> Vec<f32>| -> (usize, usize) {
        let vectors: Vec<Vec<f32>> = group.iter().map(|r| embed(r)).collect();
        let mut shared = 0;
        let mut per: HashMap<Category, (usize, usize, HashMap<Category, usize>)> = HashMap::new();
        for (i, a) in vectors.iter().enumerate() {
            let mut near: Vec<(f32, usize)> = vectors
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .map(|(j, b)| (a.iter().zip(b).map(|(x, y)| x * y).sum(), j))
                .collect();
            near.sort_by(|x, y| y.0.total_cmp(&x.0));
            let mine = family(group[i].truth.cats[0]);
            let e = per.entry(mine).or_default();
            for (_, j) in near.iter().take(5) {
                let theirs = family(group[*j].truth.cats[0]);
                e.1 += 1;
                if theirs == mine {
                    shared += 1;
                    e.0 += 1;
                } else {
                    *e.2.entry(theirs).or_default() += 1;
                }
            }
        }
        if group[0].truth.held_out && std::env::var("KNN").is_ok() {
            for (c, (n, d, others)) in &per {
                let mut o: Vec<_> = others.iter().collect();
                o.sort_by(|a, b| b.1.cmp(a.1));
                let o: Vec<String> = o
                    .iter()
                    .take(4)
                    .map(|(c, n)| format!("{} {n}", c.name()))
                    .collect();
                println!(
                    "    {:<11} {:>5.1}% of {d}: else {}",
                    c.name(),
                    100.0 * *n as f32 / *d as f32,
                    o.join(", ")
                );
            }
        }
        (shared, group.len() * 5)
    };
    let mut packs: Vec<&String> = rows.iter().map(|r| &r.truth.pack).collect();
    packs.sort();
    packs.dedup();
    let (mut tn, mut td, mut fnn, mut fd) = (0, 0, 0, 0);
    for pack in packs {
        let group: Vec<&Row> = rows
            .iter()
            .filter(|r| {
                &r.truth.pack == pack
                    && r.truth.kind == Some(Kind::OneShot)
                    && !r.truth.cats.is_empty()
                    && r.feats.is_some()
            })
            .collect();
        if group.len() < 20 {
            continue;
        }
        let now = knn(&group, &|r: &Row| r.blind.embedding.clone());
        let fitted = knn(&group, &|r: &Row| embed(&r.feats.as_ref().unwrap().0));
        if group[0].truth.held_out {
            println!(
                "embedding, {pack} (held out): now {}, fitted {}",
                pct(now.0, now.1),
                pct(fitted.0, fitted.1)
            );
        } else {
            (tn, td, fnn, fd) = (tn + now.0, td + now.1, fnn + fitted.0, fd + fitted.1);
        }
    }
    println!(
        "embedding, the other packs each by itself: now {}, fitted {}",
        pct(tn, td),
        pct(fnn, fd)
    );
    code += &format!(
        "\n/// The embedding: the feature vector standardised (centres, scales), the one-shot nets' hidden\n/// layers after it, then projected (a linear discriminant analysis of the packs' categories).\npub static EMBED_MEAN: &[f32] = {};\npub static EMBED_SCALE: &[f32] = {};\npub static EMBED_PROJECTION: &[&[f32]] = {};\n",
        floats(&mean),
        floats(&scale),
        rows_code(&projection)
    );
    // Per category, the median and spread (interquartile range / 1.35) of the trait inputs.
    code += "\n/// Per category: the median and spread of what traits are judged by (see\n/// `analyze::trait_inputs`).\npub static TRAIT_STATS: &[(Category, [(f32, f32); 10])] = &[\n";
    let mut cats: Vec<Category> = Vec::new();
    for r in &train {
        if let Some(c) = r.truth.cats.first()
            && !cats.contains(c)
        {
            cats.push(*c);
        }
    }
    cats.sort_by_key(|c| Category::ALL.iter().position(|x| x == c));
    cats.push(Category::Other);
    for c in cats {
        let of: Vec<&[f32; 10]> = train
            .iter()
            .filter(|r| c == Category::Other || r.truth.cats.first() == Some(&c))
            .map(|r| &r.feats.as_ref().unwrap().1)
            .collect();
        if of.len() < 5 {
            continue;
        }
        let stats: Vec<String> = (0..10)
            .map(|j| {
                let mut v: Vec<f32> = of.iter().map(|x| x[j]).collect();
                v.sort_by(|a, b| a.total_cmp(b));
                let q = |p: f32| v[((v.len() - 1) as f32 * p) as usize];
                format!(
                    "({:.3}, {:.3})",
                    q(0.5),
                    ((q(0.75) - q(0.25)) / 1.35).max(0.05)
                )
            })
            .collect();
        code += &format!(
            "    (\n        Category::{c:?},\n        [{}],\n    ),\n",
            stats.join(", ")
        );
    }
    code += "];\n";
    write_model(out, &code);
    println!("wrote {}", out.display());
}

/// Writes the fitted model to the folder `dir`: what a net is in `mod.rs`, and each table (its doc
/// comment and the statics under it) in a file of its own, so no one file holds them all.
pub(crate) fn write_model(dir: &Path, code: &str) {
    let lines: Vec<&str> = code.lines().collect();
    // Each table starts at the doc comment over its first static.
    let mut cuts = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        if l.starts_with("pub static ") && i > 0 && lines[i - 1].starts_with("///") {
            let mut s = i;
            while s > 0 && lines[s - 1].starts_with("///") {
                s -= 1;
            }
            cuts.push(s);
        }
    }
    std::fs::create_dir_all(dir).unwrap();
    let mut files = Vec::new();
    for (k, &s) in cuts.iter().enumerate() {
        let e = cuts.get(k + 1).copied().unwrap_or(lines.len());
        let first = lines[s..e]
            .iter()
            .find(|l| l.starts_with("pub static "))
            .unwrap();
        let name = first["pub static ".len()..]
            .split(':')
            .next()
            .unwrap()
            .to_lowercase();
        let file = match name.as_str() {
            "loop" => "loops".to_string(),
            "trait_stats" => "traits".to_string(),
            n if n.starts_with("embed") => "embed".to_string(),
            n => n.to_string(),
        };
        let body = lines[s..e].join("\n");
        // What a net is, for the tables of nets.
        let uses = match body.contains("Net") || body.contains("Category") {
            true => "use super::*;\n\n",
            false => "",
        };
        std::fs::write(
            dir.join(format!("{file}.rs")),
            format!(
                "// Fitted numbers, some near a constant by chance.\n#![allow(clippy::approx_constant)]\n\n{uses}{}\n",
                body.trim_end()
            ),
        )
        .unwrap();
        files.push(file);
    }
    let decls: String = files.iter().map(|f| format!("mod {f};\n")).collect();
    let uses: String = files.iter().map(|f| format!("pub use {f}::*;\n")).collect();
    let head = lines[..cuts[0]].join("\n");
    std::fs::write(
        dir.join("mod.rs"),
        format!("{}\n\n{decls}\n{uses}", head.trim_end()),
    )
    .unwrap();
}
