//! Owns editing-tool identities, scoped recipes and application presets separately from rendered images.

mod actions;
pub mod advanced_color;
mod before;
pub mod dehaze;
pub mod direct_color;
mod menus;
mod recipes;
mod sections;

pub use before::{Before, Rendition};
pub use menus::{Action, Menus, State};
pub use recipes::{storage, Library, Scope};
pub use sections::{move_section, normalize as normalize_sections, Section};
