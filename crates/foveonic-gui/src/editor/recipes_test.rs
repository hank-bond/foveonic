//! Checks named-preset replacement, portable scopes and transient settings exclusion.

use super::*;
use foveonic::render::PreviewQuality;

#[test]
fn library_replaces_named_scopes_and_shares_universal_tools_only() {
    let mut library = Library::default();
    let mut source = Settings {
        exposure: 1.,
        preview_quality: PreviewQuality::Draft,
        ..Default::default()
    };
    library
        .save(
            ProcessingFamily::Merrill,
            Scope::Tool(Section::Tone),
            "Portrait",
            &source,
        )
        .unwrap();
    source.exposure = 0.5;
    library
        .save(
            ProcessingFamily::Merrill,
            Scope::Tool(Section::Tone),
            " Portrait ",
            &source,
        )
        .unwrap();
    assert_eq!(library.presets.len(), 1);
    assert_eq!(
        library
            .choices(ProcessingFamily::Quattro, Scope::Tool(Section::Tone))
            .count(),
        1
    );
    assert_eq!(
        library.presets[0].settings.preview_quality,
        PreviewQuality::Full
    );
    library
        .save(ProcessingFamily::Merrill, Scope::All, "All", &source)
        .unwrap();
    assert_eq!(
        library
            .choices(ProcessingFamily::Quattro, Scope::All)
            .count(),
        0
    );
    assert!(library
        .save(ProcessingFamily::Merrill, Scope::All, " ", &source)
        .is_err());
    let stored = serde_json::to_vec(&library).unwrap();
    assert_eq!(serde_json::from_slice::<Library>(&stored).unwrap(), library);
}
