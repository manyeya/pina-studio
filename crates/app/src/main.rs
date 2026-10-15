mod strip;
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
use tempo_engine::{engine::Engine, player::Player, project::Project, session::Session};
use workspace::*;

const USAGE: &str = "usage:
  tempo [project]                 open a project folder (default ~/Music/tempo/Untitled)
  tempo render <project> <out>    bounce a project folder to a WAV file
  tempo play <project>            play a project folder through the default output";

/// Seconds after the last region, for releases and effect tails.
const TAIL: f64 = 2.0;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match args.as_slice() {
        ["render", dir, out] => render(Path::new(dir), Path::new(out)),
        ["play", dir] => play(Path::new(dir)),
        ["-h" | "--help" | "help"] => {
            println!("{USAGE}");
            Ok(())
        }
        [] => open(&default_project()),
        [dir] if !dir.starts_with('-') => open(Path::new(dir)),
        _ => Err(USAGE.to_string()),
    };
    if let Err(e) = result {
        eprintln!("{e}");
        exit(1);
    }
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
                MenuItem::action("Inspector", ToggleInspector),
                MenuItem::separator(),
                MenuItem::action("Zoom In", ZoomIn),
                MenuItem::action("Zoom Out", ZoomOut),
            ]),
            Menu::new("Transport").items([
                MenuItem::action("Play or Stop", PlayStop),
                MenuItem::action("Go to Beginning", GoToStart),
            ]),
        ]);
        let bounds = Bounds::centered(None, size(px(1400.), px(860.)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            ..Default::default()
        };
        cx.open_window(options, |window, cx| {
            cx.new(|cx| Workspace::new(session, player, window, cx))
        })
        .unwrap();
        cx.on_window_closed(|cx, _| cx.quit()).detach();
        cx.activate(true);
    });
    Ok(())
}

/// Loads a project and its engine, and how long it plays for, in samples.
fn load(dir: &Path) -> Result<(Project, Engine, Vec<tempo_engine::plugin::Plugin>, u64), String> {
    let project = Project::load(dir)?;
    let rate = project.sample_rate as f64;
    let (engine, plugins) = Engine::load(&project, dir, rate)?;
    let frames = project.samples(project.end(), rate) + (TAIL * rate) as u64;
    Ok((project, engine, plugins, frames))
}

fn render(dir: &Path, out: &Path) -> Result<(), String> {
    let (project, mut engine, _plugins, frames) = load(dir)?;
    engine.bounce(out, frames)?;
    println!(
        "{}: {:.1} s at {} Hz",
        out.display(),
        frames as f64 / engine.rate(),
        project.sample_rate
    );
    Ok(())
}

fn play(dir: &Path) -> Result<(), String> {
    let (_project, mut engine, _plugins, frames) = load(dir)?;
    let seconds = frames as f64 / engine.rate();
    engine.play();
    let _stream = engine.start_output()?;
    println!("playing {seconds:.1} s (ctrl-c stops)");
    thread::sleep(Duration::from_secs_f64(seconds));
    Ok(())
}
