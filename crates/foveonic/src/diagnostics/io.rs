//! Observes actual input bytes through ordinary readers without additional file access.

use super::{capture_id, event_in_capture, scope_with, Category, Scope};
use std::{io, path::Path, time::Instant};

struct Observation {
    capture: u64,
    source: String,
    storage: &'static str,
    bytes: u64,
    calls: u64,
    errors: u64,
    read_ns: u64,
    complete: bool,
}

/// Counts only bytes returned by the wrapped reader. Metadata checks and cache
/// badges cannot accidentally report a source payload read through this adapter.
pub(crate) struct ObservedReader<R> {
    reader: R,
    observation: Option<Observation>,
    scope: Option<Scope>,
}

impl<R> ObservedReader<R> {
    pub fn new(reader: R, storage: &'static str, source: &Path) -> Self {
        let observation = capture_id().map(|capture| Observation {
            capture,
            source: source.to_string_lossy().into_owned(),
            storage,
            bytes: 0,
            calls: 0,
            errors: 0,
            read_ns: 0,
            complete: false,
        });
        let scope = scope_with(Category::Io, "source.read", |fields| {
            fields.str("storage", storage);
            fields.str("source", &source.to_string_lossy());
        });
        Self {
            reader,
            observation,
            scope: Some(scope),
        }
    }

    pub fn complete(&mut self) {
        self.finish(true);
    }

    pub fn get_ref(&self) -> &R {
        &self.reader
    }

    fn finish(&mut self, complete: bool) {
        if let Some(mut observation) = self.observation.take() {
            observation.complete = complete;
            event_in_capture(
                observation.capture,
                Category::Io,
                "source.read.complete",
                |fields| {
                    fields.str("source", &observation.source);
                    fields.str("storage", observation.storage);
                    fields.u64("bytes", observation.bytes);
                    fields.u64("read_operations", observation.calls);
                    fields.u64("read_errors", observation.errors);
                    fields.u64("read_time_ns", observation.read_ns);
                    fields.bool("complete", observation.complete);
                },
            );
        }
        self.scope.take();
    }
}

impl<R: io::Read> io::Read for ObservedReader<R> {
    fn read_to_end(&mut self, output: &mut Vec<u8>) -> io::Result<usize> {
        // Preserve File's bulk-read allocation strategy. Counting through the
        // default Read implementation would replace it with generic chunking.
        let before = output.len();
        let began = self.observation.as_ref().map(|_| Instant::now());
        let result = self.reader.read_to_end(output);
        if let Some(observation) = &mut self.observation {
            observation.read_ns = observation
                .read_ns
                .saturating_add(began.unwrap().elapsed().as_nanos().min(u64::MAX as u128) as u64);
            observation.calls += 1;
            observation.bytes += (output.len() - before) as u64;
            observation.errors += u64::from(result.is_err());
        }
        result
    }

    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let began = self.observation.as_ref().map(|_| Instant::now());
        let result = self.reader.read(output);
        if let Some(observation) = &mut self.observation {
            observation.read_ns = observation
                .read_ns
                .saturating_add(began.unwrap().elapsed().as_nanos().min(u64::MAX as u128) as u64);
            observation.calls += 1;
            match &result {
                Ok(bytes) => observation.bytes += *bytes as u64,
                Err(_) => observation.errors += 1,
            }
        }
        result
    }
}

impl<R: io::Seek> io::Seek for ObservedReader<R> {
    fn seek(&mut self, position: io::SeekFrom) -> io::Result<u64> {
        self.reader.seek(position)
    }
}

impl<R> Drop for ObservedReader<R> {
    fn drop(&mut self) {
        self.finish(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Read, Seek};

    #[test]
    fn observation_preserves_read_seek_and_eof_without_additional_reads() {
        let mut reader =
            ObservedReader::new(Cursor::new(b"sensor bytes"), "original", Path::new("a.x3f"));
        let mut first = [0; 6];
        reader.read_exact(&mut first).unwrap();
        assert_eq!(&first, b"sensor");
        assert_eq!(reader.seek(io::SeekFrom::Start(7)).unwrap(), 7);
        let mut rest = Vec::new();
        assert_eq!(reader.read_to_end(&mut rest).unwrap(), 5);
        assert_eq!(&rest, b"bytes");
        assert_eq!(reader.get_ref().position(), 12);
        reader.complete();
    }

    #[test]
    fn bulk_reads_delegate_and_count_partial_bytes_on_failure() {
        struct Partial;
        impl Read for Partial {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                panic!("bulk read lost the underlying specialization")
            }
            fn read_to_end(&mut self, output: &mut Vec<u8>) -> io::Result<usize> {
                output.extend_from_slice(b"partial");
                Err(io::ErrorKind::UnexpectedEof.into())
            }
        }
        let mut reader = ObservedReader::new(Partial, "original", Path::new("a.x3f"));
        reader.observation = Some(Observation {
            capture: 0,
            source: String::new(),
            storage: "original",
            bytes: 0,
            calls: 0,
            errors: 0,
            read_ns: 0,
            complete: false,
        });
        let mut output = vec![1, 2];
        assert_eq!(
            reader.read_to_end(&mut output).unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
        let observation = reader.observation.as_ref().unwrap();
        assert_eq!(
            (observation.bytes, observation.calls, observation.errors),
            (7, 1, 1)
        );
        assert_eq!(&output[2..], b"partial");
    }
}
