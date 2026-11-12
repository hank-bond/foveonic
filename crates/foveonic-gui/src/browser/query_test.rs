//! Checks photographic sort values, missing metadata, folder grouping and combined browser filters.

use super::*;

#[test]
fn sorting_is_numeric_stable_and_missing_last_in_both_directions() {
    let mut files = ["one/a.x3f", "one/b.x3f", "one/c.x3f", "two/a.x3f"].map(PathBuf::from);
    let entries = HashMap::from([
        (
            files[0].clone(),
            Entry::new(
                BTreeMap::from([("ISO".into(), "1000".into())]),
                None,
                Rating::default(),
            ),
        ),
        (
            files[1].clone(),
            Entry::new(
                BTreeMap::from([("ISO".into(), "200".into())]),
                None,
                Rating::default(),
            ),
        ),
    ]);
    sort(
        &mut files,
        &entries,
        Sort {
            field: Field::Iso,
            reverse: false,
        },
    );
    assert_eq!(
        files
            .iter()
            .map(|p| p.to_string_lossy())
            .collect::<Vec<_>>(),
        ["one/b.x3f", "one/a.x3f", "one/c.x3f", "two/a.x3f"]
    );
    sort(
        &mut files,
        &entries,
        Sort {
            field: Field::Iso,
            reverse: true,
        },
    );
    assert_eq!(
        files
            .iter()
            .map(|p| p.to_string_lossy())
            .collect::<Vec<_>>(),
        ["one/a.x3f", "one/b.x3f", "one/c.x3f", "two/a.x3f"]
    );
}

#[test]
fn substring_search_combines_names_relative_paths_and_ratings_without_hidden_metadata_matches() {
    let entry = Entry::new(
        BTreeMap::from([("CAMMODEL".into(), "SIGMA DP3 Merrill".into())]),
        None,
        Rating::try_from(4).unwrap(),
    );
    let path = Path::new("card/DCIM/one.X3F");
    assert!(!Filter {
        text: "Merrill".into(),
        rating: RatingFilter::All
    }
    .matches(path, Some(Path::new("card")), Some(&entry)));
    let serial_match = Entry::new(
        BTreeMap::from([("CAMSERIAL".into(), "123899".into())]),
        None,
        Rating::default(),
    );
    let query = Filter {
        text: "238".into(),
        rating: RatingFilter::All,
    };
    assert!(!query.matches(
        Path::new("card238/DCIM/_P1M0236.X3F"),
        Some(Path::new("card238")),
        Some(&serial_match)
    ));
    assert!(query.matches(
        Path::new("card/DCIM/_P1M0238.X3F"),
        Some(Path::new("card")),
        Some(&serial_match)
    ));
    assert!(query.matches(
        Path::new("card/DCIM/238/_P1M0236.X3F"),
        Some(Path::new("card")),
        Some(&serial_match)
    ));
    for text in ["dCiM", "one.x3f"] {
        assert!(Filter {
            text: text.into(),
            rating: RatingFilter::AtLeast(3)
        }
        .matches(path, Some(Path::new("card")), Some(&entry)));
    }
    assert!(!Filter {
        text: String::new(),
        rating: RatingFilter::AtLeast(5)
    }
    .matches(path, None, Some(&entry)));
    assert!(!Filter {
        text: String::new(),
        rating: RatingFilter::Unrated
    }
    .matches(path, None, Some(&entry)));
    assert!(!Filter {
        text: "no match".into(),
        rating: RatingFilter::All
    }
    .matches(path, None, Some(&entry)));
}
