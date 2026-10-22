//! The master's tonal balance against the genre's.

use super::*;

/// The master's EQ set to bring the mix's tonal balance halfway to the reference's (else the
/// genre's): a broad move in each of the six bands, sub to air, at most 2.5 dB, none under
/// half a dB. What it measured and did; nothing when there's nothing to aim at.
pub fn tonal_balance(dir: &Path) -> Result<Value, String> {
    let p = cli::project(dir)?;
    let target = p
        .reference
        .as_ref()
        .map(|r| (r.spectrum, "the reference"))
        .or_else(|| {
            let g = p.genre.as_deref()?;
            pina_engine::music::playbook::find(g).map(|b| (b.spectrum, "the genre"))
        });
    let Some((target, aim)) = target else {
        return Ok(Value::Null);
    };
    let mut unmastered = p.clone();
    unmastered.master_inserts.clear();
    (
        unmastered.master_eq,
        unmastered.master_comp,
        unmastered.master_sat,
    ) = (None, None, None);
    unmastered.master_volume_db = 0.0;
    let s = cli::report(&unmastered, dir, None, false)?.mix.spectrum_db;
    let measured = [s.sub, s.bass, s.low_mid, s.mid, s.presence, s.air];
    const NAMES: [&str; 6] = ["sub", "bass", "low mid", "mid", "presence", "air"];
    let mut bands = Vec::new();
    let mut said = Vec::new();
    for (i, (want, got)) in target.iter().zip(measured).enumerate() {
        let gain = ((want - got) * 0.5).clamp(-2.5, 2.5);
        if gain.abs() < 0.5 {
            continue;
        }
        let gain = (gain as f64 * 10.0).round() / 10.0;
        bands.push(match i {
            0 => json!({ "shape": "low_shelf", "freq": 60, "gain_db": gain }),
            1 => json!({ "shape": "bell", "freq": 120, "gain_db": gain, "q": 0.9 }),
            2 => json!({ "shape": "bell", "freq": 500, "gain_db": gain, "q": 0.8 }),
            3 => json!({ "shape": "bell", "freq": 2500, "gain_db": gain, "q": 0.8 }),
            4 => json!({ "shape": "bell", "freq": 6000, "gain_db": gain, "q": 0.9 }),
            _ => json!({ "shape": "high_shelf", "freq": 10000, "gain_db": gain }),
        });
        said.push(
            json!({ "band": NAMES[i], "measured_db": (got as f64 * 10.0).round() / 10.0,
            "target_db": *want, "eq_db": gain }),
        );
    }
    let eq = match bands.is_empty() {
        true => Value::Null,
        false => json!({ "bands": bands }),
    };
    run(
        dir,
        json!({ "op": "set_eq", "channel": "master", "eq": eq }),
    )?;
    Ok(json!({ "toward": aim, "moves": said }))
}
