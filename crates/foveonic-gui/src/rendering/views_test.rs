//! Checks which photos the app plans, without a GPU.

use super::*;

#[test]
fn a_shown_photo_the_editing_owner_evicted_stays_displayed_for_the_loupe() {
    let edited = PathBuf::from("edited.x3f");
    let evicted = PathBuf::from("evicted.x3f");
    let photos = displayed(
        std::slice::from_ref(&edited),
        vec![evicted.clone(), edited.clone()],
        Some(evicted.clone()),
    );
    assert_eq!(photos, [edited, evicted]);
}
