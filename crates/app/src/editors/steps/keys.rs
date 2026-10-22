//! The step sequencer's keyboard shortcuts, while it's open.

use gpui::{App, KeyBinding, actions};
use pina_engine::song::project::{RATES, rate_name};

use super::*;

actions!(
    steps,
    [
        ShowBrowser,
        RepeatStep,
        CutSteps,
        CopySteps,
        PasteSteps,
        CopyRow,
        PasteRow,
        PasteRowSteps,
        PasteRowSettings,
        PasteToNewRow,
        DuplicateRow,
        DeleteRow,
        DeleteUnused,
        ClearStep,
        ClearRow,
        ClearPattern,
        ClearRowValues,
        ClearAllValues,
        LearnAdd,
        StoreTemplate,
        RevertTemplate,
        SemitoneUp,
        SemitoneDown,
        OctaveUp,
        OctaveDown,
        RandomizeRow,
        RandomizeAll,
        StepAbove,
        StepBelow,
        StepBefore,
        StepAfter,
        ToggleStep,
        LiveRecord,
        Mono,
        RenameRow,
        NextPage,
        SavePattern,
        SaveTemplate,
        PatternForward,
        PatternBackward,
        PatternPingPong,
        PatternShuffle,
        PatternPendulum,
        PatternWander,
        PatternRandom,
        RowForward,
        RowBackward,
        RowPingPong,
        RowShuffle,
        RowPendulum,
        RowWander,
        RowRandom,
        RotatePatternRight,
        RotatePatternLeft,
        RotateRowRight,
        RotateRowLeft,
        RotateValuesRight,
        RotateValuesLeft,
        SoloRow,
        MuteRow,
        LoopStartUp,
        LoopStartDown,
        LoopEndUp,
        LoopEndDown,
        ShowSubrows,
        HideSubrows,
        ShowAllSubrows,
        HideAllSubrows,
        ModeOnOff,
        ModeVelocity,
        ModeGate,
        ModeTie,
        ModeNote,
        ModeOctave,
        ModeLoop,
        ModeRepeat,
        ModeChance,
        ModeOffset,
        ModeRate,
        ModeSkip,
        PatternRateUp,
        PatternRateDown,
        RowRateUp,
        RowRateDown,
    ]
);

pub fn bind_keys(cx: &mut App) {
    let s = Some("Steps && !TextInput");
    cx.bind_keys([
        KeyBinding::new("alt-shift-b", ShowBrowser, s),
        KeyBinding::new("cmd-r", RepeatStep, s),
        KeyBinding::new("cmd-x", CutSteps, s),
        KeyBinding::new("cmd-c", CopySteps, s),
        KeyBinding::new("cmd-v", PasteSteps, s),
        KeyBinding::new("ctrl-c", CopyRow, s),
        KeyBinding::new("ctrl-v", PasteRow, s),
        KeyBinding::new("alt-shift-v", PasteRowSteps, s),
        KeyBinding::new("ctrl-alt-shift-v", PasteRowSettings, s),
        KeyBinding::new("ctrl-shift-v", PasteToNewRow, s),
        KeyBinding::new("cmd-d", DuplicateRow, s),
        KeyBinding::new("cmd-backspace", DeleteRow, s),
        KeyBinding::new("cmd-shift-backspace", DeleteUnused, s),
        KeyBinding::new("ctrl-backspace", ClearStep, s),
        KeyBinding::new("ctrl-cmd-backspace", ClearRow, s),
        KeyBinding::new("ctrl-shift-cmd-backspace", ClearPattern, s),
        KeyBinding::new("alt-cmd-backspace", ClearRowValues, s),
        KeyBinding::new("alt-shift-cmd-backspace", ClearAllValues, s),
        KeyBinding::new("alt-cmd-l", LearnAdd, s),
        KeyBinding::new("ctrl-alt-cmd-d", StoreTemplate, s),
        KeyBinding::new("ctrl-alt-shift-d", RevertTemplate, s),
        KeyBinding::new("alt-up", SemitoneUp, s),
        KeyBinding::new("alt-down", SemitoneDown, s),
        KeyBinding::new("alt-shift-up", OctaveUp, s),
        KeyBinding::new("alt-shift-down", OctaveDown, s),
        KeyBinding::new("alt-cmd-r", RandomizeRow, s),
        KeyBinding::new("shift-cmd-r", RandomizeAll, s),
        KeyBinding::new("up", StepAbove, s),
        KeyBinding::new("down", StepBelow, s),
        KeyBinding::new("left", StepBefore, s),
        KeyBinding::new("right", StepAfter, s),
        KeyBinding::new("'", ToggleStep, s),
        KeyBinding::new("shift-r", LiveRecord, s),
        KeyBinding::new("ctrl-shift-cmd-m", Mono, s),
        KeyBinding::new("shift-enter", RenameRow, s),
        KeyBinding::new("alt-`", NextPage, s),
        KeyBinding::new("shift-cmd-p", SavePattern, s),
        KeyBinding::new("shift-cmd-t", SaveTemplate, s),
        KeyBinding::new("ctrl-alt-]", PatternForward, s),
        KeyBinding::new("ctrl-alt-[", PatternBackward, s),
        KeyBinding::new("ctrl-alt-p", PatternPingPong, s),
        KeyBinding::new("ctrl-alt-\\", PatternShuffle, s),
        KeyBinding::new("ctrl-alt-y", PatternPendulum, s),
        KeyBinding::new("ctrl-alt-u", PatternWander, s),
        KeyBinding::new("ctrl-alt-j", PatternRandom, s),
        KeyBinding::new("ctrl-]", RowForward, s),
        KeyBinding::new("ctrl-[", RowBackward, s),
        KeyBinding::new("ctrl-p", RowPingPong, s),
        KeyBinding::new("ctrl-\\", RowShuffle, s),
        KeyBinding::new("ctrl-y", RowPendulum, s),
        KeyBinding::new("ctrl-u", RowWander, s),
        KeyBinding::new("ctrl-j", RowRandom, s),
        KeyBinding::new("ctrl-alt-cmd-right", RotatePatternRight, s),
        KeyBinding::new("ctrl-alt-cmd-left", RotatePatternLeft, s),
        KeyBinding::new("alt-cmd-right", RotateRowRight, s),
        KeyBinding::new("alt-cmd-left", RotateRowLeft, s),
        KeyBinding::new("alt-shift-right", RotateValuesRight, s),
        KeyBinding::new("alt-shift-left", RotateValuesLeft, s),
        KeyBinding::new("ctrl-shift-s", SoloRow, s),
        KeyBinding::new("ctrl-shift-m", MuteRow, s),
        KeyBinding::new("ctrl-alt-=", LoopStartUp, s),
        KeyBinding::new("ctrl-alt--", LoopStartDown, s),
        KeyBinding::new("alt-cmd-=", LoopEndUp, s),
        KeyBinding::new("alt-cmd--", LoopEndDown, s),
        KeyBinding::new("ctrl-cmd-right", ShowSubrows, s),
        KeyBinding::new("ctrl-cmd-left", HideSubrows, s),
        KeyBinding::new("ctrl-shift-cmd-right", ShowAllSubrows, s),
        KeyBinding::new("ctrl-shift-cmd-left", HideAllSubrows, s),
        KeyBinding::new("ctrl-alt-s", ModeOnOff, s),
        KeyBinding::new("ctrl-alt-v", ModeVelocity, s),
        KeyBinding::new("ctrl-alt-g", ModeGate, s),
        KeyBinding::new("ctrl-alt-t", ModeTie, s),
        KeyBinding::new("ctrl-alt-n", ModeNote, s),
        KeyBinding::new("ctrl-alt-o", ModeOctave, s),
        KeyBinding::new("ctrl-alt-l", ModeLoop, s),
        KeyBinding::new("ctrl-alt-r", ModeRepeat, s),
        KeyBinding::new("ctrl-alt-c", ModeChance, s),
        KeyBinding::new("ctrl-alt-f", ModeOffset, s),
        KeyBinding::new("ctrl-alt-a", ModeRate, s),
        KeyBinding::new("ctrl-alt-k", ModeSkip, s),
        KeyBinding::new("ctrl-shift-=", PatternRateUp, s),
        KeyBinding::new("ctrl-shift--", PatternRateDown, s),
        KeyBinding::new("ctrl-=", RowRateUp, s),
        KeyBinding::new("ctrl--", RowRateDown, s),
    ]);
}

/// The next straight rate faster (or slower) than `ticks`.
fn next_rate(ticks: u64, faster: bool) -> u64 {
    let straight = &RATES[..9];
    let k = straight.iter().position(|r| r.1 == ticks).unwrap_or(6);
    let k = if faster {
        (k + 1).min(8)
    } else {
        k.saturating_sub(1)
    };
    straight[k].1
}

impl Workspace {
    /// A shortcut's op, with what it needs from the view.
    fn steps_key(&mut self, key: &dyn gpui::Action, cx: &mut Context<Self>) {
        let r = self.steps.row;
        let lane = self.steps.mode.lane();
        let Some((_, p)) = self.shown_pattern() else {
            return;
        };
        let rows = p.rows.len();
        let n = p.steps();
        let (row_rate, step) = (p.rows.get(r).and_then(|x| x.rate).unwrap_or(p.step), p.step);
        let loops = p.rows.get(r).map_or((0, n), |x| x.loop_range(n));
        let cursor = self.steps.cursor.unwrap_or((r, 0));
        let any = key.as_any();
        let is = |a: &dyn std::any::Any, t: std::any::TypeId| a.type_id() == t;
        macro_rules! on {
            ($($action:ident => $op:expr),* $(,)?) => {
                $(if is(any, std::any::TypeId::of::<$action>()) {
                    let op: Option<StepsOp> = $op;
                    if let Some(op) = op {
                        self.steps_op(op, cx);
                    }
                    cx.notify();
                    return;
                })*
            };
        }
        let mode = |m: EditMode| Some(StepsOp::Mode(m));
        let lane_mode = |l: Lane| Some(StepsOp::Mode(EditMode::Lane(l)));
        let pane = self.steps.pane;
        let page = self.steps.page;
        let shown = self.steps.geom.borrow().shown.1.max(1);
        let pages = n.div_ceil(shown);
        on! {
            ShowBrowser => Some(StepsOp::Pane((pane != Some(Pane::Browser)).then_some(Pane::Browser))),
            RepeatStep => Some(StepsOp::RepeatStep),
            CutSteps => Some(StepsOp::CutSteps),
            CopySteps => Some(StepsOp::CopySteps),
            PasteSteps => Some(StepsOp::PasteStepsAt(cursor.0, cursor.1)),
            CopyRow => Some(StepsOp::CopyRow(r)),
            PasteRow => Some(StepsOp::PasteRow(r)),
            PasteRowSteps => Some(StepsOp::PasteSteps(r)),
            PasteRowSettings => Some(StepsOp::PasteRowSettings(r)),
            PasteToNewRow => Some(StepsOp::PasteToNewRow),
            DuplicateRow => Some(StepsOp::DuplicateRow(r)),
            DeleteRow => Some(StepsOp::DeleteRow(r)),
            DeleteUnused => Some(StepsOp::DeleteUnused),
            ClearStep => Some(StepsOp::ClearStep),
            ClearRow => Some(StepsOp::ClearRow(r)),
            ClearPattern => Some(StepsOp::ClearPattern),
            ClearRowValues => Some(StepsOp::ClearRowValues(r, lane)),
            ClearAllValues => Some(StepsOp::ClearPatternValues(None)),
            LearnAdd => Some(StepsOp::Learn(Some(Learn::Add))),
            StoreTemplate => Some(StepsOp::StoreTemplate),
            RevertTemplate => Some(StepsOp::RevertTemplate),
            SemitoneUp => Some(StepsOp::Transpose(1)),
            SemitoneDown => Some(StepsOp::Transpose(-1)),
            OctaveUp => Some(StepsOp::Transpose(12)),
            OctaveDown => Some(StepsOp::Transpose(-12)),
            RandomizeRow => Some(StepsOp::RandomizeRow(r, lane)),
            RandomizeAll => Some(StepsOp::RandomizePattern(None)),
            PatternForward => Some(StepsOp::PatternPlay(PlayMode::Forward)),
            PatternBackward => Some(StepsOp::PatternPlay(PlayMode::Backward)),
            PatternPingPong => Some(StepsOp::PatternPlay(PlayMode::PingPong)),
            PatternShuffle => Some(StepsOp::PatternPlay(PlayMode::Shuffle)),
            PatternPendulum => Some(StepsOp::PatternPlay(PlayMode::Pendulum)),
            PatternWander => Some(StepsOp::PatternPlay(PlayMode::Wander)),
            PatternRandom => Some(StepsOp::PatternPlay(PlayMode::Random)),
            RowForward => Some(StepsOp::RowPlay(r, Some(PlayMode::Forward))),
            RowBackward => Some(StepsOp::RowPlay(r, Some(PlayMode::Backward))),
            RowPingPong => Some(StepsOp::RowPlay(r, Some(PlayMode::PingPong))),
            RowShuffle => Some(StepsOp::RowPlay(r, Some(PlayMode::Shuffle))),
            RowPendulum => Some(StepsOp::RowPlay(r, Some(PlayMode::Pendulum))),
            RowWander => Some(StepsOp::RowPlay(r, Some(PlayMode::Wander))),
            RowRandom => Some(StepsOp::RowPlay(r, Some(PlayMode::Random))),
            RotatePatternRight => Some(StepsOp::RotatePattern(1)),
            RotatePatternLeft => Some(StepsOp::RotatePattern(-1)),
            RotateRowRight => Some(StepsOp::RotateRow(r, None, 1)),
            RotateRowLeft => Some(StepsOp::RotateRow(r, None, -1)),
            RotateValuesRight => Some(StepsOp::RotateRow(r, lane, 1)),
            RotateValuesLeft => Some(StepsOp::RotateRow(r, lane, -1)),
            SoloRow => Some(StepsOp::Solo(r)),
            MuteRow => Some(StepsOp::Mute(r)),
            LoopStartUp => Some(StepsOp::RowLoop(r, (loops.0 + 1).min(loops.1 - 1), loops.1)),
            LoopStartDown => Some(StepsOp::RowLoop(r, loops.0.saturating_sub(1), loops.1)),
            LoopEndUp => Some(StepsOp::RowLoop(r, loops.0, (loops.1 + 1).min(n))),
            LoopEndDown => Some(StepsOp::RowLoop(r, loops.0, (loops.1 - 1).max(loops.0 + 1))),
            ModeOnOff => mode(EditMode::OnOff),
            ModeVelocity => lane_mode(Lane::Velocity),
            ModeGate => lane_mode(Lane::Gate),
            ModeTie => lane_mode(Lane::Tie),
            ModeNote => lane_mode(Lane::Note),
            ModeOctave => lane_mode(Lane::Octave),
            ModeLoop => mode(EditMode::Loop),
            ModeRepeat => lane_mode(Lane::Repeat),
            ModeChance => lane_mode(Lane::Chance),
            ModeOffset => lane_mode(Lane::Offset),
            ModeRate => lane_mode(Lane::Rate),
            ModeSkip => lane_mode(Lane::Skip),
            PatternRateUp => Some(StepsOp::PatternRate(next_rate(step, true))),
            PatternRateDown => Some(StepsOp::PatternRate(next_rate(step, false))),
            RowRateUp => Some(StepsOp::RowRate(r, Some(next_rate(row_rate, true)))),
            RowRateDown => Some(StepsOp::RowRate(r, Some(next_rate(row_rate, false)))),
            SavePattern => Some(StepsOp::SavePattern),
            SaveTemplate => Some(StepsOp::SaveTemplate),
            RenameRow => Some(StepsOp::Rename(r)),
        }
        // What isn't an op: the cursor, the live and mono switches, the page, the subrows.
        let (cr, ci) = cursor;
        let open = |ws: &mut Self, all: bool, to: bool, cx: &mut Context<Self>| {
            ws.edit_pattern(cx, |p| {
                for (k, row) in p.rows.iter_mut().enumerate() {
                    if all || k == r {
                        row.open = to;
                        if to && row.subrows.is_empty() {
                            row.subrows = default_subrows(row);
                        }
                    }
                }
            });
        };
        let t = any.type_id();
        use std::any::TypeId as T;
        if t == T::of::<StepAbove>() {
            self.steps.cursor = Some((cr.saturating_sub(1), ci));
        } else if t == T::of::<StepBelow>() {
            self.steps.cursor = Some(((cr + 1).min(rows.saturating_sub(1)), ci));
        } else if t == T::of::<StepBefore>() {
            self.steps.cursor = Some((cr, ci.saturating_sub(1)));
        } else if t == T::of::<StepAfter>() {
            self.steps.cursor = Some((cr, (ci + 1).min(n - 1)));
        } else if t == T::of::<ToggleStep>() {
            let on = self
                .shown_pattern()
                .and_then(|(_, p)| p.rows.get(cr))
                .is_some_and(|row| row.on(ci));
            self.edit_row(cr, cx, |row, _| row.set_on(ci, !on));
        } else if t == T::of::<LiveRecord>() {
            self.steps.live ^= true;
        } else if t == T::of::<Mono>() {
            self.steps.mono ^= true;
        } else if t == T::of::<NextPage>() {
            self.steps.page = (page + 1) % pages.max(1);
        } else if t == T::of::<ShowSubrows>() {
            open(self, false, true, cx);
        } else if t == T::of::<HideSubrows>() {
            open(self, false, false, cx);
        } else if t == T::of::<ShowAllSubrows>() {
            open(self, true, true, cx);
        } else if t == T::of::<HideAllSubrows>() {
            open(self, true, false, cx);
        }
        if let Some((r, _)) = self.steps.cursor {
            self.steps.row = r;
        }
        let _ = rate_name;
        cx.notify();
    }

    /// The step sequencer's shortcuts, as actions on the workspace.
    pub fn steps_actions(&self, d: gpui::Div, cx: &mut Context<Self>) -> gpui::Div {
        macro_rules! wire {
            ($d:expr, $($action:ident),*) => {
                $d$(.on_action(cx.listener(|ws, a: &$action, _, cx| ws.steps_key(a, cx))))*
            };
        }
        wire!(
            d,
            ShowBrowser,
            RepeatStep,
            CutSteps,
            CopySteps,
            PasteSteps,
            CopyRow,
            PasteRow,
            PasteRowSteps,
            PasteRowSettings,
            PasteToNewRow,
            DuplicateRow,
            DeleteRow,
            DeleteUnused,
            ClearStep,
            ClearRow,
            ClearPattern,
            ClearRowValues,
            ClearAllValues,
            LearnAdd,
            StoreTemplate,
            RevertTemplate,
            SemitoneUp,
            SemitoneDown,
            OctaveUp,
            OctaveDown,
            RandomizeRow,
            RandomizeAll,
            StepAbove,
            StepBelow,
            StepBefore,
            StepAfter,
            ToggleStep,
            LiveRecord,
            Mono,
            RenameRow,
            NextPage,
            SavePattern,
            SaveTemplate,
            PatternForward,
            PatternBackward,
            PatternPingPong,
            PatternShuffle,
            PatternPendulum,
            PatternWander,
            PatternRandom,
            RowForward,
            RowBackward,
            RowPingPong,
            RowShuffle,
            RowPendulum,
            RowWander,
            RowRandom,
            RotatePatternRight,
            RotatePatternLeft,
            RotateRowRight,
            RotateRowLeft,
            RotateValuesRight,
            RotateValuesLeft,
            SoloRow,
            MuteRow,
            LoopStartUp,
            LoopStartDown,
            LoopEndUp,
            LoopEndDown,
            ShowSubrows,
            HideSubrows,
            ShowAllSubrows,
            HideAllSubrows,
            ModeOnOff,
            ModeVelocity,
            ModeGate,
            ModeTie,
            ModeNote,
            ModeOctave,
            ModeLoop,
            ModeRepeat,
            ModeChance,
            ModeOffset,
            ModeRate,
            ModeSkip,
            PatternRateUp,
            PatternRateDown,
            RowRateUp,
            RowRateDown
        )
    }
}
