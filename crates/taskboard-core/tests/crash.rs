//! Crash test (SPEC §7): a writer loop in a child process is killed at varying
//! points; after every kill the database must pass `quick_check` and contain
//! every write the child acknowledged (printed after the commit returned).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use taskboard_core::{Db, NewTask};

const CHILD_ENV: &str = "TASKBOARD_CRASH_CHILD_DB";
const ITERATIONS: u32 = 100;

/// Child side: runs only when re-executed by `writes_survive_kills`.
#[test]
fn crash_child_writer() {
    let Some(path) = std::env::var_os(CHILD_ENV) else {
        return;
    };
    let prefix = std::env::var("TASKBOARD_CRASH_PREFIX").unwrap();
    let mut db = Db::open(Path::new(&path)).unwrap();
    let mut out = std::io::stdout().lock();
    // Runs until the parent kills it (long before the bound).
    for n in 0..u64::MAX {
        let title = format!("{prefix}-{n}");
        let task = db
            .create_task(NewTask {
                title: title.clone(),
                ..Default::default()
            })
            .unwrap();
        writeln!(out, "ack add {title}").unwrap();
        out.flush().unwrap();
        if n % 3 == 0 {
            db.set_task_done(task.id, true).unwrap();
            writeln!(out, "ack done {title}").unwrap();
            out.flush().unwrap();
        }
    }
}

/// Small deterministic generator so failures are reproducible from the seed.
struct XorShift(u64);
impl XorShift {
    fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % n
    }
}

#[test]
fn writes_survive_kills() {
    if std::env::var_os(CHILD_ENV).is_some() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("crash.db");
    Db::open(&path).unwrap();
    let exe = std::env::current_exe().unwrap();
    let mut rng = XorShift(0x9E37_79B9_7F4A_7C15);
    // title -> acknowledged as done
    let mut acked: HashMap<String, bool> = HashMap::new();

    for iteration in 0..ITERATIONS {
        let mut child = Command::new(&exe)
            .args([
                "--exact",
                "crash_child_writer",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD_ENV, &path)
            .env("TASKBOARD_CRASH_PREFIX", format!("i{iteration}"))
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let mut lines = BufReader::new(stdout).lines();

        // Kill after a varying number of acknowledged writes, sometimes with a
        // short delay so the kill lands at different points inside a commit.
        let kill_after = 1 + rng.below(25);
        let mut seen = 0;
        while seen < kill_after {
            let line = lines.next().expect("child exited early").unwrap();
            if record(&line, &mut acked) {
                seen += 1;
            }
        }
        let delay = rng.below(4);
        if delay > 0 {
            std::thread::sleep(Duration::from_millis(delay));
        }
        child.kill().unwrap();
        // Acks printed before the kill are acknowledged writes too.
        for line in lines.map_while(Result::ok) {
            record(&line, &mut acked);
        }
        child.wait().unwrap();

        // Db::open runs PRAGMA quick_check and fails on any damage.
        let db = Db::open(&path).unwrap_or_else(|e| panic!("iteration {iteration}: {e}"));
        drop(db);
        verify(&path, &acked, iteration);
    }
    assert!(acked.len() >= ITERATIONS as usize);
}

/// Records an `ack` line; returns true when it was one.
fn record(line: &str, acked: &mut HashMap<String, bool>) -> bool {
    if let Some(title) = line.strip_prefix("ack add ") {
        acked.entry(title.to_owned()).or_insert(false);
        true
    } else if let Some(title) = line.strip_prefix("ack done ") {
        acked.insert(title.to_owned(), true);
        true
    } else {
        false
    }
}

fn verify(path: &Path, acked: &HashMap<String, bool>, iteration: u32) {
    let conn = rusqlite::Connection::open(path).unwrap();
    let mut stmt = conn
        .prepare("SELECT title, completed_at IS NOT NULL FROM task WHERE deleted_at IS NULL")
        .unwrap();
    let rows: HashMap<String, bool> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    let mut missing = HashSet::new();
    for (title, done) in acked {
        match rows.get(title) {
            None => {
                missing.insert(title.clone());
            }
            Some(stored) => assert!(
                !done || *stored,
                "iteration {iteration}: “{title}” was acknowledged done but is open"
            ),
        }
    }
    assert!(
        missing.is_empty(),
        "iteration {iteration}: acknowledged writes lost: {missing:?}"
    );
}
