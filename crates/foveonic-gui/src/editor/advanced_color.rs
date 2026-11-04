//! Stores transient color-tab selection and range-name editing apart from saved adjustments.

use eframe::egui;

#[derive(Clone, Default)]
pub struct State {
    pub advanced: bool,
    pub selected: usize,
    pub rename: Option<Rename>,
    photo: Option<std::path::PathBuf>,
}
#[derive(Clone)]
pub struct Rename {
    pub index: usize,
    pub text: String,
    pub focus: bool,
}

pub fn sync_photo(ctx: &egui::Context, photo: Option<&std::path::Path>) {
    let mut s = state(ctx);
    if s.photo.as_deref() != photo {
        s.photo = photo.map(ToOwned::to_owned);
        s.rename = None;
        store(ctx, s);
    }
}

pub fn cancel_rename(ctx: &egui::Context) {
    let mut s = state(ctx);
    s.rename = None;
    store(ctx, s);
}

fn id() -> egui::Id {
    egui::Id::new("Advanced Color Range tool")
}
pub fn state(ctx: &egui::Context) -> State {
    ctx.data_mut(|d| d.get_temp(id())).unwrap_or_default()
}
pub fn store(ctx: &egui::Context, state: State) {
    ctx.data_mut(|d| d.insert_temp(id(), state));
}
pub fn select(ctx: &egui::Context, selected: usize) {
    let mut state = state(ctx);
    state.selected = selected;
    state.rename = None;
    state.advanced = true;
    store(ctx, state);
}
