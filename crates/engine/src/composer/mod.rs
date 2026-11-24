//! Composer models: MIDI written by a model trained on music itself (MIDI-GPT and its kind), the
//! bars asked for written with the rest of the song heard. Pina Studio ships none: the person
//! downloads one from where its makers host it (`catalog`), on its makers' terms. Each runs as a
//! local server of its own (`server`), in an isolated Python that uv installs; a song goes to it as
//! a score, and what it writes comes back as notes (`score`).

pub mod catalog;
pub mod score;
pub mod server;
#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

/// Where composer models, their Python and its packages are kept (`PINA_COMPOSERS` moves it).
pub fn folder() -> PathBuf {
    match std::env::var_os("PINA_COMPOSERS") {
        Some(d) => d.into(),
        None => crate::paths::support().join("Composers"),
    }
}

/// A model that's been downloaded: what it is, and its file.
#[derive(Clone, Debug)]
pub struct Installed {
    pub known: &'static catalog::Known,
    pub file: PathBuf,
}

/// The known models downloaded into `dir`.
pub fn installed(dir: &Path) -> Vec<Installed> {
    catalog::KNOWN
        .iter()
        .filter_map(|k| {
            let file = k.file_in(dir);
            file.is_file().then_some(Installed { known: k, file })
        })
        .collect()
}

/// What a model said it can do (`server::info`), as kept from the last time it ran: its controls
/// show before it's started again.
pub fn saved_info(k: &catalog::Known, dir: &Path) -> Option<serde_json::Value> {
    serde_json::from_str(&std::fs::read_to_string(dir.join(k.id).join("info.json")).ok()?).ok()
}

/// `info`, kept for `saved_info`.
pub fn save_info(k: &catalog::Known, dir: &Path, info: &serde_json::Value) {
    let _ = std::fs::write(dir.join(k.id).join("info.json"), info.to_string());
}

/// A downloaded model taken away (its family's Python stays, for the others).
pub fn remove(k: &catalog::Known, dir: &Path) -> Result<(), String> {
    let d = dir.join(k.id);
    match std::fs::remove_dir_all(&d) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("{}: {e}", d.display())),
    }
}

/// The person's Hugging Face token, for models whose makers ask them to sign in: HF_TOKEN, or
/// the one `hf auth login` saved.
pub fn hf_token() -> Option<String> {
    let t = std::env::var("HF_TOKEN")
        .ok()
        .or_else(|| std::fs::read_to_string(hf_token_file()).ok())?;
    let t = t.trim().to_string();
    (!t.is_empty()).then_some(t)
}

/// Where `hf auth login` keeps the token (HF_HOME moves it), so every Hugging Face tool finds
/// the one saved here too.
fn hf_token_file() -> PathBuf {
    std::env::var_os("HF_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::paths::home().join(".cache").join("huggingface"))
        .join("token")
}

/// `token` saved where `hf auth login` saves it, readable only by this user.
pub fn save_hf_token(token: &str) -> Result<(), String> {
    let f = hf_token_file();
    if let Some(d) = f.parent() {
        std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    std::fs::write(&f, token.trim()).map_err(|e| format!("{}: {e}", f.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}
