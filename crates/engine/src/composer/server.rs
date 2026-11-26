//! A composer model's server on this computer: its family's Python (made by uv, in the composers'
//! folder, apart from anything else on the computer), the model served from it, and talked to
//! the way its HTTP API asks, by curl (the system's, on macOS and Windows alike). A server stays
//! up between asks (another ask, from the app or the command line, finds it running) and shuts
//! itself down after ten idle minutes.

use std::{
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

use serde_json::{Value, json};

use super::{Installed, catalog::Family};

/// uv, at the version run with Pina Studio, for when the computer has none.
const UV_VERSION: &str = "0.13.9";

/// How long a server waits, idle, before it shuts itself down.
const IDLE_SECONDS: u32 = 600;

/// A model being served, and where.
#[derive(Clone, Debug, PartialEq)]
pub struct Running {
    pub url: String,
    pub model: String,
}

/// How far a model has got: the notes written so far.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Progress {
    pub tokens: u64,
    pub notes: usize,
}

/// A note as it's written, before the take is done: its part (the request's track), its bar,
/// and where in it, in the ticks the model's answers come in (their score's resolution).
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
pub struct Written {
    pub track: usize,
    pub bar: u64,
    pub pitch: u8,
    pub onset_ticks: i64,
    pub duration_ticks: i64,
    pub velocity: u8,
}

/// uv: the computer's own, or Pina Studio's (downloaded into `dir` once).
pub fn uv(dir: &Path, say: &mut dyn FnMut(&str), stop: &AtomicBool) -> Result<PathBuf, String> {
    if Command::new("uv")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
    {
        return Ok("uv".into());
    }
    let ours = dir.join("uv").join(exe_name("uv"));
    if ours.is_file() {
        return Ok(ours);
    }
    let (asset, bytes) = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => ("uv-aarch64-apple-darwin.tar.gz", 14_554_974),
        ("macos", "x86_64") => ("uv-x86_64-apple-darwin.tar.gz", 17_362_924),
        ("windows", "x86_64") => ("uv-x86_64-pc-windows-msvc.zip", 15_567_875),
        _ => return Err("install uv (https://docs.astral.sh/uv) and try again".into()),
    };
    say("Downloading uv");
    let home = dir.join("uv");
    std::fs::create_dir_all(&home).map_err(|e| format!("{}: {e}", home.display()))?;
    let archive = home.join(asset);
    let url = format!("https://github.com/astral-sh/uv/releases/download/{UV_VERSION}/{asset}");
    crate::separate::catalog::fetch(&url, bytes, &archive, |_| {}, stop)?;
    // tar opens both (bsdtar is Windows' own tar, and reads zips).
    let out = home.join("x");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let ok = Command::new("tar")
        .arg("-xf")
        .arg(&archive)
        .arg("-C")
        .arg(&out)
        .status()
        .is_ok_and(|s| s.success());
    let found = ok.then(|| find(&out, &exe_name("uv"))).flatten();
    let _ = std::fs::remove_file(&archive);
    let found = found.ok_or("uv's download had no uv in it")?;
    std::fs::rename(&found, &ours).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(&out);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&ours, std::fs::Permissions::from_mode(0o755));
    }
    Ok(ours)
}

/// `name` in `dir` or a folder in it.
fn find(dir: &Path, name: &str) -> Option<PathBuf> {
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let p = e.path();
        if p.is_dir() {
            if let Some(f) = find(&p, name) {
                return Some(f);
            }
        } else if p.file_name().is_some_and(|n| n == name) {
            return Some(p);
        }
    }
    None
}

fn exe_name(name: &str) -> String {
    match cfg!(windows) {
        true => format!("{name}.exe"),
        false => name.to_string(),
    }
}

/// `family`'s Python in `dir`.
fn env(family: Family, dir: &Path) -> PathBuf {
    dir.join(family.id()).join("env")
}

/// A program installed in a Python `env`.
fn tool(env: &Path, name: &str) -> PathBuf {
    match cfg!(windows) {
        true => env.join("Scripts").join(exe_name(name)),
        false => env.join("bin").join(name),
    }
}

/// Whether `family`'s Python is made and its package, at the pinned version, installed.
pub fn ready(family: Family, dir: &Path) -> bool {
    let env = env(family, dir);
    tool(&env, family.server()).is_file()
        && std::fs::read_to_string(env.join("pina-package.txt"))
            .is_ok_and(|p| p.trim() == family.package())
}

/// `family`'s Python made and its package installed, `say` told what's happening (uv's own
/// lines, as it downloads and installs). Its Python, uv's downloads and cache all stay in `dir`.
pub fn install(
    family: Family,
    dir: &Path,
    say: &mut dyn FnMut(&str),
    stop: &AtomicBool,
) -> Result<(), String> {
    let uv = uv(dir, say, stop)?;
    let env = env(family, dir);
    let uv_cmd = || {
        let mut c = Command::new(&uv);
        c.env("UV_PYTHON_INSTALL_DIR", dir.join("python"))
            .env("UV_CACHE_DIR", dir.join("cache"));
        c
    };
    if !tool(&env, "python").is_file() {
        say("Getting Python 3.12");
        let mut c = uv_cmd();
        // uv's own Python, always: it needs none on the computer, and one there being changed or
        // taken away later can't break the model's.
        c.args([
            "venv",
            "--python",
            "3.12",
            "--python-preference",
            "only-managed",
        ])
        .arg(&env);
        run(c, say, stop)?;
    }
    say(&format!("Installing {}", family.package()));
    let mut c = uv_cmd();
    c.args(["pip", "install", "--python"])
        .arg(tool(&env, "python"))
        .arg(family.package());
    run(c, say, stop)?;
    // What uv downloaded is in the Python now: its cache would only take the space twice.
    let _ = std::fs::remove_dir_all(dir.join("cache"));
    std::fs::write(env.join("pina-package.txt"), family.package()).map_err(|e| e.to_string())
}

/// `c` run to its end, each line it writes told to `say`; stopped (and its error said) when
/// asked to.
fn run(mut c: Command, say: &mut dyn FnMut(&str), stop: &AtomicBool) -> Result<(), String> {
    let mut child = c
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Couldn't start {:?}: {e}", c.get_program()))?;
    let (tx, rx) = mpsc::channel::<String>();
    for out in [
        child
            .stdout
            .take()
            .map(|o| Box::new(o) as Box<dyn std::io::Read + Send>),
        child
            .stderr
            .take()
            .map(|e| Box::new(e) as Box<dyn std::io::Read + Send>),
    ]
    .into_iter()
    .flatten()
    {
        let tx = tx.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                let _ = tx.send(line);
            }
        });
    }
    drop(tx);
    let mut last = Vec::new();
    loop {
        if stop.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Stopped".into());
        }
        while let Ok(line) = rx.try_recv() {
            let line = line.trim().to_string();
            if !line.is_empty() {
                say(&line);
                last.push(line);
                if last.len() > 8 {
                    last.remove(0);
                }
            }
        }
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            while let Ok(line) = rx.try_recv() {
                last.push(line);
            }
            return match status.success() {
                true => Ok(()),
                false => Err(last.join("\n")),
            };
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Where the running server of `family` is noted: its address, model and process.
fn noted(family: Family, dir: &Path) -> PathBuf {
    dir.join(family.id()).join("server.json")
}

/// The server of `family` last started, as noted: the model it serves, where, and on what
/// (it may since have stopped itself: `healthy` says).
pub fn noted_server(family: Family, dir: &Path) -> Option<(String, String, String)> {
    let w: Value = serde_json::from_str(&std::fs::read_to_string(noted(family, dir)).ok()?).ok()?;
    Some((
        w["model"].as_str()?.to_string(),
        w["url"].as_str()?.to_string(),
        w["device"].as_str().unwrap_or("auto").to_string(),
    ))
}

/// The model served on `device` ("auto": the GPU where there is one, else the CPU; or "cpu"): the
/// server already running it, if it still answers; else one started on a free port (a running
/// one serving another model, or on another device, is stopped first), `say` told while it loads.
pub fn start(
    m: &Installed,
    dir: &Path,
    device: &str,
    say: &mut dyn FnMut(&str),
    stop: &AtomicBool,
) -> Result<Running, String> {
    let family = m.known.family;
    if !ready(family, dir) {
        return Err(format!("{} isn't installed yet", family.name()));
    }
    let note = noted(family, dir);
    let was: Option<Value> = std::fs::read_to_string(&note)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok());
    if let Some(w) = &was {
        let url = w["url"].as_str().unwrap_or_default();
        if w["model"] == m.known.id
            && w["device"].as_str().unwrap_or("auto") == device
            && healthy(url)
        {
            return Ok(Running {
                url: url.to_string(),
                model: m.known.id.to_string(),
            });
        }
        if let Some(pid) = w["pid"].as_u64() {
            end(pid as u32);
        }
    }
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map_err(|e| e.to_string())?
        .port();
    let log_path = dir.join(family.id()).join("server.log");
    let log = std::fs::File::create(&log_path).map_err(|e| e.to_string())?;
    let mut c = Command::new(tool(&env(family, dir), family.server()));
    c.arg("--ckpt")
        .arg(&m.file)
        .args(["--device", device, "--host", "127.0.0.1", "--port"])
        .arg(port.to_string())
        .args(["--idle-timeout", &IDLE_SECONDS.to_string()])
        .env("PYTHONUNBUFFERED", "1")
        .stdin(Stdio::null())
        .stdout(log.try_clone().map_err(|e| e.to_string())?)
        .stderr(log);
    detach(&mut c);
    say(&format!("Starting {}", m.known.name));
    let mut child: Child = c
        .spawn()
        .map_err(|e| format!("Couldn't start {}: {e}", family.server()))?;
    let url = format!("http://127.0.0.1:{port}");
    let began = Instant::now();
    loop {
        if stop.load(Ordering::Relaxed) {
            let _ = child.kill();
            return Err("Stopped".into());
        }
        if healthy(&url) {
            break;
        }
        if let Ok(Some(_)) = child.try_wait() {
            let log = std::fs::read_to_string(&log_path).unwrap_or_default();
            let tail: Vec<&str> = log.lines().rev().take(6).collect();
            return Err(format!(
                "{} stopped while starting:\n{}",
                family.server(),
                tail.into_iter().rev().collect::<Vec<_>>().join("\n")
            ));
        }
        if began.elapsed() > Duration::from_secs(300) {
            let _ = child.kill();
            return Err(format!("{} didn't start in five minutes", m.known.name));
        }
        std::thread::sleep(Duration::from_millis(400));
    }
    let _ = std::fs::write(
        &note,
        json!({ "url": url, "model": m.known.id, "pid": child.id(), "device": device }).to_string(),
    );
    // Reaped when it stops itself, so it doesn't linger as a zombie while the app runs on.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(Running {
        url,
        model: m.known.id.to_string(),
    })
}

/// The server of `family`, if one's running, stopped.
pub fn stop_server(family: Family, dir: &Path) {
    let note = noted(family, dir);
    if let Some(pid) = std::fs::read_to_string(&note)
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .and_then(|w| w["pid"].as_u64())
    {
        end(pid as u32);
    }
    let _ = std::fs::remove_file(note);
}

/// The process `pid` ended.
fn end(pid: u32) {
    // A server noted long ago may be gone, its number since given to something else: only a
    // model's server is ended.
    if !serves(pid) {
        return;
    }
    let _ = match cfg!(windows) {
        true => Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status(),
        false => Command::new("kill")
            .arg(pid.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status(),
    };
}

/// Whether process `pid` is a composer model's server (its Python, running its package).
fn serves(pid: u32) -> bool {
    let out = match cfg!(windows) {
        true => Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
            .output(),
        false => Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "command="])
            .output(),
    };
    out.ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_lowercase())
        .is_some_and(|c| c.contains("midigpt") || (cfg!(windows) && c.contains("python")))
}

/// The server runs on its own, past this process (the app quitting, or the command that started
/// it ending), till its idle timeout.
fn detach(c: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        c.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // A process group of its own, and no console window.
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
    }
}

/// curl, quiet, its errors said.
fn curl() -> Command {
    let mut c = Command::new("curl");
    c.args(["-sS", "-A", "PinaStudio"]);
    c
}

/// Whether a server answers at `url`.
pub fn healthy(url: &str) -> bool {
    !url.is_empty()
        && curl()
            .args(["--max-time", "2"])
            .arg(format!("{url}/health"))
            .stderr(Stdio::null())
            .output()
            .ok()
            .and_then(|o| serde_json::from_slice::<Value>(&o.stdout).ok())
            .is_some_and(|v| v["status"] == "ok")
}

/// What the model at `url` can do: its controls and their levels, the bars it hears at once.
pub fn info(url: &str) -> Result<Value, String> {
    let out = curl()
        .args(["--max-time", "15"])
        .arg(format!("{url}/info"))
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    serde_json::from_slice(&out.stdout).map_err(|e| format!("its info: {e}"))
}

/// `body` asked of the model at `url`, `progress` told as it writes (one take is streamed, its
/// notes as they come); `stop` ends it, and the server stops writing when the connection goes.
pub fn generate(
    url: &str,
    body: &Value,
    progress: &mut dyn FnMut(Progress, Vec<Written>),
    stop: &AtomicBool,
) -> Result<Value, String> {
    let mut child = curl()
        .args(["-N", "-X", "POST", "-H", "Content-Type: application/json"])
        .args(["--data-binary", "@-"])
        .arg(format!("{url}/generate"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Couldn't start curl: {e}"))?;
    if let Some(mut input) = child.stdin.take() {
        input
            .write_all(body.to_string().as_bytes())
            .map_err(|e| e.to_string())?;
    }
    let (tx, rx) = mpsc::channel::<String>();
    let out = child.stdout.take().ok_or("no answer")?;
    std::thread::spawn(move || {
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                return;
            }
        }
    });
    let mut whole = String::new();
    let mut seen = Progress::default();
    loop {
        if stop.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Stopped".into());
        }
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(line) => {
                let Some(data) = line.strip_prefix("data:") else {
                    whole.push_str(&line);
                    continue;
                };
                let event: Value =
                    serde_json::from_str(data.trim()).map_err(|e| format!("its answer: {e}"))?;
                match event["type"].as_str() {
                    Some("done") => {
                        let _ = child.wait();
                        return Ok(event["response"].clone());
                    }
                    Some("error") => {
                        let _ = child.wait();
                        return Err(event["message"]
                            .as_str()
                            .or(event["error"].as_str())
                            .unwrap_or("the model failed")
                            .to_string());
                    }
                    Some("cancelled") => {
                        let _ = child.wait();
                        return Err("Stopped".into());
                    }
                    kind => {
                        let new: Vec<Written> = match kind {
                            Some("notes") => {
                                serde_json::from_value(event["notes"].clone()).unwrap_or_default()
                            }
                            _ => Vec::new(),
                        };
                        seen.notes += new.len();
                        if let Some(t) = event["progress"]["generated_tokens"].as_u64() {
                            seen.tokens = t;
                        }
                        progress(seen, new);
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            // The whole answer's in: not streamed (several takes), or the server said why not.
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let status = child.wait().map_err(|e| e.to_string())?;
                let mut why = String::new();
                if let Some(mut e) = child.stderr.take() {
                    let _ = std::io::Read::read_to_string(&mut e, &mut why);
                }
                let answer: Value = serde_json::from_str(&whole).map_err(|_| {
                    match why.trim().is_empty() && status.success() {
                        true => "the model sent nothing back".to_string(),
                        false => format!("Couldn't reach the model: {}", why.trim()),
                    }
                })?;
                return match answer.get("detail") {
                    Some(d) => Err(d.as_str().map_or(d.to_string(), str::to_string)),
                    None => Ok(answer),
                };
            }
        }
    }
}
