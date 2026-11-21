//! Validates row-worker damage ordering and compares serial and bounded-pool original loading.

use super::*;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Instant,
};

pub(super) static SERIAL: AtomicBool = AtomicBool::new(false);
struct Reset;
impl Drop for Reset {
    fn drop(&mut self) {
        SERIAL.store(false, Ordering::Relaxed);
    }
}

#[test]
fn parallel_rows_preserve_first_damaged_row_and_all_sample_values() {
    let table = Table::new(&[1 << 27, (1 << 27) | 1], &[-1, 32767]).unwrap();
    let data = [0b10110000, 0, 0b11110000, 0, 0b10101000];
    let rows = [0, 1, 2, 3, 4, 5];
    let mut serial = vec![0.; 5 * 6];
    decode_rows(&table, &data, &rows, 6, &mut serial).unwrap();
    for workers in [1, 2, 4] {
        let pool = Arc::new(crate::decode_pool::Pool::new(workers).unwrap());
        let order = Arc::new(crate::decode_pool::Order::default());
        let gate = Arc::new(crate::priority::Gate::default());
        let mut parallel = vec![0.; serial.len()];
        crate::priority::interruptible(gate, || {
            crate::priority::decoder(
                crate::decode_pool::Task {
                    pool,
                    turn: order.issue(),
                },
                || {
                    crate::decode_pool::rows(&mut parallel, 6, |first, output| {
                        decode_rows(&table, &data, &rows[first..], 6, output)
                    })
                },
            )
        })
        .unwrap();
        assert_eq!(parallel, serial);
    }
    // Row zero truncates. Row two reaches an unassigned prefix. Completion
    // order must not change the earlier row's technical cause.
    let table = Table::new(&[2 << 27], &[1]).unwrap();
    let rows = [0, 0, 1, 2, 3];
    let data = [0, 0xff, 0];
    let mut output = vec![0.; 4 * 3];
    let expected = decode_rows(&table, &data, &rows, 3, &mut output).unwrap_err();
    let actual = crate::decode_pool::rows(&mut output, 3, |first, output| {
        decode_rows(&table, &data, &rows[first..], 3, output)
    })
    .unwrap_err();
    assert_eq!(actual.to_string(), expected.to_string());
}

#[test]
#[ignore = "requires DECODER_RESEARCH_FILES, optional DECODER_RESEARCH_REPEATS, serial test execution"]
fn profile_bounded_classic_rows() {
    let paths: Vec<PathBuf> =
        std::env::split_paths(&std::env::var_os("DECODER_RESEARCH_FILES").unwrap()).collect();
    let repeats: usize = std::env::var("DECODER_RESEARCH_REPEATS")
        .ok()
        .map_or(5, |v| v.parse().unwrap());
    let selected = std::env::var("DECODER_CLASSIC_MODE").ok();
    for path in paths {
        SERIAL.store(selected.as_deref() == Some("serial"), Ordering::Relaxed);
        let reset = Reset;
        let expected = crate::source::Source::open(&path).unwrap();
        drop(reset);
        for repeat in 0..repeats {
            for (mode, workers) in [
                ("serial", 0),
                ("foreground", 0),
                ("shared", 1),
                ("shared", 2),
                ("shared", 4),
            ] {
                let label = if workers == 0 {
                    mode.to_owned()
                } else {
                    format!("shared{workers}")
                };
                if selected
                    .as_deref()
                    .is_some_and(|selected| selected != label)
                {
                    continue;
                }
                SERIAL.store(mode == "serial", Ordering::Relaxed);
                let reset = Reset;
                let pool = (workers > 0)
                    .then(|| Arc::new(crate::decode_pool::Pool::new(workers).unwrap()));
                let order = Arc::new(crate::decode_pool::Order::default());
                let gate = Arc::new(crate::priority::Gate::default());
                let began = Instant::now();
                let source = if let Some(pool) = pool.as_ref() {
                    crate::priority::interruptible(gate, || {
                        crate::priority::decoder(
                            crate::decode_pool::Task {
                                pool: pool.clone(),
                                turn: order.issue(),
                            },
                            || crate::source::Source::open(&path),
                        )
                    })
                    .unwrap()
                } else {
                    crate::source::Source::open(&path).unwrap()
                };
                let milliseconds = began.elapsed().as_secs_f64() * 1000.;
                drop(reset);
                assert_eq!(source.samples(), expected.samples());
                println!(
                    "{}",
                    crate::profile::report(
                        "decoder_classic_rows",
                        crate::profile::Mode::Off,
                        serde_json::json!({"source":path,"mode":mode,"workers":workers,"repeat":repeat,"open_ms":milliseconds,"stages":source.load_timings(),"exact":true})
                    )
                );
            }
        }
    }
}
