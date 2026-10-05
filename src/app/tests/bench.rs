//! Timings over a ten-thousand-command vault, ignored by default. Run them with
//! `cargo test --release bench -- --ignored --nocapture --test-threads=1` and read the
//! medians, one thread so the timings do not share the CPU.
//! Each also fails past a ceiling far above its measured time, so only a slowdown a
//! user would feel turns into a failure.

use std::hint::black_box;
use std::time::Instant;

use super::*;

const COMMANDS: usize = 10_000;
const RUNS: usize = 21;

/// The median of `RUNS` timings of `f`, printed under `label`.
fn median<T>(label: &str, mut f: impl FnMut() -> T) -> Duration {
    let mut took: Vec<Duration> = (0..RUNS)
        .map(|_| {
            let start = Instant::now();
            black_box(f());
            start.elapsed()
        })
        .collect();
    took.sort();
    let median = took[RUNS / 2];
    eprintln!("{label}: median {median:?} of {RUNS} runs");
    median
}

/// Ten thousand commands over forty categories two levels deep and fifty tags, every
/// third one copied once.
fn big_app() -> App {
    let mut app = app();
    let areas: Vec<Uuid> = (0..8)
        .map(|i| app.vault.add_category(format!("area {i}"), None))
        .collect();
    let topics: Vec<Uuid> = (0..32)
        .map(|i| {
            app.vault
                .add_category(format!("topic {i}"), Some(areas[i % areas.len()]))
        })
        .collect();
    for i in 0..COMMANDS {
        let mut c = Command::new(Draft {
            title: format!("Command number {i}"),
            command_text: format!("echo {i} && ls -la /tmp/{i}"),
            description: "a description with some words in it".into(),
            tags: vec![format!("tag{}", i % 50)],
            category_id: Some(topics[i % topics.len()]),
        });
        if i % 3 == 0 {
            c.copies = 1;
            c.last_copied = Some(c.created_at);
        }
        app.vault.commands.push(c);
    }
    app.vault.ensure_tags();
    app
}

#[test]
#[ignore = "a timing, run with --ignored and read the median"]
fn bench_search_and_category_filter() {
    let mut app = big_app();
    app.search = "echo 99 tag4".into();
    let search = median("search, three terms", || app.visible().len());
    app.search = "cat:\"topic 3\"".into();
    let prefix = median("search, a cat: prefix", || app.visible().len());
    app.search.clear();
    app.selected_category = app.vault.categories.first().map(|c| c.id);
    let subtree = median("sidebar category, no search", || app.visible().len());
    for took in [search, prefix, subtree] {
        assert!(
            took < Duration::from_millis(500),
            "a keystroke must not lag"
        );
    }
}

#[test]
#[ignore = "a timing, run with --ignored and read the median"]
fn bench_every_sort_order() {
    let mut app = big_app();
    for sort in Sort::ALL {
        app.settings.sort = sort;
        let took = median(&format!("list, {sort}"), || app.visible().len());
        assert!(took < Duration::from_millis(500), "{sort} must not lag");
    }
}

#[test]
#[ignore = "a timing, run with --ignored and read the median"]
fn bench_save_and_load() {
    let app = big_app();
    let dir = std::env::temp_dir().join(format!("commandvault-bench-{}", Uuid::new_v4()));
    let path = dir.join("vault.json");
    let save = median("save with fsync and .bak", || {
        storage::save(&path, &app.vault).unwrap();
    });
    let load = median("load", || storage::load::<Vault>(&path).unwrap());
    std::fs::remove_dir_all(dir).unwrap();
    assert!(save + load < Duration::from_secs(2), "every edit saves");
}

#[test]
#[ignore = "a timing, run with --ignored and read the median"]
fn bench_export() {
    let app = big_app();
    let rows = app.visible();
    let md = median("Markdown export", || export::markdown(&app.vault, &rows));
    let sh = median("shell export", || export::shell(&app.vault, &rows));
    assert!(md + sh < Duration::from_secs(2));
}

#[test]
#[ignore = "a timing, run with --ignored and read the median"]
fn bench_category_tree() {
    // The tree rescans the category list per level, see the Known limit on Vault::tree.
    let mut vault = Vault::default();
    let mut parent = None;
    for i in 0..1_000 {
        let id = vault.add_category(format!("category {i}"), parent);
        parent = (i % 10 != 9).then_some(id);
    }
    let took = median("tree of 1000 categories", || vault.tree().len());
    assert!(
        took < Duration::from_millis(100),
        "the sidebar redraws with it"
    );
}
