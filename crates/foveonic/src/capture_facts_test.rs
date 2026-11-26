//! Checks capture fact resolution: the camera's embedded EXIF first, then X3F properties.

use super::*;

fn properties(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[test]
fn properties_resolve_with_named_lenses_and_camera_shutter_descriptions() {
    let facts = CaptureFacts::from_properties(&properties(&[
        ("CAMMODEL", "SIGMA DP2 Merrill"),
        ("LENSMODEL", "1003"),
        ("LENSFRANGE", "30"),
        ("LENSARANGE", "2.8 to 16"),
        ("SH_DESC", "1/60"),
        ("SHUTTER", "0.01563"),
        ("AP_DESC", "f/2.8"),
        ("APERTURE", "2.83"),
        ("ISO", "200"),
        ("FLENGTH", "30"),
        ("PMODE", "A"),
        ("AEMODE", "8"),
        ("TIME", "1790791918"),
    ]));
    assert_eq!(facts.make, "SIGMA");
    assert_eq!(facts.model.as_deref(), Some("SIGMA DP2 Merrill"));
    assert_eq!(facts.lens.as_deref(), Some("30mm F2.8"));
    assert_eq!(facts.exposure_seconds, Some(1. / 60.));
    assert_eq!(facts.f_number, Some(2.8));
    assert_eq!(facts.iso, Some(200));
    assert_eq!(facts.focal_length, Some(30.));
    assert_eq!((facts.exposure_program, facts.metering), (Some(3), Some(5)));
    assert_eq!(facts.taken.as_deref(), Some("2026:09:30 18:11:58"));
    let lens = |pairs: &[(&str, &str)]| CaptureFacts::from_properties(&properties(pairs)).lens;
    assert_eq!(
        lens(&[("LENSMODEL", "1005")]),
        None,
        "an ID alone names nothing"
    );
    assert_eq!(
        lens(&[("LENSFRANGE", "16.6"), ("LENSARANGE", "4.0")]).as_deref(),
        Some("16.6mm F4")
    );
    assert_eq!(lens(&[("LENSFRANGE", "30")]).as_deref(), Some("30mm"));
    let microseconds = CaptureFacts::from_properties(&properties(&[("EXPTIME", "20000")]));
    assert_eq!(microseconds.exposure_seconds, Some(0.02));
}

#[test]
fn a_zoom_keeps_one_name_and_its_widest_aperture_only_at_the_end_it_was_shot() {
    let zoom = |used: &str| {
        CaptureFacts::from_properties(&properties(&[
            ("LENSFRANGE", "50 to 18"),
            ("LENSARANGE", "22 to 2.8"),
            ("FLENGTH", used),
        ]))
    };
    for (used, widest) in [
        ("18", [Some(2.8), None]),
        ("35", [None, None]),
        ("50", [None, Some(2.8)]),
    ] {
        let facts = zoom(used);
        assert_eq!(facts.lens.as_deref(), Some("18-50mm"), "at {used}mm");
        assert_eq!(
            facts.lens_range,
            Some(LensRange {
                focal: [18., 50.],
                widest
            }),
            "at {used}mm"
        );
    }
}

#[test]
fn a_bracketed_frame_records_its_own_bias_and_an_unknown_offset_records_none() {
    let bias =
        |pairs: &[(&str, &str)]| CaptureFacts::from_properties(&properties(pairs)).exposure_bias;
    assert_eq!(bias(&[("EXPCOMP", "+0.30")]), Some(0.3));
    assert_eq!(
        bias(&[
            ("EXPCOMP", "+0.00"),
            ("EXPNET", "+1.00"),
            ("BRACKET", "3 of 3")
        ]),
        Some(1.)
    );
    assert_eq!(bias(&[("EXPCOMP", "+0.00"), ("BRACKET", "3 of 3")]), None);
}

#[test]
fn the_cameras_embedded_exif_wins_over_x3f_properties() {
    let rational = |n: u32, d: u32| [n.to_ne_bytes(), d.to_ne_bytes()].concat();
    let embedded = [
        EmbeddedTag {
            ifd: 1,
            id: F_NUMBER,
            kind: 5,
            count: 1,
            data: rational(56, 10),
        },
        EmbeddedTag {
            ifd: 1,
            id: ISO_SPEED_RATINGS,
            kind: 3,
            count: 1,
            data: 400_u16.to_ne_bytes().to_vec(),
        },
        EmbeddedTag {
            ifd: 0,
            id: MODEL,
            kind: 2,
            count: 18,
            data: b"SIGMA dp2 Quattro\0".to_vec(),
        },
    ];
    let facts = CaptureFacts::resolve(
        &properties(&[
            ("AP_DESC", "2.8"),
            ("ISO", "100"),
            ("CAMMODEL", "DP2 Quattro"),
        ]),
        &embedded,
    );
    assert_eq!(facts.f_number, Some(5.6));
    assert_eq!(facts.iso, Some(400));
    assert_eq!(facts.model.as_deref(), Some("SIGMA dp2 Quattro"));
}
