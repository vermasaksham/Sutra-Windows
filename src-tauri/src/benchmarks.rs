//! The performance baseline: what Sutra costs as a vault gets large.
//!
//! This is not a micro-benchmark suite. It exists to answer one architectural
//! question — *which operations get slower as the vault grows?* — because that
//! is the question a storage contract can be frozen against. An operation whose
//! cost tracks the size of the vault is a design decision; an operation that
//! quietly started tracking it is a bug, and this file is how it is caught.
//!
//! Run it, at each size, with:
//!
//! ```text
//! cargo test --bins -- --ignored --nocapture baseline_at_1k
//! cargo test --bins -- --ignored --nocapture baseline_at_10k
//! cargo test --bins -- --ignored --nocapture baseline_at_50k
//! ```
//!
//! Each prints a table and then asserts. The assertions come in two kinds, and
//! the distinction is the whole point:
//!
//!   * **Flat operations** — opening a note, moving it, renaming it, asking for
//!     its backlinks — are checked against a ceiling that does *not* scale with
//!     the vault. Reading one note out of fifty thousand must cost what reading
//!     one out of a thousand costs. If these ever fail, something has started
//!     scanning the vault to find a file whose path it already knows.
//!
//!   * **Whole-vault operations** — the startup listing, an index rebuild, the
//!     research overview — are linear by construction: they read every file.
//!     They are checked against a *per-note* budget, so the ceiling grows with
//!     the vault but the cost per note may not. That is what separates linear
//!     from quadratic.
//!
//! The budgets are deliberately loose. They are set several times above what a
//! developer laptop measures, because the value here is catching a change in
//! *shape*, not policing milliseconds on someone else's hardware. Recorded
//! numbers live in `docs/architecture/performance.md`; these ceilings are the
//! part that fails a build.

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::index::Index;
use crate::vault::Vault;

/// One measured operation.
struct Measure {
    name: &'static str,
    /// How many times it ran.
    runs: usize,
    /// Total wall time across all runs.
    total: Duration,
}

impl Measure {
    fn each(&self) -> Duration {
        self.total / self.runs.max(1) as u32
    }
}

/// Time `body` `runs` times and record it.
fn measure(name: &'static str, runs: usize, mut body: impl FnMut(usize)) -> Measure {
    let started = Instant::now();
    for i in 0..runs {
        body(i);
    }
    Measure {
        name,
        runs,
        total: started.elapsed(),
    }
}

/// A vault that deletes itself, built somewhere a benchmark can write freely.
struct Bench {
    root: PathBuf,
}

impl Bench {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("sutra-bench-{}", ulid::Ulid::generate()));
        fs::create_dir_all(&root).unwrap();
        Self { root }
    }
}

impl Drop for Bench {
    fn drop(&mut self) {
        // Tens of thousands of files. Failing to clean up would leave the next
        // run measuring a disk that is already full.
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// Prose long enough to be worth indexing, and varied enough that search has to
/// do real work rather than matching one repeated string.
fn body_for(i: usize, hub: &str) -> String {
    let mut text = format!(
        "Antimony selenide ribbons grow along the c axis in run {i}. The seed \
         layer decides the texture, and iodine transports the material as \
         $\\ce{{SbI3}}$ in the vapour. Substrate temperature was {} °C and the \
         source sat at {} °C, giving a gradient the growth front follows.\n\n\
         The measured band gap was {}.{} eV, which is consistent with the \
         literature for a film of this thickness, though the tail suggests \
         selenium vacancies rather than a clean edge.\n",
        380 + (i % 90),
        480 + (i % 60),
        1,
        (i % 30) + 10,
    );

    // A fixed number of notes link to the hub, whatever the vault's size, so
    // the backlink lookup returns a constant number of rows at 1k and at 50k.
    // A lookup that nonetheless slows down is scanning, not looking up.
    if i > 0 && i <= LINKERS {
        text.push_str(&format!(
            "\nThis follows the argument in [[{hub}]], which is where the \
             transport model came from.\n"
        ));
    }

    // One rare term, in one note, so search has a needle to find as well as a
    // haystack to reject.
    if i == 7 {
        text.push_str("\nThe anomaly here is unmistakably chalcostibite.\n");
    }

    text
}

/// How many notes link to the hub note, at every vault size.
const LINKERS: usize = 25;

/// How many times each flat operation is sampled.
const SAMPLES: usize = 25;

/// Build a vault of `notes` notes, then measure the operations a person waits on.
///
/// Returns the measurements in the order they were taken, having already printed
/// the table.
fn baseline(notes: usize) -> Vec<Measure> {
    assert!(notes > LINKERS + 10, "too small to measure anything");

    let bench = Bench::new();
    let vault = Vault::open(bench.root.clone()).unwrap();

    // Spread across folders the way a vault actually grows. A single flat
    // directory of 50,000 files measures the filesystem, not Sutra.
    let folders: Vec<String> = (0..12)
        .flat_map(|a| (0..8).map(move |b| format!("Strand {a}/Sub {b}")))
        .collect();
    for folder in &folders {
        vault.create_folder(folder).unwrap();
    }

    // The hub is created first so every other note can link to it by id.
    let hub = vault
        .create_note("Vapour transport of Sb2Se3", Some(folders[0].clone()))
        .unwrap()
        .summary
        .id;
    vault
        .save_note(&hub, "Vapour transport of Sb2Se3", &body_for(0, &hub))
        .unwrap();

    let mut ids = Vec::with_capacity(notes);
    ids.push(hub.clone());

    // Creation is measured in blocks rather than as one total, because *how it
    // changes* across the build is the interesting number: a per-note cost that
    // climbs as the vault fills is the signature of an O(vault) create path.
    let build_started = Instant::now();
    let block = (notes / 4).max(1);
    let mut block_started = Instant::now();
    let mut blocks: Vec<(usize, Duration)> = Vec::new();
    for i in 1..notes {
        let folder = folders[i % folders.len()].clone();
        let doc = vault
            .create_note(&format!("Run {i} on Sb2Se3 growth"), Some(folder))
            .unwrap();
        let id = doc.summary.id.clone();
        vault
            .save_note(&id, &doc.summary.title, &body_for(i, &hub))
            .unwrap();
        ids.push(id);

        if i % block == 0 {
            blocks.push((i, block_started.elapsed() / block as u32));
            block_started = Instant::now();
        }
    }
    let built = build_started.elapsed();

    eprintln!();
    eprintln!("=== Sutra performance baseline — {notes} notes ===");
    eprintln!(
        "built in {built:?} ({:?} per note average)",
        built / notes as u32
    );
    eprintln!("  create+save cost per note, as the vault fills:");
    for (at, each) in &blocks {
        eprintln!("    after {at:>6} notes: {each:?}");
    }

    let mut out = Vec::new();

    // --- startup ---------------------------------------------------------
    // What happens between double-clicking Sutra and seeing the sidebar.
    out.push(measure("vault open (cold)", 3, |_| {
        Vault::open(bench.root.clone()).unwrap();
    }));
    out.push(measure("list_notes (startup listing)", 3, |_| {
        let listed = vault.list_notes().unwrap();
        assert_eq!(listed.len(), notes);
    }));
    out.push(measure("list_folders", 3, |_| {
        vault.list_folders().unwrap();
    }));
    out.push(measure("list_tags", 3, |_| {
        vault.list_tags().unwrap();
    }));

    // --- indexing --------------------------------------------------------
    let index_path = bench.root.join(".sutra").join("bench-index.sqlite3");
    let index = Index::open(&index_path).unwrap();
    out.push(measure("index rebuild (whole vault)", 1, |_| {
        let indexed = index.rebuild(&vault).unwrap();
        assert_eq!(indexed, notes);
    }));
    drop(index);
    out.push(measure("index open (already built)", 3, |_| {
        Index::open(&index_path).unwrap();
    }));
    let index = Index::open(&index_path).unwrap();

    // --- per-note work ---------------------------------------------------
    // Everything below must be flat. The indices are spread across the vault
    // so no measurement can accidentally sample only the first folder.
    let spread: Vec<&String> = (0..SAMPLES)
        .map(|s| &ids[(s * notes / SAMPLES).min(notes - 1)])
        .collect();

    out.push(measure("read_note (open one note)", SAMPLES, |s| {
        vault.read_note(spread[s]).unwrap();
    }));
    out.push(measure("save_note (edit one note)", SAMPLES, |s| {
        let doc = vault.read_note(spread[s]).unwrap();
        vault
            .save_note(&doc.summary.id, &doc.summary.title, &doc.body)
            .unwrap();
    }));
    out.push(measure(
        "save_note (rename, moves the file)",
        SAMPLES,
        |s| {
            let doc = vault.read_note(spread[s]).unwrap();
            vault
                .save_note(
                    &doc.summary.id,
                    &format!("{} renamed", doc.summary.title),
                    &doc.body,
                )
                .unwrap();
        },
    ));
    out.push(measure("move_note (between folders)", SAMPLES, |s| {
        let target = &folders[(s + 3) % folders.len()];
        vault.move_note(spread[s], target).unwrap();
    }));
    out.push(measure("index upsert (one note)", SAMPLES, |s| {
        let doc = vault.read_note(spread[s]).unwrap();
        index.upsert(&doc.summary, &doc.body).unwrap();
    }));
    out.push(measure("backlinks (constant result size)", SAMPLES, |_| {
        let found = index.backlinks(&hub).unwrap();
        assert_eq!(
            found.len(),
            LINKERS,
            "the hub should have {LINKERS} backlinks"
        );
    }));

    // --- search ----------------------------------------------------------
    out.push(measure("search (rare term, one hit)", SAMPLES, |_| {
        let hits = index.search("chalcostibite", 30).unwrap();
        assert_eq!(hits.len(), 1);
    }));
    out.push(measure("search (common term, capped)", SAMPLES, |_| {
        let hits = index.search("selenide", 30).unwrap();
        assert_eq!(
            hits.len(),
            30,
            "the limit should cap the work, not the corpus"
        );
    }));
    out.push(measure("related (one note)", SAMPLES / 5, |s| {
        let doc = vault.read_note(spread[s]).unwrap();
        index.related(&doc.summary.id, &doc.body, 8).unwrap();
    }));

    // --- the dashboard ---------------------------------------------------
    // `overview` reads every file. It is on this list because it is the one
    // whole-vault read that happens while a person is already looking at the
    // app, rather than during startup.
    out.push(measure("research overview (whole vault)", 1, |_| {
        vault.overview().unwrap();
    }));

    let width = out.iter().map(|m| m.name.len()).max().unwrap_or(0);
    eprintln!();
    eprintln!("  {:<width$}  {:>12}  {:>12}", "operation", "each", "runs");
    for m in &out {
        eprintln!(
            "  {:<width$}  {:>12}  {:>12}",
            m.name,
            format!("{:?}", m.each()),
            m.runs
        );
    }
    eprintln!();

    out
}

/// Look one measurement up by name, so the assertions read as prose.
fn each(measures: &[Measure], name: &str) -> Duration {
    measures
        .iter()
        .find(|m| m.name == name)
        .unwrap_or_else(|| panic!("no measurement named {name}"))
        .each()
}

/// The operations that must cost the same at 50,000 notes as at 1,000.
///
/// `ceiling` is per operation and independent of `notes` — that independence is
/// the assertion. It is generous: these are single file reads and single SQLite
/// statements, so a laptop measures tens of microseconds, and the budget is
/// milliseconds.
fn assert_flat(measures: &[Measure], notes: usize) {
    for (name, ceiling) in [
        ("read_note (open one note)", 25),
        ("save_note (edit one note)", 200),
        ("save_note (rename, moves the file)", 200),
        ("move_note (between folders)", 200),
        ("index upsert (one note)", 200),
        ("backlinks (constant result size)", 100),
        ("search (rare term, one hit)", 250),
        ("search (common term, capped)", 250),
    ] {
        let took = each(measures, name);
        assert!(
            took.as_millis() < ceiling,
            "{name} took {took:?} in a vault of {notes} notes, over its {ceiling}ms ceiling; \
             this cost is supposed to be independent of vault size"
        );
    }
}

/// The operations that read the whole vault, budgeted per note.
///
/// A linear operation passes this at every size. A quadratic one passes at 1k
/// and fails at 10k, which is exactly the failure this file exists to produce
/// before a user does.
fn assert_linear(measures: &[Measure], notes: usize) {
    for (name, micros_per_note) in [
        // `Vault::open` walks the vault on purpose — it populates the id -> path
        // map so the first note a person opens does not pay for the scan. That
        // makes launching Sutra linear in vault size, which is a decision worth
        // measuring rather than a bug worth fixing.
        ("vault open (cold)", 800),
        ("list_notes (startup listing)", 800),
        ("list_tags", 800),
        ("index rebuild (whole vault)", 2_000),
        ("research overview (whole vault)", 1_500),
    ] {
        let took = each(measures, name);
        let budget = Duration::from_micros(micros_per_note * notes as u64);
        assert!(
            took < budget,
            "{name} took {took:?} for {notes} notes, over its budget of {budget:?} \
             ({micros_per_note}µs per note); a whole-vault read may be linear, not worse"
        );
    }
}

#[test]
#[ignore = "benchmark: builds a vault of 1,000 notes"]
fn baseline_at_1k() {
    let measures = baseline(1_000);
    assert_flat(&measures, 1_000);
    assert_linear(&measures, 1_000);
}

#[test]
#[ignore = "benchmark: builds a vault of 10,000 notes"]
fn baseline_at_10k() {
    let measures = baseline(10_000);
    assert_flat(&measures, 10_000);
    assert_linear(&measures, 10_000);
}

#[test]
#[ignore = "benchmark: builds a vault of 50,000 notes, and takes minutes"]
fn baseline_at_50k() {
    let measures = baseline(50_000);
    assert_flat(&measures, 50_000);
    assert_linear(&measures, 50_000);
}
