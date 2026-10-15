mod cli;
mod editor;
mod inspector;
mod liveloops;
mod mcp;
mod menu;
mod mixer;
mod pianoroll;
mod record;
mod score;
mod server;
mod steps;
mod strip;
mod takes;
#[cfg(test)]
mod tests;
mod text;
mod theme;
mod topbar;
mod tracks;
mod workspace;

use std::{
    path::{Path, PathBuf},
    process::exit,
    thread,
    time::Duration,
};

use gpui::{App, AppContext, Bounds, Menu, MenuItem, WindowBounds, WindowOptions, px, size};
use serde_json::Value;
use server::Request;
use tempo_engine::{engine::Engine, player::Player, project::Project, session::Session};
use topbar::MIN_W;
use workspace::*;

const USAGE: &str = "usage:
  tempo [project]                  open a project folder (default ~/Music/tempo/Untitled)
  tempo describe                   the commands an agent can send, with examples
  tempo show <project>             the project, as JSON
  tempo do <project> <command>     apply a command (JSON); live in the app if it's open there
  tempo analyze <project|file.wav> loudness, peaks and balance, of the mix and each track
  tempo render <project> <out>     bounce to a WAV file
  tempo play <project>             play it (in the app if it's open, else through the output)
  tempo stop|save|undo|redo <project>, tempo locate <project> <tick>,
  tempo launch <project> <track> [scene]
                                   drive the app that has the project open
  tempo plugins                    the VST3 plugins installed
  tempo params <project> <track> [--insert N]
  tempo params <project> --aux N|--master [--insert N]
                                   a plugin's parameters, to automate or set
  tempo mcp                        an MCP server on stdin and stdout";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let p = Path::new;
    let result = match args.as_slice() {
        ["describe"] => {
            print!("{}", tempo_engine::commands::DESCRIBE);
            Ok(())
        }
        ["show", dir] => print(cli::show(p(dir))),
        ["do", dir, command] => print(cli::edit(p(dir), command)),
        ["analyze", path] => print(cli::analyze(p(path))),
        ["render", dir, out] => print(cli::render(p(dir), p(out))),
        ["play", dir] => match server::ask(p(dir), &Request::Play) {
            Some(reply) => print(reply),
            None => play(p(dir)),
        },
        ["stop", dir] => print(cli::app(p(dir), Request::Stop)),
        ["save", dir] => print(cli::app(p(dir), Request::Save)),
        ["undo", dir] => print(cli::app(p(dir), Request::Undo)),
        ["redo", dir] => print(cli::app(p(dir), Request::Redo)),
        ["locate", dir, tick] => match tick.parse() {
            Ok(tick) => print(cli::app(p(dir), Request::Locate { tick })),
            Err(_) => Err(format!("{tick} isn't a tick")),
        },
        ["launch", dir, track, scene @ ..] if scene.len() <= 1 => {
            match (track.parse(), scene.first().map(|s| s.parse()).transpose()) {
                (Ok(track), Ok(scene)) => print(cli::app(p(dir), Request::Launch { track, scene })),
                _ => Err("tempo launch <project> <track> [scene], numbers from 0".to_string()),
            }
        }
        ["plugins"] => print(cli::plugins()),
        ["params", dir, rest @ ..] => match params_args(rest) {
            Some((channel, plugin)) => print(cli::params(p(dir), channel, plugin)),
            None => Err(USAGE.to_string()),
        },
        ["mcp"] => mcp::serve(),
        // What a plugin bundle holds, as JSON: run by the plugin scan, one bundle per process.
        ["__scan", bundle] => tempo_engine::scan::inspect(p(bundle))
            .map(|found| println!("{}", serde_json::to_string(&found).unwrap())),
        ["-h" | "--help" | "help"] => {
            println!("{USAGE}");
            Ok(())
        }
        [] => open(&default_project()),
        [dir] if !dir.starts_with('-') => open(p(dir)),
        _ => Err(USAGE.to_string()),
    };
    if let Err(e) = result {
        eprintln!("{e}");
        exit(1);
    }
}

/// `<track> [--insert N]`, `--aux N [--insert N]` or `--master --insert N`.
fn params_args(
    args: &[&str],
) -> Option<(
    tempo_engine::commands::ChannelRef,
    tempo_engine::project::Which,
)> {
    use tempo_engine::{commands::ChannelRef, project::Which};
    let (channel, rest) = match args {
        ["--aux", n, rest @ ..] => (ChannelRef::Aux(n.parse().ok()?), rest),
        ["--master", rest @ ..] => (ChannelRef::Master, rest),
        [n, rest @ ..] => (ChannelRef::Track(n.parse().ok()?), rest),
        [] => return None,
    };
    let plugin = match rest {
        [] if matches!(channel, ChannelRef::Track(_)) => Which::Instrument,
        ["--insert", n] => Which::Insert(n.parse().ok()?),
        _ => return None,
    };
    Some((channel, plugin))
}

fn print(result: Result<Value, String>) -> Result<(), String> {
    println!("{}", serde_json::to_string_pretty(&result?).unwrap());
    Ok(())
}

fn default_project() -> PathBuf {
    let home = std::env::var_os("HOME").unwrap_or_default();
    Path::new(&home).join("Music/tempo/Untitled")
}

fn open(dir: &Path) -> Result<(), String> {
    let session = Session::open(dir)?;
    let player = Player::start(&session.project, dir)?;
    gpui_platform::application().run(move |cx: &mut App| {
        if let Err(e) = cx.text_system().add_fonts(theme::fonts()) {
            eprintln!("fonts: {e}");
        }
        bind_keys(cx);
        cx.set_menus([
            Menu::new("tempo").items([MenuItem::action("Quit tempo", Quit)]),
            Menu::new("File").items([
                MenuItem::action("Save", Save),
                MenuItem::action("Export to WAV", Export),
            ]),
            Menu::new("Edit").items([
                MenuItem::action("Undo", Undo),
                MenuItem::action("Redo", Redo),
                MenuItem::separator(),
                MenuItem::action("Split at Playhead", Split),
                MenuItem::action("Delete", Delete),
            ]),
            Menu::new("Track").items([
                MenuItem::action("New Track", NewTrack),
                MenuItem::action("Delete Track", DeleteTrack),
                MenuItem::action("Create Track Stack", CreateStack),
                MenuItem::action("New Pattern Region", NewPattern),
            ]),
            Menu::new("View").items([
                MenuItem::action("Mixer", ToggleMixer),
                MenuItem::action("Piano Roll", TogglePianoRoll),
                MenuItem::action("Live Loops", ToggleLoops),
                MenuItem::action("Score", ToggleScore),
                MenuItem::action("Automation", ToggleAutomation),
                MenuItem::action("Inspector", ToggleInspector),
                MenuItem::separator(),
                MenuItem::action("Zoom In", ZoomIn),
                MenuItem::action("Zoom Out", ZoomOut),
            ]),
            Menu::new("Transport").items([
                MenuItem::action("Play or Stop", PlayStop),
                MenuItem::action("Go to Beginning", GoToStart),
                MenuItem::action("Record", Record),
                MenuItem::separator(),
                MenuItem::action("Cycle", ToggleCycle),
                MenuItem::action("Metronome", ToggleMetronome),
            ]),
            Menu::new("Window").items([MenuItem::action("Musical Typing", ToggleTyping)]),
        ]);
        let bounds = Bounds::centered(None, size(px(1440.), px(900.)), cx);
        // The control bar, tracks and an editor all fit down to this.
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(MIN_W), px(640.))),
            ..Default::default()
        };
        cx.open_window(options, |window, cx| {
            cx.new(|cx| {
                let dir = session.dir.clone();
                let mut ws = Workspace::new(session, player, window, cx);
                ws.start_scan(cx);
                server::serve(cx.entity().downgrade(), &dir, cx);
                ws
            })
        })
        .unwrap();
        cx.on_window_closed(|cx, _| cx.quit()).detach();
        cx.activate(true);
    });
    Ok(())
}

/// Plays a project through the default output, without the app.
fn play(dir: &Path) -> Result<(), String> {
    let project = Project::load(dir)?;
    let rate = project.sample_rate as f64;
    let (mut engine, _plugins) = Engine::load(&project, dir, rate)?;
    let frames = project.samples(project.end(), rate) + (cli::TAIL * rate) as u64;
    let seconds = frames as f64 / rate;
    engine.play();
    let _stream = engine.start_output()?;
    println!("playing {seconds:.1} s (ctrl-c stops)");
    thread::sleep(Duration::from_secs_f64(seconds));
    Ok(())
}
