//! Checks shared damage reporting across original, preview and capture readers.

use super::*;

#[test]
fn damage_messages_keep_recovery_advice_and_the_original_cause() {
    let path = Path::new("photos/damaged.x3f");
    let error = context(path, damaged("invalid Huffman prefix"));
    let text = error.to_string();
    assert!(text.contains("damaged.x3f” appears damaged or incomplete"));
    assert!(text.contains("downloading it again"));
    assert!(text.contains("invalid Huffman prefix"));
    assert_eq!(
        error.source().unwrap().to_string(),
        "invalid Huffman prefix"
    );
}

#[test]
fn unsupported_formats_io_errors_and_cancellation_are_not_reported_as_damage() {
    let path = Path::new("photo.x3f");
    for error in [
        "unsupported sensor compression".into(),
        std::io::Error::from(std::io::ErrorKind::NotFound).into(),
        std::io::Error::from(std::io::ErrorKind::PermissionDenied).into(),
    ] {
        assert!(!context(path, error).to_string().contains("damaged"));
    }
    let error = context(path, Box::new(crate::background::Interrupted));
    assert!(error.is::<crate::background::Interrupted>());
    let error = context(
        path,
        std::io::Error::from(std::io::ErrorKind::UnexpectedEof).into(),
    );
    assert!(error.to_string().contains("damaged or incomplete"));
}

#[test]
fn all_original_readers_report_truncation_and_distinguish_unknown_versions() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("damaged.x3f");
    let readers: [fn(&Path) -> Result<()>; 4] = [
        |p| crate::source::Source::open(p).map(|_| ()),
        |p| crate::preview::read(p).map(|_| ()),
        |p| crate::preview::read_full(p).map(|_| ()),
        |p| crate::source::read_capture_properties(p).map(|_| ()),
    ];
    for data in [
        b"FOV".to_vec(),
        {
            let mut d = vec![0; 44];
            d[..4].copy_from_slice(b"FOVb");
            d[4..8].copy_from_slice(&0x90000u32.to_le_bytes());
            d
        },
        {
            let mut d = vec![0; 44];
            d[..4].copy_from_slice(b"FOVb");
            d[4..8].copy_from_slice(&0x30000u32.to_le_bytes());
            d[40..].copy_from_slice(&u32::MAX.to_le_bytes());
            d
        },
    ] {
        std::fs::write(&path, &data).unwrap();
        let unsupported = data.len() == 44 && data[6] == 9;
        for reader in readers {
            let error = reader(&path).unwrap_err();
            let text = error.to_string();
            assert!(
                text.contains(path.file_name().unwrap().to_str().unwrap()),
                "{text}"
            );
            assert_eq!(
                text.contains("damaged or incomplete"),
                !unsupported,
                "{text}"
            );
        }
    }
}
