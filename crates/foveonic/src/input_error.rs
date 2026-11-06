//! Adds actionable file-read messages while retaining typed damage evidence and technical causes.

use crate::source::Result;
use std::{
    error::Error,
    fmt,
    path::{Path, PathBuf},
};

type Cause = Box<dyn Error + Send + Sync>;

#[derive(Debug)]
struct Damaged(&'static str);
impl fmt::Display for Damaged {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl Error for Damaged {}

/// Mark invalid on-disk structures where they are detected, without classifying unsupported formats as damage.
pub(crate) fn damaged(detail: &'static str) -> Cause {
    Box::new(Damaged(detail))
}

#[derive(Debug)]
struct InputError {
    path: PathBuf,
    cause: Cause,
}
impl fmt::Display for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = self
            .path
            .file_name()
            .unwrap_or(self.path.as_os_str())
            .to_string_lossy();
        if is_damaged(self.cause.as_ref()) {
            write!(f, "“{name}” appears damaged or incomplete. Try downloading it again or copying it from the original card. Technical detail: {}", self.cause)
        } else if let Some(io) = self.cause.downcast_ref::<std::io::Error>() {
            match io.kind() {
                std::io::ErrorKind::NotFound => write!(
                    f,
                    "“{name}” could not be found. Check that the file or drive is still available."
                ),
                std::io::ErrorKind::PermissionDenied => {
                    write!(f, "“{name}” could not be read. Check the file permissions.")
                }
                _ => write!(f, "Could not read “{name}”: {}", self.cause),
            }
        } else {
            write!(f, "Could not read “{name}”: {}", self.cause)
        }
    }
}
impl Error for InputError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.cause.as_ref())
    }
}
fn is_damaged(mut error: &(dyn Error + 'static)) -> bool {
    loop {
        if error.is::<Damaged>()
            || error.is::<std::str::Utf8Error>()
            || error.is::<std::string::FromUtf16Error>()
            || error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|e| e.kind() == std::io::ErrorKind::UnexpectedEof)
            || matches!(
                error.downcast_ref::<image::ImageError>(),
                Some(image::ImageError::Decoding(_))
            )
        {
            return true;
        }
        let Some(source) = error.source() else {
            return false;
        };
        error = source;
    }
}
fn context(path: &Path, error: Cause) -> Cause {
    if error.is::<crate::background::Interrupted>() {
        error
    } else {
        Box::new(InputError {
            path: path.to_owned(),
            cause: error,
        })
    }
}
pub(crate) fn read<T>(path: &Path, operation: impl FnOnce() -> Result<T>) -> Result<T> {
    operation().map_err(|error| context(path, error))
}

#[cfg(test)]
#[path = "input_error_test.rs"]
mod tests;
