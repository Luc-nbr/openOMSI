//! The contracts the company can take on, and those it has.

use super::super::theme::*;
use super::super::Launcher;
use super::kit;
use omsi_ui::paint::Align;
use omsi_ui::{Rect, Weight};

pub fn draw(l: &mut Launcher, area: Rect) {
    l.ui.text_in("Contracts", Rect::new(area.x, area.y, area.w, 30.0), kit::BODY, Weight::Medium, TEXT_DIM, Align::Left);
}
