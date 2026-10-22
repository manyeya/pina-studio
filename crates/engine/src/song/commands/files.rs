//! Audio and samples brought into the song's folder.

use super::*;

/// Brings a WAV file into the project: one already under `dir` is used where it is, one elsewhere
/// is copied into `Audio Files`. Returns its path relative to `dir`.
pub(crate) fn import(dir: &Path, file: &Path) -> Result<PathBuf, String> {
    let shown = file.display();
    if file.is_relative() && dir.join(file).exists() {
        return Ok(file.to_path_buf());
    }
    let name = file
        .file_name()
        .ok_or_else(|| format!("{shown}: not a file"))?;
    std::fs::create_dir_all(dir.join(AUDIO_DIR)).map_err(|e| e.to_string())?;
    let bytes = std::fs::read(file).map_err(|e| format!("{shown}: {e}"))?;
    // The same name from elsewhere, with other audio, gets a number: Kick.wav, Kick 2.wav.
    let stem = Path::new(name)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();
    let ext = Path::new(name)
        .extension()
        .unwrap_or_default()
        .to_string_lossy();
    for n in 1.. {
        let to = match n {
            1 => PathBuf::from(AUDIO_DIR).join(name),
            n => PathBuf::from(AUDIO_DIR).join(format!("{stem} {n}.{ext}")),
        };
        match std::fs::read(dir.join(&to)) {
            Ok(there) if there == bytes => return Ok(to),
            Ok(_) => continue,
            Err(_) => {
                std::fs::write(dir.join(&to), &bytes).map_err(|e| format!("{shown}: {e}"))?;
                return Ok(to);
            }
        }
    }
    unreachable!()
}

/// Brings a sampler's samples into the project, as `import` does audio.
pub(crate) fn import_instrument(dir: &Path, instrument: &mut Instrument) -> Result<(), String> {
    if let Instrument::Sampler(s) = instrument {
        for z in &mut s.zones {
            z.file = import(dir, &z.file)?;
        }
    }
    Ok(())
}
