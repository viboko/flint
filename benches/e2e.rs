// SPDX-FileCopyrightText: The flint contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tier 2: real filesystem, real binary. Complements `benches/logic.rs`'s
//! in-memory benchmarks by measuring the actual `flint` binary against
//! real on-disk directories, so a regression in the directory-reading
//! path (e.g. an accidental per-file `stat()` call, or non-linear
//! scaling) shows up even though `checker::check` and `flint_file::parse`
//! never touch disk themselves.
//!
//! Real file creation and process spawns are inherently slower per sample
//! than Tier 1's in-memory calls, even at the same sizes, so expect
//! noisier numbers here (see docs/benchmarking.md). Each size's fixture
//! directory is created once, outside the timed portion — only the
//! repeated `flint` invocations against that fixture are measured.

// Measuring real disk I/O and real process spawns is this bench's job —
// `clippy.toml`'s bans are for the library.
#![allow(clippy::disallowed_methods, clippy::disallowed_types)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

/// The numbers of files each run is timed with.
const SIZES: [usize; 3] = [100, 1_000, 10_000];

/// A throwaway directory containing `n` real files and a matching
/// `.flint` allow-list, cleaned up on drop.
struct TempProject {
    /// Where it is.
    dir: PathBuf,
}

impl TempProject {
    /// Creates a fresh one, with `n` files.
    fn with_file_count(n: usize) -> Self {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock is after the Unix epoch")
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("flint-bench-{}-{unique}-{n}", std::process::id()));
        fs::create_dir_all(&dir).expect("create temp project dir");

        let mut flint_contents = String::new();
        for i in 0..n {
            let name = format!("file_{i}.txt");
            fs::write(dir.join(&name), b"").expect("create fixture file");
            flint_contents.push_str(&name);
            flint_contents.push('\n');
        }
        fs::write(dir.join(".flint"), flint_contents).expect("write .flint");

        TempProject { dir }
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// Times `flint` checking a directory of each of [`SIZES`]' file counts.
fn bench_e2e(c: &mut Criterion) {
    let mut group = c.benchmark_group("flint binary (real directory)");
    group.sample_size(20);
    for &n in &SIZES {
        let project = TempProject::with_file_count(n);
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| {
                let output = Command::new(env!("CARGO_BIN_EXE_flint"))
                    .current_dir(&project.dir)
                    .output()
                    .expect("run flint");
                assert!(
                    output.status.success(),
                    "flint reported violations against its own generated fixture"
                );
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_e2e);
criterion_main!(benches);
