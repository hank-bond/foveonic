//! Defines browser metadata, stable folder-local sorting and shared substring/rating filtering.

use foveonic::review::Rating;
use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Field {
    #[default]
    Name,
    Date,
    Rating,
    Camera,
    Lens,
    Iso,
    FocalLength,
    Width,
    Height,
    FileSize,
    Aperture,
    Shutter,
}
impl Field {
    pub const ALL: [Self; 12] = [
        Self::Name,
        Self::Date,
        Self::Rating,
        Self::Camera,
        Self::Lens,
        Self::Iso,
        Self::FocalLength,
        Self::Width,
        Self::Height,
        Self::FileSize,
        Self::Aperture,
        Self::Shutter,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::Date => "Date",
            Self::Rating => "Rating",
            Self::Camera => "Camera",
            Self::Lens => "Lens",
            Self::Iso => "ISO",
            Self::FocalLength => "Focal length",
            Self::Width => "Width",
            Self::Height => "Height",
            Self::FileSize => "File size",
            Self::Aperture => "Aperture",
            Self::Shutter => "Shutter speed",
        }
    }
    fn property(self) -> Option<&'static str> {
        match self {
            Self::Date => Some("TIME"),
            Self::Camera => Some("CAMMODEL"),
            Self::Lens => Some("LENSMODEL"),
            Self::Iso => Some("ISO"),
            Self::FocalLength => Some("FLENGTH"),
            Self::Aperture => Some("APERTURE"),
            Self::Shutter => Some("SHUTTER"),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Sort {
    pub field: Field,
    pub reverse: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Entry {
    pub properties: BTreeMap<String, String>,
    pub bytes: Option<u64>,
    pub extent: Option<[u32; 2]>,
    pub rating: Rating,
}
impl Entry {
    pub fn new(properties: BTreeMap<String, String>, bytes: Option<u64>, rating: Rating) -> Self {
        Self {
            properties,
            bytes,
            rating,
            extent: None,
        }
    }
    fn number(&self, field: Field) -> Option<f64> {
        let value = match field {
            Field::Rating => f64::from(self.rating.stars()),
            Field::Width => f64::from(self.extent?[0]),
            Field::Height => f64::from(self.extent?[1]),
            Field::FileSize => self.bytes? as f64,
            _ => self
                .properties
                .get(field.property()?)?
                .trim()
                .parse()
                .ok()?,
        };
        value.is_finite().then_some(value)
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RatingFilter {
    #[default]
    All,
    Unrated,
    AtLeast(u8),
}
#[derive(Default)]
pub struct Filter {
    pub text: String,
    pub rating: RatingFilter,
}
impl Filter {
    pub fn matches(&self, path: &Path, root: Option<&Path>, entry: Option<&Entry>) -> bool {
        let rating = entry.map_or(0, |entry| entry.rating.stars());
        let rated = match self.rating {
            RatingFilter::All => true,
            RatingFilter::Unrated => rating == 0,
            RatingFilter::AtLeast(min) => rating >= min,
        };
        let query = self.text.trim().to_lowercase();
        rated
            && (query.is_empty()
                || root
                    .and_then(|r| path.strip_prefix(r).ok())
                    .unwrap_or(path)
                    .to_string_lossy()
                    .to_lowercase()
                    .contains(&query))
    }
}

fn optional<T>(
    a: Option<T>,
    b: Option<T>,
    reverse: bool,
    compare: impl FnOnce(T, T) -> Ordering,
) -> Ordering {
    match (a, b) {
        (Some(a), Some(b)) => {
            let order = compare(a, b);
            if reverse {
                order.reverse()
            } else {
                order
            }
        }
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}
pub fn sort(files: &mut [PathBuf], entries: &HashMap<PathBuf, Entry>, sort: Sort) {
    let mut folders = HashMap::new();
    for path in files.iter() {
        let count = folders.len();
        folders
            .entry(path.parent().unwrap_or(Path::new("")).to_owned())
            .or_insert(count);
    }
    files.sort_by(|a, b| {
        let folder = folders[a.parent().unwrap_or(Path::new(""))]
            .cmp(&folders[b.parent().unwrap_or(Path::new(""))]);
        if folder != Ordering::Equal {
            return folder;
        }
        let (left, right) = (entries.get(a), entries.get(b));
        let field = sort.field;
        let order = if field == Field::Name {
            optional(
                Some(
                    a.file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_lowercase(),
                ),
                Some(
                    b.file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_lowercase(),
                ),
                sort.reverse,
                |a, b| a.cmp(&b),
            )
        } else if matches!(field, Field::Camera | Field::Lens) {
            let text = |entry: Option<&Entry>| {
                entry
                    .and_then(|e| e.properties.get(field.property().unwrap()))
                    .map(|s| s.trim().to_lowercase())
                    .filter(|s| !s.is_empty())
            };
            optional(text(left), text(right), sort.reverse, |a, b| a.cmp(&b))
        } else {
            optional(
                left.and_then(|e| e.number(field)),
                right.and_then(|e| e.number(field)),
                sort.reverse,
                |a, b| a.total_cmp(&b),
            )
        };
        order.then_with(|| a.cmp(b))
    });
}

#[cfg(test)]
#[path = "browser_query_test.rs"]
mod tests;
