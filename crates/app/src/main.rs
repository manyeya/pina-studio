mod agent;
mod arrange;
mod cli;
mod editors;
mod mix;
mod pages;
mod sounds;
#[cfg(test)]
mod tests;
mod ui;
mod workspace;

use std::{
    path::{Path, PathBuf},
    process::exit,
    thread,
    time::Duration,
};

use cli::server::Request;
use gpui::{App, AppContext, Bounds, Menu, MenuItem, WindowBounds, WindowOptions, px, size};
use pina_engine::{engine::Engine, player::Player, project::Project, session::Session};
use serde_json::Value;
use workspace::topbar::MIN_W;
use workspace::*;

const USAGE: &str = "usage:
  pina [project]                   open a project folder (none: the Songs page, to start or open one)
  pina describe                   the commands an agent can send, with examples
  pina show <project>             the project, as JSON
  pina do <project> <command>     apply a command (JSON); live in the app if it's open there
  pina analyze <project|audio file> [--bars 9-16]
                                   loudness, peaks and balance, of the mix and each track
  pina listen <project> [--track N] [--bars 9-16]
                                   what each track sounds like alone (the sample library's
                                   model's guess, its character, brightness, width, key)
                                   against its part, and what's wrong with it
  pina render <project> <out> [--bars 9-16]
  pina bounce <project> [--stems [--tracks 0,2] [--master] | --sections] [--formats wav,mp3,flac,aiff,aac,alac,midi]
                         [--bits 16|24|32] [--rate 48000] [--lufs -14 [--ceiling -1] | --peak -1] [--kbps 320]
                         [--no-tail] [--bars 9-16] [--out <folder>] [--name <name>]
                                   bounce to a WAV file
  pina play <project>             play it (in the app if it's open, else through the output)
  pina stop|save|undo|redo <project>, pina locate <project> <tick>,
  pina launch <project> <track> [scene]
                                   drive the app that has the project open
  pina plugins                    the plugins installed: VST3, CLAP, VST2, Audio Units
  pina params <project> <track> [--insert N]
  pina params <project> --aux N|--master [--insert N]
                                   a plugin's parameters, to automate or set
  pina presets <project> <track> [--insert N] [--save <name>]
  pina presets <project> --aux N|--master [--insert N] [--save <name>]
                                   a plugin's presets, to load with set_preset; or
                                   save its settings as one
  pina samples                    the sample library: its folders and what's in them
  pina samples scan|add <folder>|remove <folder>
                                   analyse what's new; add or take out a folder
  pina samples find <words or JSON> [--for <project>] [--limit N]
                                   samples by words, kind, key, tempo, traits, likeness
                                   (\"dusty keys loop in C minor 95 bpm\"); --for fits a song
  pina samples similar <file>     the samples most like one
  pina samples kit [<words or JSON>] [--for <project>]
                                   a kit that goes together, with alternatives
  pina samples info <file>        what a sample is
  pina samples put <file> <project> [--track N] [--at TICK]
                                   into the song, made to fit: a loop stretched to its
                                   tempo and moved into its key; a one-shot as a sampler
                                   zone in tune
  pina critique <project> [--quick] [--bars 9-16]
                                   a producer's notes on the song, worst first, each with
                                   the command that fixes it; --quick skips the render
  pina study <file> [--summary] [--playbook] [--offline]
                                   everything about a song: its tempo, key, chords, form,
                                   groove, what plays, its mix, the genres it's nearest, and
                                   (online) its lyrics, year and genre; --playbook makes a
                                   playbook from it to write songs like it
  pina playbook [genre]           what a genre is: tempo, swing, parts and their sounds,
                                   sections, progressions, mix targets
  pina playbook --edit [genre]    open the playbooks (JSON, one a genre) to edit or add to
  pina theory <key> [words]       the key's chords, and progressions for a genre or mood
  pina reference <file> [project] measure a finished song; with a project, aim its mix there
  pina sounds <role> [--genre g] [words]
                                   every instrument's presets that suit a part, and samples
  pina mix <project> [--track N] [--no-levels]
                                   EQ, compression, saturation, pan, reverb and level, by
                                   the part each track plays
  pina master <project> [--lufs -9]
                                   a limiter on the master, set to the loudness
  pina ask [--agent <id>] <project> <words>
                                   an agent does what's asked to the song, saying so as it
                                   goes (live in the app, and its Agent panel, if it's open):
                                   claude-code or codex; the last one picked in the app by
                                   default
  pina projects                   every song in ~/Music/Pina Studio (and opened lately), as JSON
  pina new <name> [--tempo N] [--key \"A minor\"] [--time 6/8] [--template beat]
                                   start a song in ~/Music/Pina Studio from a template: empty,
                                   beat, band, lofi or electronic
  pina mcp                        an MCP server on stdin and stdout";

fn main() {
    // What tempo, as it was called before, kept: moved to Pina Studio's places, once.
    pina_engine::paths::migrate();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let p = Path::new;
    let result = match args.as_slice() {
        ["describe"] => {
            print!("{}", pina_engine::commands::DESCRIBE);
            Ok(())
        }
        ["show", dir] => print(cli::show(p(dir))),
        ["do", dir, command] => print(cli::edit(p(dir), command)),
        ["analyze", path] => print(cli::analyze(p(path), None)),
        ["analyze", path, "--bars", range] => match bar_range(range) {
            Some(r) => print(cli::analyze(p(path), Some(r))),
            None => Err(USAGE.to_string()),
        },
        ["listen", dir, rest @ ..] => {
            let mut track = None;
            let mut bars = None;
            let mut ok = true;
            for pair in rest.chunks(2) {
                match pair {
                    ["--track", t] => {
                        track = t.parse().ok().or_else(|| {
                            ok = false;
                            None
                        })
                    }
                    ["--bars", r] => {
                        bars = bar_range(r).or_else(|| {
                            ok = false;
                            None
                        })
                    }
                    _ => ok = false,
                }
            }
            match ok {
                true => print(agent::listen::listen(p(dir), track, bars)),
                false => Err(USAGE.to_string()),
            }
        }
        ["bounce", dir, rest @ ..] => match bounce_args(rest) {
            Some(spec) => print(cli::bounce_spec(p(dir), &spec)),
            None => Err(USAGE.to_string()),
        },
        ["render", dir, out] => print(cli::render(p(dir), p(out), None)),
        ["render", dir, out, "--bars", range] => match bar_range(range) {
            Some(r) => print(cli::render(p(dir), p(out), Some(r))),
            None => Err(USAGE.to_string()),
        },
        ["play", dir] => match cli::server::ask(p(dir), &Request::Play) {
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
                _ => Err("pina launch <project> <track> [scene], numbers from 0".to_string()),
            }
        }
        ["plugins"] => print(cli::plugins()),
        ["params", dir, rest @ ..] => match params_args(rest) {
            Some((channel, plugin)) => print(cli::params(p(dir), channel, plugin)),
            None => Err(USAGE.to_string()),
        },
        ["presets", dir, rest @ ..] => {
            let (rest, save) = match rest {
                [rest @ .., "--save", name] => (rest, Some(*name)),
                _ => (rest, None),
            };
            match (params_args(rest), save) {
                (Some((channel, plugin)), None) => print(cli::presets(p(dir), channel, plugin)),
                (Some((channel, plugin)), Some(name)) => {
                    print(cli::save_preset(p(dir), channel, plugin, name))
                }
                (None, _) => Err(USAGE.to_string()),
            }
        }
        ["samples", rest @ ..] => print(cli::samples::command(rest)),
        ["critique", dir, rest @ ..] => {
            let quick = rest.contains(&"--quick");
            match rest.iter().position(|a| *a == "--bars") {
                Some(i) => match rest.get(i + 1).and_then(|r| bar_range(r)) {
                    Some(range) => print(agent::produce::critique(p(dir), quick, Some(range))),
                    None => Err(USAGE.to_string()),
                },
                None => print(agent::produce::critique(p(dir), quick, None)),
            }
        }
        ["study", file, rest @ ..] => {
            let online = !rest.contains(&"--offline");
            match cli::study::study(p(file), online) {
                Err(e) => Err(e),
                Ok(s) if rest.contains(&"--summary") => {
                    println!("{}", cli::study::summary(&s));
                    Ok(())
                }
                Ok(s) if rest.contains(&"--playbook") => {
                    let taken = pina_engine::playbook::ids();
                    let book = pina_engine::study::to_playbook(&s, &taken);
                    pina_engine::playbook::save(&book)
                        .map(|f| println!("{} ({}): {}", book.name, book.id, f.display()))
                }
                Ok(s) => print(Ok(cli::study::for_agent(&s))),
            }
        }
        ["playbook"] => print(agent::produce::playbook(None)),
        ["playbook", "--edit"] => agent::produce::edit_playbooks(None),
        ["playbook", "--edit", genre] => agent::produce::edit_playbooks(Some(genre)),
        // An instrument's presets, listed by loading it alone: run by `pina sounds`.
        ["__audition", plugin, part, presets] => sounds::audition::command(plugin, part, presets),
        ["__presets", plugin] => serde_json::from_str::<pina_engine::project::PluginRef>(plugin)
            .map_err(|e| e.to_string())
            .and_then(|r| {
                let dir = std::env::temp_dir();
                let (plugin, _) = pina_engine::engine::load_plugin(&r, &dir, 48_000.0)?;
                println!("{}", serde_json::to_string(&plugin.presets()).unwrap());
                Ok(())
            }),
        ["playbook", genre @ ..] => print(agent::produce::playbook(Some(&genre.join(" ")))),
        ["theory", key, words @ ..] => print(agent::produce::theory(key, &words.join(" "))),
        ["reference", file] => print(agent::produce::reference(p(file), None)),
        ["reference", file, dir] => print(agent::produce::reference(p(file), Some(p(dir)))),
        ["sounds", role, rest @ ..] => {
            let (genre, words) = match rest {
                ["--genre", g, words @ ..] => (Some(*g), words),
                words => (None, words),
            };
            print(agent::produce::sounds(
                role,
                genre,
                &words.join(" "),
                None,
                None,
            ))
        }
        ["mix", dir, rest @ ..] => {
            let levels = !rest.contains(&"--no-levels");
            match rest {
                ["--track", t, ..] => match t.parse() {
                    Ok(t) => print(agent::produce::mix_role(p(dir), Some(t), None, levels)),
                    Err(_) => Err(USAGE.to_string()),
                },
                _ => print(agent::produce::mix_role(p(dir), None, None, levels)),
            }
        }
        ["master", dir] => print(agent::produce::master(p(dir), None)),
        ["master", dir, "--lufs", lufs] => match lufs.parse() {
            Ok(l) => print(agent::produce::master(p(dir), Some(l))),
            Err(_) => Err(USAGE.to_string()),
        },
        ["mcp"] => agent::mcp::serve(),
        ["projects"] => print(Ok(serde_json::to_value(pages::projects::list(
            &pages::projects::root(),
            &pages::projects::recent(),
        ))
        .unwrap())),
        ["new", name, rest @ ..] => new_song(name, rest),
        ["ask", "--agent", id, dir, words @ ..] if !words.is_empty() => {
            match agent::Kind::from_id(id) {
                Some(kind) => agent::ask(kind, p(dir), &words.join(" ")),
                None => Err(format!(
                    "no agent called {id}: {}",
                    agent::AGENTS
                        .iter()
                        .map(|d| d.id)
                        .collect::<Vec<_>>()
                        .join(", ")
                )),
            }
        }
        ["ask", dir, words @ ..] if !words.is_empty() => {
            agent::ask(agent::chosen(), p(dir), &words.join(" "))
        }
        // What a plugin bundle holds, as JSON: run by the plugin scan, one bundle per process.
        ["__scan", bundle] => pina_engine::scan::inspect(p(bundle))
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

/// "9-16" (bars from 1, both in) or "9".
/// `pina bounce`'s flags as the spec `cli::bounce_spec` takes.
fn bounce_args(args: &[&str]) -> Option<Value> {
    let mut spec = serde_json::Map::new();
    let mut i = 0;
    while i < args.len() {
        let next = || args.get(i + 1).copied();
        let n = |v: &str| v.parse::<f64>().ok().map(Value::from);
        match args[i] {
            "--stems" => drop(spec.insert("what".into(), "stems".into())),
            "--sections" => drop(spec.insert("what".into(), "sections".into())),
            "--master" => drop(spec.insert("master".into(), true.into())),
            "--no-tail" => drop(spec.insert("tail".into(), false.into())),
            flag @ ("--format" | "--formats") => {
                let _ = flag;
                spec.insert(
                    "formats".into(),
                    next()?
                        .split(',')
                        .map(|f| Value::from(f.trim()))
                        .collect::<Vec<_>>()
                        .into(),
                );
                i += 1;
            }
            "--tracks" => {
                spec.insert(
                    "tracks".into(),
                    next()?
                        .split(',')
                        .map(|t| t.trim().parse::<u64>().ok().map(Value::from))
                        .collect::<Option<Vec<_>>>()?
                        .into(),
                );
                i += 1;
            }
            "--bars" => {
                let (a, b) = bar_range(next()?)?;
                spec.insert("from_bar".into(), a.into());
                spec.insert("to_bar".into(), b.into());
                i += 1;
            }
            key @ ("--bits" | "--rate" | "--lufs" | "--peak" | "--ceiling" | "--kbps") => {
                spec.insert(key.trim_start_matches("--").into(), n(next()?)?);
                i += 1;
            }
            key @ ("--out" | "--name") => {
                spec.insert(key.trim_start_matches("--").into(), next()?.into());
                i += 1;
            }
            _ => return None,
        }
        i += 1;
    }
    Some(Value::Object(spec))
}

fn bar_range(s: &str) -> Option<(u64, u64)> {
    match s.split_once('-') {
        Some((a, b)) => Some((a.trim().parse().ok()?, b.trim().parse().ok()?)),
        None => s.trim().parse().ok().map(|a| (a, a)),
    }
}

/// `<track> [--insert N]`, `--aux N [--insert N]` or `--master --insert N`.
fn params_args(
    args: &[&str],
) -> Option<(
    pina_engine::commands::ChannelRef,
    pina_engine::project::Which,
)> {
    use pina_engine::{commands::ChannelRef, project::Which};
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

/// With no song named: a new one, not made yet, so the window opens on the Songs page.
fn default_project() -> PathBuf {
    pages::projects::scratch()
}

fn open(dir: &Path) -> Result<(), String> {
    let session = Session::open(dir)?;
    if session.on_disk {
        pages::projects::opened(dir);
    }
    let player = Player::start(&session.project, dir)?;
    gpui_platform::application().run(move |cx: &mut App| {
        if let Err(e) = cx.text_system().add_fonts(ui::theme::fonts()) {
            eprintln!("fonts: {e}");
        }
        bind_keys(cx);
        cx.set_menus([
            Menu::new("Pina Studio").items([
                MenuItem::action("Settings…", ShowSettings),
                MenuItem::separator(),
                MenuItem::action("Quit Pina Studio", Quit),
            ]),
            Menu::new("File").items([
                MenuItem::action("New Song…", NewSong),
                MenuItem::action("Songs…", ShowSongs),
                MenuItem::action("Close Song", CloseSong),
                MenuItem::action("Genre Playbooks…", EditPlaybooks),
                MenuItem::action("Study a Song…", StudySong),
                MenuItem::separator(),
                MenuItem::action("Save", Save),
                MenuItem::action("Bounce…", Export),
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
                MenuItem::action("Sample Browser", ToggleSamples),
                MenuItem::action("Samples Page", ToggleSamplesPage),
                MenuItem::action("Agent", ToggleAgent),
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
                ws.load_library(cx);
                ws.offer_kept(window, cx);
                cli::server::serve(cx.entity().downgrade(), &dir, cx);
                ws
            })
        })
        .unwrap();
        cx.on_window_closed(|cx, _| cx.quit()).detach();
        cx.activate(true);
    });
    Ok(())
}

/// `pina new <name> [--tempo N] [--key "A minor"] [--time 6/8] [--template beat]`.
fn new_song(name: &str, rest: &[&str]) -> Result<(), String> {
    let mut tempo = 120.0;
    let mut key = None;
    let mut time = [4, 4];
    let mut template = pages::projects::Template::Empty;
    let mut it = rest.iter();
    while let Some(&flag) = it.next() {
        let value = it.next().copied().ok_or(format!("{flag} takes a value"))?;
        match flag {
            "--tempo" => {
                tempo = value
                    .parse()
                    .map_err(|_| format!("{value} isn't a tempo"))?
            }
            "--key" => {
                key = Some(
                    pina_engine::music::Key::parse(value).ok_or(format!("{value} isn't a key"))?,
                )
            }
            "--time" => {
                let (b, u) = value.split_once('/').ok_or("--time is like 4/4")?;
                time = [
                    b.parse().map_err(|_| "--time is like 4/4")?,
                    u.parse().map_err(|_| "--time is like 4/4")?,
                ];
            }
            "--template" => {
                template = pages::projects::Template::from_name(value).ok_or(format!(
                    "no template {value}: empty, beat, band, lofi, electronic"
                ))?
            }
            _ => return Err(USAGE.to_string()),
        }
    }
    let dir = pages::projects::create(&pages::projects::root(), name, tempo, key, time, template)?;
    println!("{}", dir.display());
    Ok(())
}

/// Plays a project through the default output, without the app.
fn play(dir: &Path) -> Result<(), String> {
    let project = Project::load(dir)?;
    let rate = pina_engine::engine::output_rate(project.sample_rate)? as f64;
    let (mut engine, _plugins) = Engine::load(&project, dir, rate)?;
    let frames = project.samples(project.end(), rate) + (cli::TAIL * rate) as u64;
    let seconds = frames as f64 / rate;
    engine.play();
    let _stream = engine.start_output()?;
    println!("playing {seconds:.1} s (ctrl-c stops)");
    thread::sleep(Duration::from_secs_f64(seconds));
    Ok(())
}
