//! Maintains visual editing-tool order using processing-owned photographic section identities.

pub use foveonic::render::EditSection as Section;
use std::collections::BTreeSet;

pub fn normalize(order: &mut Vec<Section>) {
    if !order.contains(&Section::WhiteBalance) {
        order.insert(0, Section::WhiteBalance);
    }
    let mut seen = BTreeSet::new();
    order.retain(|section| seen.insert(*section));
    for section in Section::ALL {
        if seen.insert(section) {
            order.push(section);
        }
    }
}
pub fn move_section(order: &mut Vec<Section>, source: Section, target: Section, after: bool) {
    if source == target {
        return;
    }
    if !order.contains(&target) {
        return;
    }
    order.retain(|s| *s != source);
    let index = order.iter().position(|s| *s == target).unwrap() + usize::from(after);
    order.insert(index, source);
}
