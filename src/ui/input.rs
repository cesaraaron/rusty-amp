use std::sync::atomic::Ordering::Relaxed;

use crate::dsp::{AmpModel, CHAIN_LEN, CabModel, ChainStage, Params};

use super::config::{
    ADD_TILE, AMP_END, AMP_START, CHAIN_TILE, KNOBS, MIC_END, MIC_START, PEDALS, PRACTICE_TILE,
    Panels, pedal_of,
};

/// A knob is reachable only if it belongs to the amp/mic (always present) or to
/// a pedal currently on the board.
fn knob_visible(knob: usize, board: &[bool]) -> bool {
    match pedal_of(knob) {
        Some(p) => board[p],
        None => true,
    }
}

/// Panel owning a focus: 1 = live-order ribbon, 2 = amp/cab (selectors plus
/// amp/mic knobs), 3 = practice timeline, 4 = pedalboard (pedal knobs + ADD).
pub(super) fn panel_of(focus: Option<usize>) -> u8 {
    match focus {
        Some(i) if i == CHAIN_TILE => 1,
        Some(i) if i == PRACTICE_TILE => 3,
        Some(i) if i == ADD_TILE => 4,
        Some(i) if pedal_of(i).is_some() => 4,
        _ => 2,
    }
}

fn panel_visible(panels: &Panels, panel: u8) -> bool {
    match panel {
        1 => true, // the ribbon is always visible
        2 => panels.amp,
        3 => panels.timeline,
        _ => panels.rig,
    }
}

fn set_panel_visible(panels: &mut Panels, panel: u8, visible: bool) {
    match panel {
        2 => panels.amp = visible,
        3 => panels.timeline = visible,
        4 => panels.rig = visible,
        _ => {}
    }
}

/// Rendered chain stages as `(slot, stage)` pairs: on-board pedals in chain
/// order with the amp+cab block at its slot. This is what the ribbon draws,
/// what the cursor steps through, and what moves swap.
pub(super) fn rendered_stages(order: &[u8; CHAIN_LEN], board: &[bool]) -> Vec<(usize, ChainStage)> {
    order
        .iter()
        .enumerate()
        .filter_map(|(slot, &raw)| {
            let stage = ChainStage::from_u8(raw)?;
            if stage == ChainStage::AmpCab {
                return Some((slot, stage));
            }
            let pi = stage.pedal_index()?;
            board
                .get(pi)
                .copied()
                .unwrap_or(false)
                .then_some((slot, stage))
        })
        .collect()
}

/// Where `Tab` lands when entering a panel: the ribbon, the selectors, the
/// timeline, or the first on-board pedal in chain order (`+ADD` when the board
/// is empty).
pub(super) fn panel_entry(panel: u8, board: &[bool], order: &[u8; CHAIN_LEN]) -> Option<usize> {
    match panel {
        1 => Some(CHAIN_TILE),
        2 => Some(AMP_START),
        3 => Some(PRACTICE_TILE),
        _ => order
            .iter()
            .filter_map(|&raw| ChainStage::from_u8(raw))
            .find_map(|stage| {
                let pi = stage.pedal_index()?;
                board
                    .get(pi)
                    .copied()
                    .unwrap_or(false)
                    .then_some(PEDALS[pi].start)
            })
            .or(Some(ADD_TILE)),
    }
}

/// Next/previous visible panel in 1→2→3→4 order, wrapping around panel 1 (which
/// is always visible, so this always terminates).
fn next_panel(panel: u8, panels: &Panels, dir: i32) -> u8 {
    let mut p = panel;
    for _ in 0..4 {
        p = (((p as i32 - 1 + dir).rem_euclid(4)) + 1) as u8;
        if panel_visible(panels, p) {
            return p;
        }
    }
    1
}

/// First entry scanning panels 1→2→3→4, for focus repair after a panel hides.
fn first_visible_entry(panels: &Panels, board: &[bool], order: &[u8; CHAIN_LEN]) -> Option<usize> {
    for p in 1..=4 {
        if panel_visible(panels, p) {
            return panel_entry(p, board, order);
        }
    }
    Some(CHAIN_TILE)
}

/// Number-key tri-state for panels 1–4: a hidden panel is shown and focused; a
/// visible but unfocused panel is focused; the focused panel hides (except the
/// ribbon, which only focuses). Other keys leave everything untouched.
pub(super) fn press_number(
    n: u8,
    focus: Option<usize>,
    board: &[bool],
    panels: &Panels,
    order: &[u8; CHAIN_LEN],
) -> (Panels, Option<usize>) {
    if !(1..=4).contains(&n) {
        return (*panels, focus);
    }
    let mut panels = *panels;
    if !panel_visible(&panels, n) {
        set_panel_visible(&mut panels, n, true);
        return (panels, panel_entry(n, board, order));
    }
    if panel_of(focus) == n {
        if n == 1 {
            return (panels, focus);
        }
        set_panel_visible(&mut panels, n, false);
        return (panels, first_visible_entry(&panels, board, order));
    }
    (panels, panel_entry(n, board, order))
}

/// `Tab` / `Shift-Tab`: jump to the next / previous visible panel's entry.
pub(super) fn next_panel_focus(
    focus: Option<usize>,
    board: &[bool],
    panels: &Panels,
    order: &[u8; CHAIN_LEN],
) -> Option<usize> {
    panel_entry(next_panel(panel_of(focus), panels, 1), board, order)
}

pub(super) fn prev_panel_focus(
    focus: Option<usize>,
    board: &[bool],
    panels: &Panels,
    order: &[u8; CHAIN_LEN],
) -> Option<usize> {
    panel_entry(next_panel(panel_of(focus), panels, -1), board, order)
}

fn cycle(stops: &[Option<usize>], current: Option<usize>, dir: i32) -> Option<usize> {
    let n = stops.len() as i32;
    let cur = stops.iter().position(|&s| s == current).unwrap_or(0) as i32;
    stops[(((cur + dir) % n + n) % n) as usize]
}

/// Knob stops within one panel for `←`/`→`: panel 2 walks amp knobs → mic
/// knobs (wrapping); panel 4 walks on-board pedal knobs in chain order plus
/// `+ADD`. Panels 1 and 3 own their arrows (stage cursor / seek), so they
/// contribute no stops.
fn panel_knob_stops(panel: u8, board: &[bool], order: &[u8; CHAIN_LEN]) -> Vec<Option<usize>> {
    let mut v = Vec::new();
    match panel {
        2 => {
            v.extend((AMP_START..AMP_END).chain(MIC_START..MIC_END).map(Some));
        }
        4 => {
            for &raw in order {
                if let Some(stage) = ChainStage::from_u8(raw)
                    && let Some(pi) = stage.pedal_index()
                {
                    v.extend(
                        (PEDALS[pi].start..PEDALS[pi].end)
                            .filter(|&k| knob_visible(k, board))
                            .map(Some),
                    );
                }
            }
            v.push(Some(ADD_TILE));
        }
        _ => {}
    }
    if v.is_empty() {
        v.push(None);
    }
    v
}

/// `←`/`→` inside the focused panel (2 or 4). Panels 1 and 3 handle their own
/// arrows, so focus there is returned untouched.
pub(super) fn step_knob_in_panel(
    focus: Option<usize>,
    board: &[bool],
    order: &[u8; CHAIN_LEN],
    dir: i32,
) -> Option<usize> {
    match panel_of(focus) {
        2 | 4 => cycle(&panel_knob_stops(panel_of(focus), board, order), focus, dir),
        _ => focus,
    }
}

/// Keep `focus` on a visible panel after one hides: focus inside a hidden panel
/// jumps to the first visible panel's entry, everything else stays put.
pub(super) fn ensure_focus_visible(
    focus: Option<usize>,
    board: &[bool],
    panels: &Panels,
    order: &[u8; CHAIN_LEN],
) -> Option<usize> {
    if panel_visible(panels, panel_of(focus)) {
        focus
    } else {
        first_visible_entry(panels, board, order)
    }
}

/// Move the ribbon cursor to the neighbouring rendered stage (wrapping around
/// the ends). A stale cursor (its pedal left the board) re-anchors at the
/// nearest end instead.
pub(super) fn move_chain_cursor(
    order: &[u8; CHAIN_LEN],
    board: &[bool],
    cursor: ChainStage,
    dir: i32,
) -> ChainStage {
    let rendered = rendered_stages(order, board);
    if rendered.is_empty() {
        return cursor;
    }
    if let Some(pos) = rendered.iter().position(|&(_, s)| s == cursor) {
        let n = rendered.len() as i32;
        rendered[(((pos as i32 + dir) % n + n) % n) as usize].1
    } else if dir < 0 {
        rendered.last().map(|&(_, s)| s).unwrap_or(cursor)
    } else {
        rendered.first().map(|&(_, s)| s).unwrap_or(cursor)
    }
}

/// Move the cursor's stage one rendered slot earlier (`dir < 0`, `[`) or later
/// (`dir > 0`, `]`). The cursor follows its stage. Off-board stages hold their
/// slots silently; the ends refuse. Returns true when something moved.
pub(super) fn move_selected_stage(
    params: &Params,
    board: &[bool],
    cursor: ChainStage,
    dir: i32,
) -> bool {
    let order = params.chain_slots();
    let rendered = rendered_stages(&order, board);
    let Some(pos) = rendered.iter().position(|&(_, s)| s == cursor) else {
        return false;
    };
    let other = pos as i32 + dir.signum();
    if other < 0 || other >= rendered.len() as i32 {
        return false;
    }
    let (a, _) = rendered[pos];
    let (b, _) = rendered[other as usize];
    let mut order = order;
    order.swap(a, b);
    params.set_chain_order(&order);
    true
}

/// Bypass/un-bypass the cursor's pedal on the ribbon. The amp+cab block and
/// off-board stages are a no-op. Returns true when a flag flipped.
pub(super) fn toggle_stage(params: &Params, board: &[bool], cursor: ChainStage) -> bool {
    let Some(pi) = cursor.pedal_index() else {
        return false;
    };
    if !board.get(pi).copied().unwrap_or(false) {
        return false;
    }
    toggle_pedal(params, PEDALS[pi].start);
    true
}

pub(super) fn nudge(params: &Params, idx: usize, delta: f32) {
    let atom = (KNOBS[idx].param)(params);
    let new = (atom.load(Relaxed) + delta).clamp(0.0, 1.0);
    atom.store(new, Relaxed);
}

/// Apply an amp-modal pick: a built-in index selects that model and returns to
/// built-in; the trailing index (present only when `au_loaded`) activates the
/// loaded AU instead. Out-of-range picks are ignored.
pub(super) fn select_amp(params: &Params, index: usize, au_loaded: bool) {
    if index < AmpModel::ALL.len() {
        params.amp_model.store(AmpModel::ALL[index] as u8, Relaxed);
        params.amp_external_active.store(false, Relaxed);
    } else if au_loaded && index == AmpModel::ALL.len() {
        params.amp_external_active.store(true, Relaxed);
    }
}

/// Apply a cab-modal pick: a built-in index selects that model and returns to
/// built-in (the IR stays loaded, so `X` can re-engage it); the trailing index
/// (present only when `ir_loaded`) activates the loaded IR instead.
pub(super) fn select_cab(params: &Params, index: usize, ir_loaded: bool) {
    if index < CabModel::ALL.len() {
        params.cab_model.store(CabModel::ALL[index] as u8, Relaxed);
        params.cab_external_active.store(false, Relaxed);
    } else if ir_loaded && index == CabModel::ALL.len() {
        params.cab_external_active.store(true, Relaxed);
    }
}

/// Modal list lengths: built-ins plus one external row when loaded.
pub(super) fn amp_choices(au_loaded: bool) -> usize {
    AmpModel::ALL.len() + usize::from(au_loaded)
}

/// Modal list lengths: built-ins plus one external row when loaded.
pub(super) fn cab_choices(ir_loaded: bool) -> usize {
    CabModel::ALL.len() + usize::from(ir_loaded)
}

/// Cursor the amp modal opens with: the external row while an AU is active,
/// else the current built-in model.
pub(super) fn init_amp_cursor(params: &Params) -> usize {
    if params.amp_external_active.load(Relaxed) && params.amp_external_loaded.load(Relaxed) {
        AmpModel::ALL.len()
    } else {
        let current = params.amp_model.load(Relaxed);
        AmpModel::ALL
            .iter()
            .position(|&m| m as u8 == current)
            .unwrap_or(0)
    }
}

/// Cursor the cab modal opens with: the external row while an IR is active,
/// else the current built-in model.
pub(super) fn init_cab_cursor(params: &Params) -> usize {
    if params.cab_external_active.load(Relaxed) && params.cab_external_loaded.load(Relaxed) {
        CabModel::ALL.len()
    } else {
        let current = params.cab_model.load(Relaxed);
        CabModel::ALL
            .iter()
            .position(|&m| m as u8 == current)
            .unwrap_or(0)
    }
}

pub(super) fn toggle_pedal(params: &Params, knob_idx: usize) {
    // Amp and mic sections have no on/off toggle, so `pedal_of` returns `None`.
    if let Some(p) = pedal_of(knob_idx) {
        let flag = (PEDALS[p].enabled)(params);
        flag.store(!flag.load(Relaxed), Relaxed);
    }
}

/// Put a pedal on the board and engage it (LED on).
pub(super) fn add_pedal(params: &Params, board: &mut [bool], pedal: usize) {
    board[pedal] = true;
    (PEDALS[pedal].enabled)(params).store(true, Relaxed);
}

/// Take a pedal off the board and bypass it in the DSP chain.
pub(super) fn remove_pedal(params: &Params, board: &mut [bool], pedal: usize) {
    board[pedal] = false;
    (PEDALS[pedal].enabled)(params).store(false, Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::{AmpModel, CabModel};

    fn board(on: bool) -> Vec<bool> {
        vec![on; PEDALS.len()]
    }

    fn order() -> [u8; CHAIN_LEN] {
        ChainStage::default_order()
    }

    fn knob(params: &Params, idx: usize) -> f32 {
        (KNOBS[idx].param)(params).load(Relaxed)
    }

    // ── navigation ────────────────────────────────────────────────────────────

    #[test]
    fn tab_walks_panels_in_number_order() {
        let b = board(false);
        let p = Panels::all_visible();
        let o = order();
        // Panel 1 (ribbon) → 2 (first amp knob) → 3 (timeline) → 4 (+ADD,
        // empty board) → back to 1.
        let mut f = Some(CHAIN_TILE);
        f = next_panel_focus(f, &b, &p, &o);
        assert_eq!(f, Some(AMP_START), "Tab from ribbon must land on GAIN");
        f = next_panel_focus(f, &b, &p, &o);
        assert_eq!(f, Some(PRACTICE_TILE));
        f = next_panel_focus(f, &b, &p, &o);
        assert_eq!(f, Some(ADD_TILE), "empty board enters at +ADD");
        f = next_panel_focus(f, &b, &p, &o);
        assert_eq!(f, Some(CHAIN_TILE), "Tab must wrap back to the ribbon");
    }

    #[test]
    fn shift_tab_is_the_inverse_of_tab() {
        let b = board(true);
        let p = Panels::all_visible();
        let o = order();
        let mut f = Some(CHAIN_TILE);
        let mut forward = vec![f];
        for _ in 0..6 {
            f = next_panel_focus(f, &b, &p, &o);
            forward.push(f);
        }
        for &expected in forward.iter().rev().skip(1) {
            f = prev_panel_focus(f, &b, &p, &o);
            assert_eq!(f, expected, "BackTab did not retrace Tab");
        }
    }

    #[test]
    fn tab_skips_hidden_panels() {
        let b = board(true);
        let o = order();
        let hidden = Panels {
            amp: false,
            rig: false,
            timeline: true,
        };
        // Only the ribbon and the timeline remain.
        assert_eq!(
            next_panel_focus(Some(CHAIN_TILE), &b, &hidden, &o),
            Some(PRACTICE_TILE)
        );
        assert_eq!(
            next_panel_focus(Some(PRACTICE_TILE), &b, &hidden, &o),
            Some(CHAIN_TILE)
        );
        // Hiding everything still leaves the ribbon.
        let none = Panels {
            amp: false,
            rig: false,
            timeline: false,
        };
        assert_eq!(
            next_panel_focus(Some(CHAIN_TILE), &b, &none, &o),
            Some(CHAIN_TILE)
        );
    }

    #[test]
    fn panel_entry_lands_on_first_onboard_pedal_in_chain_order() {
        let mut b = board(false);
        b[3] = true; // only COMP is on the board
        // Reversed chain: entry must still find COMP (order-independent lookup).
        let mut rev: Vec<u8> = ChainStage::default_order().into_iter().collect();
        rev.reverse();
        let rev: [u8; CHAIN_LEN] = rev.try_into().unwrap();
        assert_eq!(
            panel_entry(4, &b, &rev),
            Some(PEDALS[3].start),
            "panel 4 must enter on the only on-board pedal"
        );
        assert_eq!(
            panel_entry(4, &board(false), &rev),
            Some(ADD_TILE),
            "empty board enters at +ADD"
        );
    }

    #[test]
    fn arrows_stay_inside_their_panel() {
        let b = board(true);
        let o = order();
        // Panel 2: amp knobs → mic knobs, wrapping around (no selector stop).
        assert_eq!(
            step_knob_in_panel(Some(AMP_START), &b, &o, -1),
            Some(MIC_END - 1),
            "← from the first amp knob must wrap to the last mic knob"
        );
        assert_eq!(
            step_knob_in_panel(Some(MIC_END - 1), &b, &o, 1),
            Some(AMP_START),
            "→ from the last mic knob must wrap to the first amp knob"
        );
        // A stale selector focus re-anchors into the amp knobs.
        let stale = step_knob_in_panel(None, &b, &o, 1);
        assert!(
            matches!(stale, Some(k) if (AMP_START..MIC_END).contains(&k)),
            "→ from a stale selector focus must enter the amp knobs: {stale:?}"
        );
        // Panel 4 never leaks into the amp: from +ADD, → wraps within the board.
        assert_eq!(
            step_knob_in_panel(Some(ADD_TILE), &b, &o, 1),
            Some(PEDALS[0].start),
            "→ from +ADD must wrap to the first pedal in chain order"
        );
        // Ribbon and timeline own their arrows: focus untouched.
        assert_eq!(
            step_knob_in_panel(Some(CHAIN_TILE), &b, &o, 1),
            Some(CHAIN_TILE)
        );
        assert_eq!(
            step_knob_in_panel(Some(PRACTICE_TILE), &b, &o, -1),
            Some(PRACTICE_TILE)
        );
    }

    #[test]
    fn arrows_visit_pedal_knobs_in_chain_order() {
        let b = board(true);
        let mut rev: Vec<u8> = ChainStage::default_order().into_iter().collect();
        rev.reverse();
        let rev: [u8; CHAIN_LEN] = rev.try_into().unwrap();
        // Collect the full → walk starting at the first pedal knob.
        let first = PEDALS[17].start; // REVERB leads the reversed chain
        let mut seen = vec![first];
        let mut f = Some(first);
        for _ in 0..KNOBS.len() {
            f = step_knob_in_panel(f, &b, &rev, 1);
            if f == Some(ADD_TILE) {
                break;
            }
            seen.push(f.unwrap());
        }
        let want: Vec<usize> = rev
            .iter()
            .filter_map(|&v| ChainStage::from_u8(v))
            .filter_map(|s| s.pedal_index())
            .flat_map(|pi| PEDALS[pi].start..PEDALS[pi].end)
            .collect();
        assert_eq!(seen, want);
    }

    #[test]
    fn arrows_skip_knobs_of_off_board_pedals() {
        let b = board(false); // no pedal knobs are reachable
        let o = order();
        for k in PEDALS.iter().flat_map(|p| p.start..p.end) {
            // Stepping from a stale off-board knob re-anchors inside panel 4's
            // stops (+ADD), never onto another off-board knob.
            let f = step_knob_in_panel(Some(k), &b, &o, 1);
            assert!(
                f == Some(ADD_TILE),
                "off-board pedal knob leaked with ←/→: {f:?}"
            );
        }
        // Every amp/mic knob is still reachable.
        for k in AMP_START..MIC_END {
            assert!(
                panel_knob_stops(2, &b, &o).contains(&Some(k)),
                "amp/mic knob {k} is not reachable"
            );
        }
    }

    #[test]
    fn number_keys_focus_first_show_second_hide_third() {
        let b = board(true);
        let o = order();
        let p = Panels::all_visible();
        // Visible but unfocused panel 4 → focuses its entry (first pedal).
        let (p2, f) = press_number(4, None, &b, &p, &o);
        assert!(p2.rig);
        assert_eq!(f, Some(PEDALS[0].start));
        // Focused panel 4 → hides it and repairs focus onto the ribbon.
        let (p3, f) = press_number(4, f, &b, &p2, &o);
        assert!(!p3.rig);
        assert_eq!(f, Some(CHAIN_TILE));
        // Hidden panel 4 → shows and focuses it again.
        let (p4, f) = press_number(4, f, &b, &p3, &o);
        assert!(p4.rig);
        assert_eq!(f, Some(PEDALS[0].start));
        // Panel 1 only focuses, never hides.
        let (p5, f) = press_number(1, f, &b, &p4, &o);
        assert_eq!(f, Some(CHAIN_TILE));
        assert!(p5.rig && p5.amp && p5.timeline);
        // Out-of-range keys are ignored.
        let (p6, f) = press_number(9, f, &b, &p5, &o);
        assert!(p6.rig && p6.amp && p6.timeline);
        assert_eq!(f, Some(CHAIN_TILE));
    }

    #[test]
    fn hiding_the_focused_panel_repairs_focus() {
        let b = board(true);
        let o = order();
        // Focus on a pedal knob while the board hides → ribbon.
        let p = Panels {
            rig: false,
            ..Panels::all_visible()
        };
        assert_eq!(
            ensure_focus_visible(Some(PEDALS[0].start), &b, &p, &o),
            Some(CHAIN_TILE)
        );
        // Visible panels keep focus untouched, even mid-panel.
        assert_eq!(
            ensure_focus_visible(Some(PEDALS[0].start + 1), &b, &Panels::all_visible(), &o),
            Some(PEDALS[0].start + 1)
        );
    }

    #[test]
    fn hidden_panels_are_skipped_by_navigation() {
        let b = board(true);
        let o = order();
        // Hide the amp and the board: Tab bounces between ribbon and timeline.
        let hidden = Panels {
            amp: false,
            rig: false,
            timeline: true,
        };
        assert_eq!(
            next_panel_focus(Some(CHAIN_TILE), &b, &hidden, &o),
            Some(PRACTICE_TILE)
        );
        // Hiding everything still leaves the ribbon.
        let none = Panels {
            amp: false,
            rig: false,
            timeline: false,
        };
        assert_eq!(
            next_panel_focus(Some(CHAIN_TILE), &b, &none, &o),
            Some(CHAIN_TILE)
        );
    }

    // ── knob edits ──────────────────────────────────────────────────────────────

    #[test]
    fn nudge_clamps_to_the_unit_range() {
        let p = Params::new();
        nudge(&p, 0, 5.0);
        assert_eq!(knob(&p, 0), 1.0, "nudge above 1.0 must clamp");
        nudge(&p, 0, -5.0);
        assert_eq!(knob(&p, 0), 0.0, "nudge below 0.0 must clamp");
    }

    #[test]
    fn nudge_moves_only_the_targeted_knob() {
        let p = Params::new();
        let before: Vec<f32> = (0..KNOBS.len()).map(|k| knob(&p, k)).collect();
        nudge(&p, 2, 0.05);
        for (k, knob_before) in before.iter().enumerate().take(KNOBS.len()) {
            if k == 2 {
                assert!((knob(&p, k) - (knob_before + 0.05)).abs() < 1e-6);
            } else {
                assert_eq!(knob(&p, k), *knob_before, "knob {k} moved unexpectedly");
            }
        }
    }

    // ── amp / cab modal picks ─────────────────────────────────────────────────

    #[test]
    fn select_amp_stores_builtin_and_returns_to_builtin() {
        let p = Params::new();
        p.amp_external_active.store(true, Relaxed);
        p.amp_external_loaded.store(true, Relaxed);
        select_amp(&p, 3, true); // Vox
        assert_eq!(AmpModel::from_u8(p.amp_model.load(Relaxed)), AmpModel::Vox);
        assert!(
            !p.amp_external_active.load(Relaxed),
            "picking a built-in must return to the built-in amp"
        );
    }

    #[test]
    fn select_amp_external_row_activates_the_loaded_au() {
        let p = Params::new();
        p.amp_external_loaded.store(true, Relaxed);
        select_amp(&p, AmpModel::ALL.len(), true);
        assert!(p.amp_external_active.load(Relaxed));
        // Without a loaded AU the trailing index is a no-op.
        let q = Params::new();
        select_amp(&q, AmpModel::ALL.len(), false);
        assert!(!q.amp_external_active.load(Relaxed));
        // Garbage indices never touch anything.
        select_amp(&q, 99, true);
        assert_eq!(
            AmpModel::from_u8(q.amp_model.load(Relaxed)),
            AmpModel::Mesa,
            "default model must survive a garbage pick"
        );
    }

    #[test]
    fn select_cab_stores_builtin_and_returns_to_builtin() {
        let p = Params::new();
        p.cab_external_active.store(true, Relaxed);
        p.cab_external_loaded.store(true, Relaxed);
        select_cab(&p, 2, true); // Orange
        assert_eq!(
            CabModel::from_u8(p.cab_model.load(Relaxed)),
            CabModel::Orange
        );
        assert!(
            !p.cab_external_active.load(Relaxed),
            "picking a built-in must return to the built-in cab"
        );
    }

    #[test]
    fn select_cab_external_row_activates_the_loaded_ir() {
        let p = Params::new();
        p.cab_external_loaded.store(true, Relaxed);
        select_cab(&p, CabModel::ALL.len(), true);
        assert!(p.cab_external_active.load(Relaxed));
        let q = Params::new();
        select_cab(&q, CabModel::ALL.len(), false);
        assert!(!q.cab_external_active.load(Relaxed));
        select_cab(&q, 99, true);
        assert!(!q.cab_external_active.load(Relaxed));
    }

    #[test]
    fn modal_cursors_preselect_the_current_pick() {
        let p = Params::new();
        p.amp_model.store(AmpModel::Vox as u8, Relaxed);
        p.cab_model.store(CabModel::Wem as u8, Relaxed);
        assert_eq!(init_amp_cursor(&p), 3);
        assert_eq!(init_cab_cursor(&p), 3);
        assert_eq!(amp_choices(false), 5);
        assert_eq!(amp_choices(true), 6);
        assert_eq!(cab_choices(false), 4);
        assert_eq!(cab_choices(true), 5);
        // Active externals point at their trailing rows.
        p.amp_external_active.store(true, Relaxed);
        p.amp_external_loaded.store(true, Relaxed);
        p.cab_external_active.store(true, Relaxed);
        p.cab_external_loaded.store(true, Relaxed);
        assert_eq!(init_amp_cursor(&p), 5);
        assert_eq!(init_cab_cursor(&p), 4);
    }

    // ── board membership & toggles ──────────────────────────────────────────────

    #[test]
    fn add_then_remove_pedal_round_trips_board_and_enabled_flag() {
        let p = Params::new();
        let mut b = board(false);
        // Force the flag off first so the default-on pedals don't mask the test.
        (PEDALS[5].enabled)(&p).store(false, Relaxed);

        add_pedal(&p, &mut b, 5);
        assert!(b[5], "add_pedal must put the pedal on the board");
        assert!(
            (PEDALS[5].enabled)(&p).load(Relaxed),
            "add_pedal must engage the LED"
        );

        remove_pedal(&p, &mut b, 5);
        assert!(!b[5], "remove_pedal must take it off the board");
        assert!(
            !(PEDALS[5].enabled)(&p).load(Relaxed),
            "remove_pedal must bypass it"
        );
    }

    #[test]
    fn toggle_pedal_flips_the_enabled_flag_for_a_pedal_knob() {
        let p = Params::new();
        let flag = (PEDALS[0].enabled)(&p);
        let before = flag.load(Relaxed);
        toggle_pedal(&p, PEDALS[0].start);
        assert_eq!(flag.load(Relaxed), !before);
    }

    #[test]
    fn toggle_pedal_is_a_noop_for_amp_and_mic_knobs() {
        let p = Params::new();
        // Amp/mic knobs own no enable flag; toggling must not panic or flip anything.
        let flags_before: Vec<bool> = PEDALS
            .iter()
            .map(|pd| (pd.enabled)(&p).load(Relaxed))
            .collect();
        toggle_pedal(&p, AMP_START);
        toggle_pedal(&p, MIC_START);
        let flags_after: Vec<bool> = PEDALS
            .iter()
            .map(|pd| (pd.enabled)(&p).load(Relaxed))
            .collect();
        assert_eq!(flags_before, flags_after);
    }

    // ── ribbon cursor moves ───────────────────────────────────────────────────

    #[test]
    fn cursor_steps_through_rendered_stages_and_wraps() {
        let b = board(true);
        let o = order();
        // Full board: every slot renders, so the cursor walks raw order.
        assert_eq!(
            move_chain_cursor(&o, &b, ChainStage::Gate, 1),
            ChainStage::Whammy
        );
        assert_eq!(
            move_chain_cursor(&o, &b, ChainStage::Gate, -1),
            ChainStage::Reverb,
            "cursor must wrap around the ends"
        );
        // Sparse board: only on-board pedals + AmpCab render.
        let mut sparse = board(false);
        sparse[3] = true; // COMP only
        assert_eq!(
            move_chain_cursor(&o, &sparse, ChainStage::AmpCab, 1),
            ChainStage::Comp
        );
        assert_eq!(
            move_chain_cursor(&o, &sparse, ChainStage::Comp, 1),
            ChainStage::AmpCab,
            "cursor must wrap with two rendered stages"
        );
        // Stale cursor (pedal left the board) re-anchors at the nearest end.
        assert_eq!(
            move_chain_cursor(&o, &sparse, ChainStage::Fuzz, 1),
            ChainStage::Comp
        );
        assert_eq!(
            move_chain_cursor(&o, &sparse, ChainStage::Fuzz, -1),
            ChainStage::AmpCab
        );
    }

    #[test]
    fn move_selected_stage_swaps_with_rendered_neighbour() {
        let p = Params::new();
        let b = board(true);
        // COMP is at slot 3 with FUZZ right after it.
        assert!(move_selected_stage(&p, &b, ChainStage::Comp, 1));
        let order = p.chain_slots();
        assert_eq!(order[3], ChainStage::Fuzz as u8);
        assert_eq!(order[4], ChainStage::Comp as u8);
        // …and back again.
        assert!(move_selected_stage(&p, &b, ChainStage::Comp, -1));
        assert_eq!(p.chain_slots(), ChainStage::default_order());
    }

    #[test]
    fn move_selected_stage_moves_the_ampcab_block() {
        let p = Params::new();
        let b = board(true);
        // AmpCab sits at slot 10 with VIBE before it.
        assert!(move_selected_stage(&p, &b, ChainStage::AmpCab, -1));
        let order = p.chain_slots();
        assert_eq!(order[9], ChainStage::AmpCab as u8);
        assert_eq!(order[10], ChainStage::Vibe as u8);
        assert!(move_selected_stage(&p, &b, ChainStage::AmpCab, 1));
        assert_eq!(p.chain_slots(), ChainStage::default_order());
    }

    #[test]
    fn move_selected_stage_refuses_the_ends_and_stale_cursors() {
        let p = Params::new();
        let b = board(true);
        // GATE is first: nothing earlier. REVERB is last: nothing later.
        assert!(!move_selected_stage(&p, &b, ChainStage::Gate, -1));
        assert!(!move_selected_stage(&p, &b, ChainStage::Reverb, 1));
        assert_eq!(p.chain_slots(), ChainStage::default_order());
        // Stale cursor (off-board pedal) cannot move.
        assert!(!move_selected_stage(&p, &board(false), ChainStage::Fuzz, 1));
    }

    #[test]
    fn toggle_stage_flips_onboard_pedals_only() {
        let p = Params::new();
        let mut b = board(true);
        let flag = (PEDALS[3].enabled)(&p);
        let before = flag.load(Relaxed);
        assert!(toggle_stage(&p, &b, ChainStage::Comp));
        assert_eq!(flag.load(Relaxed), !before);
        // AmpCab is a no-op, and so is an off-board pedal.
        assert!(!toggle_stage(&p, &b, ChainStage::AmpCab));
        b[3] = false;
        assert!(!toggle_stage(&p, &b, ChainStage::Comp));
        assert_eq!(flag.load(Relaxed), !before);
    }
}
