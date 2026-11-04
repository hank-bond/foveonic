//! Arranges supported editing tools without changing their processing or saved image settings.
use crate::preferences::EditingSection;
use eframe::egui;

struct Placement {
    section: EditingSection,
    body: egui::Rect,
    divider: egui::Rect,
}

/// Hidden tools retain their order, but a drop must change the visible layout.
fn reordered(
    order: &[EditingSection],
    available: &[EditingSection],
    source: EditingSection,
    target: EditingSection,
    after: bool,
) -> Option<Vec<EditingSection>> {
    let mut next = order.to_vec();
    crate::editor::move_section(&mut next, source, target, after);
    let visible = |s: &&EditingSection| available.contains(s);
    if order.iter().filter(visible).eq(next.iter().filter(visible)) {
        None
    } else {
        Some(next)
    }
}

pub(super) fn show(
    ui: &mut egui::Ui,
    order: &mut Vec<EditingSection>,
    available: &[EditingSection],
    menus: &mut crate::editor::Menus<'_>,
    mut draw: impl FnMut(&mut egui::Ui, EditingSection) -> bool,
) -> bool {
    let mut changed = false;
    let mut placements = Vec::new();
    let spacing = ui.spacing().item_spacing.y;
    let gap = crate::widgets::section_spacing(ui);
    for section in order.iter().copied().filter(|s| available.contains(s)) {
        ui.spacing_mut().item_spacing.y = gap;
        let divider = ui.add(egui::Separator::default().spacing(0.)).rect;
        let response = ui.push_id(section, |ui| {
            ui.spacing_mut().item_spacing.y = spacing;
            draw(ui, section)
        });
        ui.spacing_mut().item_spacing.y = spacing;
        menus.header(
            ui,
            crate::editor::Scope::Tool(section),
            response.response.rect,
        );
        changed |= response.inner;
        placements.push(Placement {
            section,
            body: response.response.rect,
            divider,
        });
    }
    if let (Some(source), Some(pointer), Some(first), Some(last)) = (
        egui::DragAndDrop::payload::<EditingSection>(ui.ctx()),
        ui.input(|i| i.pointer.hover_pos()),
        placements.first(),
        placements.last(),
    ) {
        let zone = placements
            .iter()
            .fold(first.divider, |rect, p| rect.union(p.body).union(p.divider));
        if zone.contains(pointer) && available.contains(&source) {
            let slot = placements
                .iter()
                .position(|p| pointer.y < p.body.center().y);
            let (target, after, y) = match slot {
                Some(index) => (
                    placements[index].section,
                    false,
                    placements[index].divider.center().y,
                ),
                None => (last.section, true, last.body.bottom()),
            };
            if let Some(next) = reordered(order, available, *source, target, after) {
                if crate::sections::drop_editing(ui, zone, y).is_some() {
                    *order = next;
                    ui.ctx().request_repaint();
                }
            }
        }
    }
    changed
}

#[cfg(test)]
#[path = "sections_test.rs"]
mod tests;
