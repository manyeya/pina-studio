//! The sound map: every sample laid out by how it sounds (t-SNE).

use super::*;

/// Every sample's place on a map of sound, -1 to 1 each way: near ones sound alike. The two
/// directions its timbre varies most in (principal components of the embeddings), spread out by
/// rank so a crowd doesn't sit on one spot.
pub fn map(samples: &[Sample]) -> Vec<[f32; 2]> {
    let n = samples.len();
    let dims = samples.first().map_or(0, |s| s.analysis.embedding.len());
    let usable = dims > 0
        && samples.iter().all(|s| {
            s.analysis.embedding.len() == dims && s.analysis.embedding.iter().any(|v| *v != 0.0)
        });
    if n < 3 || !usable {
        // Without embeddings: categories round a circle, each a little cloud.
        return samples
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let c = Category::ALL
                    .iter()
                    .position(|c| *c == s.analysis.category)
                    .unwrap_or(0);
                let angle = c as f32 / Category::ALL.len() as f32 * std::f32::consts::TAU;
                let jitter = ((i * 7919) % 1000) as f32 / 1000.0 * 0.25;
                [angle.cos() * (0.6 + jitter), angle.sin() * (0.6 + jitter)]
            })
            .collect();
    }
    let mean: Vec<f32> = (0..dims)
        .map(|d| samples.iter().map(|s| s.analysis.embedding[d]).sum::<f32>() / n as f32)
        .collect();
    let centred: Vec<Vec<f32>> = samples
        .iter()
        .map(|s| {
            s.analysis
                .embedding
                .iter()
                .zip(&mean)
                .map(|(v, m)| v - m)
                .collect()
        })
        .collect();
    // The covariance's top two eigenvectors, by power iteration (the second with the first
    // taken out).
    let cov: Vec<Vec<f32>> = (0..dims)
        .map(|i| {
            (0..dims)
                .map(|j| centred.iter().map(|v| v[i] * v[j]).sum::<f32>() / n as f32)
                .collect()
        })
        .collect();
    let power = |against: Option<&Vec<f32>>| {
        let mut v: Vec<f32> = (0..dims).map(|i| 1.0 + i as f32 * 0.01).collect();
        for _ in 0..100 {
            let mut next: Vec<f32> = (0..dims)
                .map(|i| (0..dims).map(|j| cov[i][j] * v[j]).sum())
                .collect();
            if let Some(u) = against {
                let along: f32 = next.iter().zip(u).map(|(a, b)| a * b).sum();
                next.iter_mut().zip(u).for_each(|(a, b)| *a -= along * b);
            }
            let norm = next.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-12);
            v = next.into_iter().map(|x| x / norm).collect();
        }
        v
    };
    let first = power(None);
    let second = power(Some(&first));
    let project = |u: &Vec<f32>| -> Vec<f32> {
        centred
            .iter()
            .map(|v| v.iter().zip(u).map(|(a, b)| a * b).sum())
            .collect()
    };
    let start: Vec<[f32; 2]> = project(&first)
        .into_iter()
        .zip(project(&second))
        .map(|(x, y)| [x, y])
        .collect();
    let vectors: Vec<&[f32]> = samples.iter().map(|s| &s.analysis.embedding[..]).collect();
    let placed = tsne(&vectors, &start);
    // -1 to 1, by the 98th percentile out from the middle, so a stray sound doesn't squash
    // the rest.
    let mid = |k: usize| {
        let mut v: Vec<f32> = placed.iter().map(|p| p[k]).collect();
        v.sort_by(f32::total_cmp);
        v[n / 2]
    };
    let (mx, my) = (mid(0), mid(1));
    let mut reach: Vec<f32> = placed
        .iter()
        .map(|p| (p[0] - mx).abs().max((p[1] - my).abs()))
        .collect();
    reach.sort_by(f32::total_cmp);
    let scale = reach[(n * 98 / 100).min(n - 1)].max(1e-6);
    placed
        .iter()
        .map(|p| {
            [
                ((p[0] - mx) / scale).clamp(-1.0, 1.0),
                ((p[1] - my) / scale).clamp(-1.0, 1.0),
            ]
        })
        .collect()
}

/// t-SNE (van der Maaten and Hinton, 2008), as XO and Atlas lay out their sound maps: near
/// sounds stay near, so alike sounds gather in clusters rather than smearing along a line. From
/// `start` (the first two principal components, so it comes out the same every time), with a
/// perplexity of 30, early exaggeration, and momentum. Past 1,500 sounds it lays out 1,500 of
/// them evenly picked, and puts each other sound among its five nearest.
// ponytail: exact, O(n²) a step on up to 1,500 points (a second or two, off the UI thread);
// Barnes-Hut if libraries outgrow the landmarks.
pub(crate) fn tsne(vectors: &[&[f32]], start: &[[f32; 2]]) -> Vec<[f32; 2]> {
    const LANDMARKS: usize = 1500;
    const PERPLEXITY: f32 = 30.0;
    const STEPS: usize = 750;
    let n = vectors.len();
    let dist = |a: &[f32], b: &[f32]| a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum::<f32>();
    let marks: Vec<usize> = match n > LANDMARKS {
        true => (0..LANDMARKS).map(|i| i * n / LANDMARKS).collect(),
        false => (0..n).collect(),
    };
    let m = marks.len();
    // Affinities: each point's neighbours weighted so their perplexity is 30, then made
    // symmetric.
    let d: Vec<Vec<f32>> = marks
        .iter()
        .map(|&i| {
            marks
                .iter()
                .map(|&j| dist(vectors[i], vectors[j]))
                .collect()
        })
        .collect();
    let target = PERPLEXITY.min((m - 1) as f32 / 3.0).max(2.0).ln();
    let mut p = vec![vec![0f32; m]; m];
    for i in 0..m {
        let (mut lo, mut hi, mut beta) = (0f32, f32::INFINITY, 1f32);
        for _ in 0..64 {
            let mut sum = 0.0;
            let mut weighted = 0.0;
            for j in 0..m {
                if j != i {
                    let w = (-d[i][j] * beta).exp();
                    p[i][j] = w;
                    sum += w;
                    weighted += w * d[i][j];
                }
            }
            let sum = sum.max(1e-12);
            // The distribution's entropy, against the perplexity's.
            let entropy = sum.ln() + beta * weighted / sum;
            for v in p[i].iter_mut() {
                *v /= sum;
            }
            if (entropy - target).abs() < 1e-4 {
                break;
            }
            if entropy > target {
                lo = beta;
                beta = if hi.is_finite() {
                    (beta + hi) / 2.0
                } else {
                    beta * 2.0
                };
            } else {
                hi = beta;
                beta = (beta + lo) / 2.0;
            }
        }
    }
    let mut sym = vec![vec![0f32; m]; m];
    for i in 0..m {
        for j in 0..m {
            sym[i][j] = ((p[i][j] + p[j][i]) / (2.0 * m as f32)).max(1e-12);
        }
    }
    // From the principal components, small.
    let spread = (marks.iter().map(|&i| start[i][0].powi(2)).sum::<f32>() / m as f32)
        .sqrt()
        .max(1e-9);
    let mut y: Vec<[f32; 2]> = marks
        .iter()
        .map(|&i| [start[i][0] / spread * 1e-2, start[i][1] / spread * 1e-2])
        .collect();
    let mut velocity = vec![[0f32; 2]; m];
    let rate = (m as f32 / 12.0).max(50.0);
    for step in 0..STEPS {
        let exaggeration = if step < 250 { 12.0 } else { 1.0 };
        let momentum = if step < 250 { 0.5 } else { 0.8 };
        // Q's normaliser: every pair's Student-t kernel.
        let mut z = 0.0;
        for i in 0..m {
            for j in i + 1..m {
                let dx = y[i][0] - y[j][0];
                let dy = y[i][1] - y[j][1];
                z += 2.0 / (1.0 + dx * dx + dy * dy);
            }
        }
        let z = z.max(1e-12);
        for i in 0..m {
            let mut g = [0f32; 2];
            for j in 0..m {
                if j == i {
                    continue;
                }
                let dx = y[i][0] - y[j][0];
                let dy = y[i][1] - y[j][1];
                let k = 1.0 / (1.0 + dx * dx + dy * dy);
                let pull = (exaggeration * sym[i][j] - k / z) * k;
                g[0] += 4.0 * pull * dx;
                g[1] += 4.0 * pull * dy;
            }
            for a in 0..2 {
                velocity[i][a] = momentum * velocity[i][a] - rate * g[a];
            }
        }
        for i in 0..m {
            y[i][0] += velocity[i][0];
            y[i][1] += velocity[i][1];
        }
    }
    if m == n {
        return y;
    }
    // The rest among their five nearest landmarks, the nearer counting more.
    let mut placed = vec![[0f32; 2]; n];
    for (k, &i) in marks.iter().enumerate() {
        placed[i] = y[k];
    }
    let is_mark: std::collections::HashSet<usize> = marks.iter().copied().collect();
    for i in (0..n).filter(|i| !is_mark.contains(i)) {
        let mut near: Vec<(f32, usize)> = (0..m)
            .map(|k| (dist(vectors[i], vectors[marks[k]]), k))
            .collect();
        near.select_nth_unstable_by(4.min(m - 1), |a, b| a.0.total_cmp(&b.0));
        let (mut sum, mut at) = (0.0, [0f32; 2]);
        for &(d, k) in near.iter().take(5) {
            let w = 1.0 / (d.sqrt() + 1e-3);
            sum += w;
            at[0] += w * y[k][0];
            at[1] += w * y[k][1];
        }
        placed[i] = [at[0] / sum, at[1] / sum];
    }
    placed
}
