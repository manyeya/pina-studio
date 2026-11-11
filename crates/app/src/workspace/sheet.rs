//! Every shortcut on one sheet (⌘/ or ?): grouped by what they work on, narrowed by typing a
//! word of what one does or its keys. The keys are read from the keymap, so the sheet says what
//! pressing does today.

use gpui::{Action, Div, KeybindingKeystroke, MouseButton, Subscription};

use super::*;
use crate::ui::text::{TextEvent, TextInput};

/// The sheet while it's open: its search field and what's typed in it.
pub struct Sheet {
    pub find: Entity<TextInput>,
    pub query: String,
    _typed: Subscription,
    _done: Subscription,
}

/// A row's keys: actions' (from the keymap), or as written (a menu's, a click's).
enum Keys {
    Do(Vec<Box<dyn Action>>),
    Say(&'static str),
}

fn one(a: impl Action) -> Keys {
    Keys::Do(vec![Box::new(a)])
}

fn two(a: impl Action, b: impl Action) -> Keys {
    Keys::Do(vec![Box::new(a), Box::new(b)])
}

type Group = (&'static str, &'static str, Vec<(&'static str, Keys)>);

/// A group as shown: its rows' keys written out.
pub(crate) type Shown = (&'static str, &'static str, Vec<(&'static str, Vec<String>)>);

/// The groups, in order: a name, when they work, and what each key does.
fn groups() -> Vec<Group> {
    vec![
        (
            "Play & record",
            "",
            vec![
                ("Play or stop", one(PlayStop)),
                ("Play from the selection", one(PlayFromSelection)),
                ("Record", one(Record)),
                ("Arm the track", one(ArmTrack)),
                ("Cycle on or off", one(ToggleCycle)),
                ("Cycle round the selection", one(CycleToSelection)),
                ("Metronome", one(ToggleMetronome)),
                ("Musical typing", one(ToggleTyping)),
                ("Count-in on or off", one(ToggleCountIn)),
                ("Throw the recording away", one(DiscardRecording)),
            ],
        ),
        (
            "Move around",
            "",
            vec![
                ("To the start", one(GoToStart)),
                ("To the end", one(GoToEnd)),
                ("A bar back · on", two(BarBack, BarForward)),
                ("Eight bars back · on", two(EightBarsBack, EightBarsForward)),
                ("Marker here", one(AddMarkerHere)),
                ("Marker before · after", two(PrevMarker, NextMarker)),
                ("Marker round the selection", one(MarkerForSelection)),
                ("Rename the marker", one(RenameMarker)),
                ("Delete the marker", one(DeleteMarker)),
                (
                    "To the cycle's start · end",
                    two(GoToLeftLocator, GoToRightLocator),
                ),
                (
                    "To the selection's start · end",
                    two(GoToSelectionStart, GoToSelectionEnd),
                ),
                ("Cycle back · on its length", two(CycleBack, CycleForward)),
                ("Cycle half · twice as long", two(CycleHalve, CycleDouble)),
                ("Follow the playhead", one(CatchPlayhead)),
                ("Scroll to the selection", one(ScrollToSelection)),
                ("Zoom to the cycle", one(ZoomToLocators)),
                ("Zoom in · out", two(ZoomIn, ZoomOut)),
                ("Zoom to fit", one(ZoomToFit)),
                ("Lanes taller · shorter", two(TallerLanes, ShorterLanes)),
            ],
        ),
        (
            "Edit",
            "",
            vec![
                ("Undo", one(Undo)),
                ("Redo", one(Redo)),
                ("Copy", one(CopyRegion)),
                ("Cut", one(CutRegion)),
                ("Paste at the playhead", one(PasteRegion)),
                ("Delete", one(Delete)),
                ("Select all", one(SelectAll)),
                ("Let go of what's picked", one(Deselect)),
                ("Keys to the tracks · the editor", one(SwitchKeys)),
                (
                    "Nudge by grid · beat · bar · tick",
                    Keys::Do(vec![
                        Box::new(NudgeByDivision),
                        Box::new(NudgeByBeat),
                        Box::new(NudgeByBar),
                        Box::new(NudgeByTick),
                    ]),
                ),
                ("Snap on or off", one(ToggleSnap)),
                ("Save", one(Save)),
            ],
        ),
        (
            "Regions",
            "",
            vec![
                ("Region before · after", two(RegionPrev, RegionNext)),
                (
                    "Add the one before · after",
                    two(TogglePrevRegion, ToggleNextRegion),
                ),
                ("First · last on the track", two(FirstRegion, LastRegion)),
                ("Shorter · longer", two(RegionShorter, RegionLonger)),
                ("Split at the cycle", one(SplitAtLocators)),
                ("Remove overlaps", one(RemoveOverlaps)),
                ("Trim to the next", one(TrimToNext)),
                ("Crop to the cycle", one(CropToLocators)),
                ("Fit MIDI to its notes", one(OptimalLengths)),
                ("Dequantize", one(Dequantize)),
                ("Rename", one(RenameRegion)),
                ("Name after the track", one(NameRegionsByTrack)),
                ("Alias to a copy", one(AliasToCopy)),
                ("To the selected track", one(RegionsToTrack)),
                ("To the playhead, then the next", one(RegionToPlayheadNext)),
                ("Nudge a grid step", two(NudgeRegionLeft, NudgeRegionRight)),
                ("Transpose a semitone", two(TransposeUp, TransposeDown)),
                (
                    "Transpose an octave",
                    two(TransposeOctaveUp, TransposeOctaveDown),
                ),
                ("Split at the playhead", one(Split)),
                ("Repeat after itself", one(RepeatRegion)),
                ("Join", one(JoinRegions)),
                ("Loop", one(LoopRegion)),
                ("Quantize", one(QuantizeRegion)),
                ("Mute", one(MuteRegion)),
                ("Move to the playhead", one(RegionToPlayhead)),
                (
                    "Start · end to the playhead",
                    two(StartToPlayhead, EndToPlayhead),
                ),
                ("Bounce", one(BounceRegion)),
                ("Stretch to the cycle", one(StretchToCycle)),
                ("Stretch to a bar", one(StretchToBar)),
                ("Stretch audio to the tempo", one(StretchToTempo)),
                ("Copy as you drag", Keys::Say("⌥ drag")),
            ],
        ),
        (
            "Tracks",
            "",
            vec![
                ("Track above · below", two(TrackPrev, TrackNext)),
                ("New track", one(NewTrack)),
                (
                    "New audio · instrument track",
                    two(NewAudioTrack, NewInstrumentTrack),
                ),
                ("Duplicate with its regions", one(DuplicateTrackContent)),
                ("Open · close its stack", two(OpenStack, CloseStack)),
                (
                    "This track taller · shorter",
                    two(TrackTaller, TrackShorter),
                ),
                ("Zoom this track", one(ToggleTrackZoom)),
                (
                    "Usual height · every track's",
                    two(ResetTrackZoom, ResetAllTrackZoom),
                ),
                ("Unmute · unsolo every track", two(UnmuteAll, UnsoloAll)),
                ("Duplicate", one(DuplicateTrack)),
                ("Rename", one(RenameTrack)),
                ("Delete", one(DeleteTrack)),
                ("Mute", one(MuteTrack)),
                ("Solo", one(SoloTrack)),
                ("Stack the picked tracks", one(CreateStack)),
                ("New pattern", one(NewPattern)),
            ],
        ),
        (
            "Select",
            "regions, or notes in the piano roll",
            vec![
                ("Every following", two(RegionsFollowing, NotesFollowing)),
                (
                    "Following on the track · of the pitch",
                    two(RegionsFollowingSameTrack, NotesFollowingSamePitch),
                ),
                (
                    "Inside the cycle",
                    two(RegionsInsideCycle, NotesInsideCycle),
                ),
                ("Overlapping", two(RegionsOverlapped, NotesOverlapped)),
                ("Equal", two(RegionsEqual, NotesEqual)),
                ("Similar", two(RegionsSimilar, NotesSimilar)),
                (
                    "Same place in the bar",
                    two(RegionsSameSubposition, NotesSameSubposition),
                ),
                ("Muted", two(RegionsMuted, NotesMuted)),
                ("Aliases of the region", one(SelectAliases)),
                ("Invert", two(InvertRegions, InvertNotes)),
                ("Deselect all", two(DeselectAllRegions, DeselectAllNotes)),
            ],
        ),
        (
            "Between the locators",
            "the cycle",
            vec![
                ("Cut the time out", one(CutBetweenLocators)),
                ("Insert silence", one(InsertSilence)),
                ("Play it twice", one(RepeatBetweenLocators)),
            ],
        ),
        (
            "Automation",
            "points picked",
            vec![
                ("Show the lanes", one(ToggleAutomation)),
                ("Every point on the lane", one(SelectAll)),
                ("Move a grid step", two(PointsLeft, PointsRight)),
                ("Move finely", two(PointsLeftFine, PointsRightFine)),
                ("Up · down", two(PointsUp, PointsDown)),
                ("Up · down more", two(PointsUpMore, PointsDownMore)),
                ("Duplicate after themselves", one(DuplicatePoints)),
                ("Let go", one(DeselectPoints)),
                ("Stretch in time or value", Keys::Say("drag the box")),
                ("Stretch about the middle", Keys::Say("⌥ drag the box")),
                ("Take a point out", Keys::Say("⌥ click")),
            ],
        ),
        (
            "Piano roll",
            "while it has the keys: click it, or Tab",
            vec![
                ("Select all", one(SelectAllNotes)),
                (
                    "Copy · cut · paste",
                    Keys::Do(vec![
                        Box::new(CopyNotes),
                        Box::new(CutNotes),
                        Box::new(PasteNotes),
                    ]),
                ),
                ("Duplicate", one(DuplicateNotes)),
                ("Note before · after", two(SelectPrevNote, SelectNextNote)),
                (
                    "Add the one before · after",
                    two(TogglePrevNote, ToggleNextNote),
                ),
                ("First · last note", two(FirstNote, LastNote)),
                ("A semitone up · down", two(NotesUp, NotesDown)),
                ("An octave up · down", two(NotesOctaveUp, NotesOctaveDown)),
                ("Nudge by the nudge value", two(NudgeLeft, NudgeRight)),
                ("Nudge finely", two(NudgeLeftFine, NudgeRightFine)),
                ("Shorter · longer", two(NotesShorter, NotesLonger)),
                ("Louder · softer", two(VelocityUp, VelocityDown)),
                (
                    "Louder · softer by 10",
                    two(VelocityUpMore, VelocityDownMore),
                ),
                ("Split at the playhead", one(SplitNotes)),
                ("Join", one(JoinNotes)),
                ("Repeat after themselves", one(RepeatNotes)),
                (
                    "Start · end to the playhead",
                    two(NotesStartToPlayhead, NotesEndToPlayhead),
                ),
                ("To the playhead", one(NotesToPlayhead)),
                ("To the playhead, then the next", one(NotesToPlayheadNext)),
                ("Delete doubled notes", one(DeleteDuplicateNotes)),
                ("Quantize", one(QuantizeNotes)),
                ("Mute", one(MuteNotes)),
                ("Let go", one(DeselectNotes)),
                ("Copy as you drag", Keys::Say("⌥ drag")),
                ("Move without the grid", Keys::Say("⌃ drag")),
                ("Every length the same", Keys::Say("⌥⇧ drag an end")),
            ],
        ),
        (
            "Tools",
            "T, then a letter",
            vec![
                ("The tools", one(ToolMenu)),
                ("Next · previous tool", two(NextTool, PrevTool)),
                ("Pointer", Keys::Say("T T")),
                ("Pencil", Keys::Say("T P")),
                ("Scissors", Keys::Say("T I")),
                ("Eraser", Keys::Say("T E")),
                ("Text · Glue", Keys::Say("T X  T G")),
                ("Solo · Mute", Keys::Say("T S  T M")),
                ("Zoom · Fade · Stretch", Keys::Say("T Z  T F  T R")),
                ("Back to the pointer", one(Deselect)),
            ],
        ),
        (
            "Show",
            "",
            vec![
                ("Editor", one(ToggleEditor)),
                ("Piano roll", one(TogglePianoRoll)),
                ("Mixer", one(ToggleMixer)),
                ("Inspector", one(ToggleInspector)),
                ("Browser", one(ToggleSamples)),
                ("Sounds page", one(ToggleSamplesPage)),
                ("Plugins", one(TogglePlugins)),
                ("Loops", one(ToggleLoops)),
                ("Score", one(ToggleScore)),
                ("Tempo lane", one(ToggleTempoLane)),
                ("Studio Agent", one(ToggleAgent)),
                ("Settings", one(ShowSettings)),
                ("These shortcuts", one(ShowShortcuts)),
            ],
        ),
        (
            "Song",
            "",
            vec![
                ("New song", one(NewSong)),
                ("Songs", one(ShowSongs)),
                ("Bounce", one(Export)),
                ("Close the front window", one(CloseWindow)),
                ("New empty song", one(NewEmptySong)),
                ("Export stems", one(ExportStems)),
                ("Full screen", one(FullScreen)),
                (
                    "Minimize · zoom the window",
                    two(MinimizeWindow, ZoomWindow),
                ),
                ("Hide Pina · the others", two(HideApp, HideOthers)),
                ("Quit", one(Quit)),
            ],
        ),
    ]
}

/// A key as the Mac writes it: ⌃⌥⇧⌘ then the key, ⌘⇧Z, ⌥←.
pub(crate) fn key_text(k: &KeybindingKeystroke) -> String {
    let m = k.modifiers();
    let mut s = String::new();
    for (on, c) in [
        (m.control, '⌃'),
        (m.alt, '⌥'),
        (m.shift, '⇧'),
        (m.platform, '⌘'),
    ] {
        if on {
            s.push(c);
        }
    }
    s + &match k.key() {
        "left" => "←".into(),
        "right" => "→".into(),
        "up" => "↑".into(),
        "down" => "↓".into(),
        "backspace" => "⌫".into(),
        "delete" => "⌦".into(),
        "enter" => "↩".into(),
        "escape" => "Esc".into(),
        "space" => "Space".into(),
        "home" => "Home".into(),
        "end" => "End".into(),
        "pageup" => "⇞".into(),
        "pagedown" => "⇟".into(),
        "tab" => "⇥".into(),
        key => key.to_uppercase(),
    }
}

impl Workspace {
    /// ⌘/: the sheet, its search field taking the keys; again, it goes.
    pub fn toggle_shortcuts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.shortcuts.is_some() {
            return self.close_shortcuts(window, cx);
        }
        let find =
            cx.new(|cx| TextInput::search("Search shortcuts: what it does, or its keys", cx));
        let typed = cx.observe(&find, |ws, input, cx| {
            if let Some(s) = &mut ws.shortcuts {
                s.query = input.read(cx).text().trim().to_lowercase();
            }
            cx.notify();
        });
        let done = cx.subscribe_in(&find, window, |ws, _, e: &TextEvent, window, cx| {
            if matches!(e, TextEvent::Cancel) {
                ws.close_shortcuts(window, cx);
            }
        });
        gpui::Focusable::focus_handle(find.read(cx), cx).focus(window, cx);
        self.menu = None;
        self.shortcuts = Some(Sheet {
            find,
            query: String::new(),
            _typed: typed,
            _done: done,
        });
        cx.notify();
    }

    pub fn close_shortcuts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.shortcuts = None;
        self.focus.focus(window, cx);
        cx.notify();
    }

    /// The groups and rows the search leaves, each row's keys written out.
    pub(crate) fn shortcut_rows(&self, cx: &App) -> Vec<Shown> {
        let words: Vec<String> = self
            .shortcuts
            .as_ref()
            .map(|s| s.query.split_whitespace().map(str::to_lowercase).collect())
            .unwrap_or_default();
        let keymap = cx.key_bindings();
        let keymap = keymap.borrow();
        groups()
            .into_iter()
            .filter_map(|(name, when, rows)| {
                let rows: Vec<(&'static str, Vec<String>)> = rows
                    .into_iter()
                    .map(|(what, keys)| {
                        let keys: Vec<String> = match keys {
                            Keys::Say(s) => vec![s.to_string()],
                            // One action: each of its keys (⌫ and ⌦); several: the newest
                            // key of each (⌘= over ⌘→).
                            Keys::Do(actions) => {
                                let keys = |a: &dyn Action| -> Vec<String> {
                                    keymap
                                        .bindings_for_action(a)
                                        .filter_map(|b| match b.keystrokes() {
                                            [k] => Some(key_text(k)),
                                            _ => None,
                                        })
                                        .collect()
                                };
                                match actions.as_slice() {
                                    [a] => keys(a.as_ref()).into_iter().take(2).collect(),
                                    // The same key for regions and notes: said once.
                                    many => {
                                        let mut all: Vec<String> = many
                                            .iter()
                                            .filter_map(|a| keys(a.as_ref()).pop())
                                            .collect();
                                        all.dedup();
                                        all
                                    }
                                }
                            }
                        };
                        (what, keys)
                    })
                    .filter(|(what, keys)| {
                        let all = format!("{name} {when} {what} {}", keys.join(" ")).to_lowercase();
                        words.iter().all(|w| all.contains(w.as_str()))
                    })
                    .collect();
                (!rows.is_empty()).then_some((name, when, rows))
            })
            .collect()
    }

    pub fn shortcuts_view(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let sheet = self.shortcuts.as_ref()?;
        let groups = self.shortcut_rows(cx);
        let empty = groups.is_empty();
        // Three columns, each group to the shortest so far.
        let mut columns: [(usize, Vec<Div>); 3] = Default::default();
        for (name, when, rows) in groups {
            let col = (0..3).min_by_key(|&c| columns[c].0).unwrap_or(0);
            columns[col].0 += rows.len() + 3;
            columns[col].1.push(group_view(name, when, rows));
        }
        let size = window.viewport_size();
        let h = (f32::from(size.height) - 96.0).clamp(360.0, 780.0);
        let w = (f32::from(size.width) - 64.0).clamp(640.0, 1080.0);
        let body = div()
            .id("shortcuts-body")
            .flex_1()
            .min_h(px(0.))
            .overflow_y_scroll()
            .px(px(22.))
            .pt(px(6.))
            .pb(px(22.))
            .when(empty, |d| {
                d.flex().items_center().justify_center().child(
                    div()
                        .text_size(px(13.))
                        .text_color(rgb(TEXT_3))
                        .child(format!("No shortcut for “{}”", sheet.query)),
                )
            })
            .when(!empty, |d| {
                d.flex()
                    .gap(px(26.))
                    .children(columns.into_iter().map(|(_, groups)| {
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .flex_col()
                            .gap(px(18.))
                            .children(groups)
                    }))
            });
        let panel = div()
            .id("shortcuts")
            .key_context("Shortcuts")
            .debug_selector(|| "shortcuts".into())
            .w(px(w))
            .h(px(h))
            .flex()
            .flex_col()
            .rounded(px(16.))
            .bg(rgb(0x17171c))
            .border_1()
            .border_color(rgb(LINE_STRONG))
            .shadow_lg()
            .overflow_hidden()
            .font_family(SANS)
            .text_color(rgb(TEXT))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .h(px(60.))
                    .flex_none()
                    .px(px(22.))
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(icon(Icon::Keyboard, 17.).text_color(rgb(TEXT_2)))
                    .child(
                        div()
                            .text_size(px(16.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .whitespace_nowrap()
                            .child("Keyboard shortcuts"),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .w(px(340.))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(icon(Icon::Search, 13.).text_color(rgb(TEXT_3)))
                            .child(
                                div().flex_1().child(crate::pages::genres::field_box(
                                    Some(sheet.find.clone()),
                                    "shortcuts-search",
                                )),
                            ),
                    )
                    .child(
                        div()
                            .id("shortcuts-close")
                            .debug_selector(|| "shortcuts-close".into())
                            .size(px(28.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(7.))
                            .cursor_pointer()
                            .text_color(rgb(TEXT_3))
                            .hover(|d| d.bg(rgb(RAISED)).text_color(rgb(TEXT)))
                            .child(icon(Icon::Close, 14.))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _, window, cx| ws.close_shortcuts(window, cx)),
                            ),
                    ),
            )
            .child(div().h(px(1.)).flex_none().bg(rgb(LINE)))
            .child(body)
            .child(
                div()
                    .h(px(34.))
                    .flex_none()
                    .px(px(22.))
                    .flex()
                    .items_center()
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .text_size(px(11.5))
                    .text_color(rgb(TEXT_3))
                    .child("Keys without ⌘ or ⌥ stand down while a field is being typed in · Esc closes"),
            );
        Some(
            gpui::deferred(
                div()
                    .id("shortcuts-backdrop")
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(alpha(0x000000, 0.55))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, window, cx| ws.close_shortcuts(window, cx)),
                    )
                    .child(panel),
            )
            .with_priority(3),
        )
    }
}

/// A group: its name (and when its keys work), then what each key does beside its keys.
fn group_view(name: &str, when: &str, rows: Vec<(&'static str, Vec<String>)>) -> Div {
    let cap = |k: String| {
        div()
            .h(px(20.))
            .min_w(px(20.))
            .px(px(5.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(5.))
            .bg(rgb(RAISED))
            .border_1()
            .border_color(rgb(LINE_STRONG))
            .text_size(px(11.5))
            .font_weight(FontWeight::MEDIUM)
            .text_color(rgb(TEXT))
            .whitespace_nowrap()
            .child(k)
    };
    div()
        .flex()
        .flex_col()
        .child(
            div()
                .h(px(28.))
                .flex()
                .items_end()
                .gap(px(6.))
                .pb(px(6.))
                .mb(px(2.))
                .border_b_1()
                .border_color(rgb(LINE))
                .child(
                    div()
                        .text_size(px(11.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(TEXT_2))
                        .child(name.to_uppercase()),
                )
                .when(!when.is_empty(), |d| {
                    d.child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_4))
                            .child(when.to_string()),
                    )
                }),
        )
        .children(rows.into_iter().map(|(what, keys)| {
            div()
                .min_h(px(26.))
                .px(px(4.))
                .flex()
                .items_center()
                .gap(px(10.))
                .rounded(px(5.))
                .hover(|d| d.bg(alpha(0xffffff, 0.035)))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .text_size(px(12.5))
                        .text_color(rgb(TEXT_2))
                        .child(what),
                )
                .child(
                    div()
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap(px(4.))
                        .when(keys.is_empty(), |d| {
                            d.child(div().text_size(px(11.)).text_color(rgb(TEXT_4)).child("—"))
                        })
                        .children(keys.into_iter().map(cap)),
                )
        }))
}
