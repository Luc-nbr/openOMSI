//! RosterFlow's scrollbars (Optibus's): a slim rounded thumb on a faint track, the thumb
//! brighter under the mouse and while it is dragged. The thumb is as long as the part shown;
//! it is dragged, and a click on the track beside it moves a page that way. The Vehicles and
//! Drivers Gantt has one along the day (once zoomed in) and one down the rows, the timeplan
//! one along its stops, the Roster tab's lists one each (`area`).

use super::super::super::ui::{id_of, BarLook, Ui};
use super::rosterflow::Pal;
use omsi_ui::{Color, Rect};

/// A bar's thickness, the track's room included.
pub(super) const BAR: f32 = 12.0;

/// A bar's colours: the track, the thumb, the thumb under the mouse or held.
#[derive(Clone, Copy)]
pub(super) struct Look {
    pub track: Color,
    pub thumb: Color,
    pub hot: Color,
}

/// RosterFlow's bar in its palette (dark or light).
pub(super) fn look(p: &Pal) -> Look {
    if p.light {
        Look { track: p.text.alpha(0.06), thumb: p.text.alpha(0.24), hot: p.text.alpha(0.44) }
    } else {
        Look { track: Color::WHITE.alpha(0.05), thumb: Color::WHITE.alpha(0.22), hot: Color::WHITE.alpha(0.42) }
    }
}

/// A scrollbar in `track` for a view of `view` out of `total`, scrolled to `pos` (0 to
/// `total - view`). Draws nothing and returns None when everything is in view; else returns
/// where it was dragged or paged to.
#[allow(clippy::too_many_arguments)]
pub(super) fn bar(ui: &mut Ui, look: &Look, name: &str, track: Rect, vertical: bool, pos: f32, view: f32, total: f32) -> Option<f32> {
    // (the launcher's own bar, in RosterFlow's colours)
    ui.scrollbar(BarLook { track: look.track, thumb: look.thumb, hot: look.hot }, name, track, vertical, pos, view, total)
}

/// A list that scrolls in `r` with RosterFlow's bar on its right (the launcher's
/// `scroll_area` as RosterFlow draws it: `scroll_to` finds it by the same name). `body`
/// draws the rows given the room moved by the scroll and returns their height.
pub(super) fn area(ui: &mut Ui, look: &Look, name: &str, r: Rect, body: &mut dyn FnMut(&mut Ui, Rect) -> f32) {
    let id = id_of(name);
    let off = ui.scroll.get(&id).copied().unwrap_or(0.0);
    ui.push_clip(r, 0.0);
    let content = body(ui, Rect::new(r.x, r.y - off, r.w, r.h));
    ui.pop_clip();
    let max = (content - r.h).max(0.0);
    let mut target = ui.scroll.get(&(id ^ 0xabc)).copied().unwrap_or(off);
    if max > 0.0 && ui.hover(r) && !ui.over_popup() && ui.input.wheel.y.abs() > 0.0 && !ui.input.ctrl {
        target -= ui.input.wheel.y * 42.0;
        ui.input.wheel.y = 0.0;
    }
    let track = Rect::new(r.right() - BAR, r.y, BAR, r.h);
    let mut now_too = false;
    if let Some(to) = bar(ui, look, name, track, true, off, r.h, content) {
        target = to;
        now_too = true;
    }
    target = target.clamp(0.0, max);
    ui.scroll.insert(id ^ 0xabc, target);
    let k = 1.0 - (-ui.dt / 0.07).exp();
    let now = if now_too { target } else { off + (target - off) * k };
    ui.scroll.insert(id, if (now - target).abs() < 0.3 { target } else { now });
}
