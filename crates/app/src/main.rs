mod cli;
mod mcp;
mod menu;
mod pianoroll;
mod record;
mod server;
mod strip;
#[cfg(test)]
mod tests;
mod theme;
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
use workspace::*;

const USAGE: &str = "usage:
  tempo [project]                  open a project folder (default ~/Music/tempo/Untitled)
  tempo describe                   the commands an agent can send, with examples
  tempo show <project>             the project, as JSON
  tempo do <project> <command>     apply a command (JSON); live in the app if it's open there
  tempo analyze <project|file.wav> loudness, peaks and balance, of the mix and each track
  tempo render <project> <out>     bounce to a WAV file
  tempo play <project>             play it (in the app if it's open, else through the output)
  tempo stop|save|undo|redo <project>, tempo locate <project> <tick>
                                   drive the app that has the project open
  tempo plugins                    the VST3 plugins installed
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
        ["plugins"] => print(cli::plugins()),
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
        bind_keys(cx);
        cx.set_menus([
            Menu::new("tempo").items([MenuItem::action("Quit tempo", Quit)]),
            Menu::new("File").items([MenuItem::action("Save", Save)]),
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
            ]),
            Menu::new("View").items([
                MenuItem::action("Mixer", ToggleMixer),
                MenuItem::action("Piano Roll", TogglePianoRoll),
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
            ]),
            Menu::new("Window").items([MenuItem::action("Musical Typing", ToggleTyping)]),
        ]);
        let bounds = Bounds::centered(None, size(px(1400.), px(860.)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
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
