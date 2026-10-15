//! Embeds checked-in photographic help assets and rejects missing rendered images during offline builds.

mod build_support;

fn main() {
    let root = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let generated = build_support::generate(&root.join("assets/help"), |path| {
        println!("cargo:rerun-if-changed={}", path.display());
    })
    .unwrap_or_else(|error| panic!("Photographic help: {error}"));
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    std::fs::write(output.join("help_comparisons.rs"), generated).unwrap();
}
