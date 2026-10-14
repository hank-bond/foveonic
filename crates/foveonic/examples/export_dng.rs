//! Exports one Merrill original with explicit WB/mode for DNG reader validation, without selecting sidecars.

use foveonic::{
    dng,
    profile::Profile,
    render::{Engine, Settings},
    source::Result,
};
fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 4 {
        return Err("usage: export_dng INPUT.x3f OUTPUT.dng WB MODE (no sidecar selection)".into());
    }
    let settings = Settings::new(&args[2], &args[3]);
    dng::write_file(
        &Engine::new()?,
        std::path::Path::new(&args[0]),
        &settings,
        std::path::Path::new(&args[1]),
        &mut Profile::default(),
    )
}
