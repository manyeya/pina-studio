mod arrange;

use std::{path::Path, process::exit, thread, time::Duration};

use gpui::{App, AppContext, Bounds, WindowBounds, WindowOptions, px, size};
use tempo_engine::{engine::Engine, project::Project};

const USAGE: &str = "usage:
  tempo                           open the app
  tempo render <project> <out>    bounce a project folder to a WAV file
  tempo play <project>            play a project folder through the default output";

/// Seconds after the last region, for releases and effect tails.
const TAIL: f64 = 2.0;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match args.as_slice() {
        [] => {
            app();
            Ok(())
        }
        ["render", dir, out] => render(Path::new(dir), Path::new(out)),
        ["play", dir] => play(Path::new(dir)),
        ["-h" | "--help" | "help"] => {
            println!("{USAGE}");
            Ok(())
        }
        _ => Err(USAGE.to_string()),
    };
    if let Err(e) = result {
        eprintln!("{e}");
        exit(1);
    }
}

fn app() {
    gpui_platform::application().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1200.), px(700.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(|_| arrange::Arrange::new()),
        )
        .unwrap();
        cx.activate(true);
    });
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
    println!("playing {:.1} s (ctrl-c stops)", seconds);
    thread::sleep(Duration::from_secs_f64(seconds));
    Ok(())
}
