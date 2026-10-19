//! Progress of a self-test run: `progress.log` written line by line as the
//! run goes, a time limit on every script run in the page, and a watchdog
//! that ends a run whose interface stopped answering, with the report of what
//! was done until then.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use dioxus::prelude::*;

struct Progress {
    file: Option<std::fs::File>,
    dir: PathBuf,
    start: Instant,
    /// The state check or the step running (named in the scripts that time out).
    current: String,
    /// When the last result was recorded (the duration of the next one).
    last: Instant,
    /// When the run last wrote a line (the watchdog's clock).
    heard: Instant,
    /// The page did not answer the last script: the next ones wait less.
    stalled: bool,
    /// Scripts without an answer since the last check of the timeouts.
    timeouts: Vec<String>,
    /// The entries of the report so far, and their problems.
    entries: Vec<serde_json::Value>,
    problems: usize,
    finished: bool,
}

static PROGRESS: Mutex<Option<Progress>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut Progress) -> R) -> Option<R> {
    let mut slot = PROGRESS.lock().unwrap_or_else(|e| e.into_inner());
    slot.as_mut().map(f)
}

fn write_line(p: &mut Progress, text: &str) {
    let secs = p.start.elapsed().as_secs_f64();
    if let Some(f) = p.file.as_mut() {
        let _ = writeln!(f, "{secs:9.1}s  {text}");
        let _ = f.flush();
    }
}

/// Start the progress log in `dir` and the watchdog.
pub(super) fn open(dir: &Path) {
    let now = Instant::now();
    *PROGRESS.lock().unwrap_or_else(|e| e.into_inner()) = Some(Progress {
        file: std::fs::File::create(dir.join("progress.log")).ok(),
        dir: dir.to_path_buf(),
        start: now,
        current: "start".into(),
        last: now,
        heard: now,
        stalled: false,
        timeouts: Vec::new(),
        entries: Vec::new(),
        problems: 0,
        finished: false,
    });
    watchdog();
}

/// One line of the progress log, written through at once.
pub(super) fn line(text: &str) {
    with(|p| {
        p.heard = Instant::now();
        write_line(p, text);
    });
}

/// A state check or a step starts: logged and named for the scripts it runs.
pub(super) fn begin(kind: &str, name: &str) -> Instant {
    with(|p| {
        p.current = format!("{kind} {name}");
        p.last = Instant::now();
    });
    line(&format!("start {kind} {name}"));
    Instant::now()
}

/// A state check or a step ended with `result`.
pub(super) fn finish(kind: &str, name: &str, result: &str, started: Instant) {
    with(|p| p.current = format!("after {kind} {name}"));
    line(&format!(
        "end   {kind} {name}: {result} ({:.1} s)",
        started.elapsed().as_secs_f64()
    ));
}

/// The time since the last result (or the start of the step), restarted.
pub(super) fn took() -> Duration {
    with(|p| std::mem::replace(&mut p.last, Instant::now()).elapsed()).unwrap_or_default()
}

/// The scripts that got no answer since the last call.
pub(super) fn take_timeouts() -> Vec<String> {
    with(|p| std::mem::take(&mut p.timeouts)).unwrap_or_default()
}

/// Keep a copy of a report entry with its problems, for a report written by
/// the watchdog.
pub(super) fn report(entry: &serde_json::Value, problems: usize) {
    with(|p| {
        p.entries.push(entry.clone());
        p.problems += problems;
    });
}

/// The run is over: the watchdog stops.
pub(super) fn done(total: usize) {
    line(&format!("finished: {total} problem(s)"));
    with(|p| p.finished = true);
}

/// Seconds from the environment variable `name`, or `default`.
fn limit_from_env(name: &str, default: f64) -> Duration {
    let secs = std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|s| s.is_finite() && *s > 0.0)
        .unwrap_or(default);
    Duration::from_secs_f64(secs)
}

/// How long a script may take to answer (`FOLITECTA_SELFTEST_TIMEOUT`, in
/// seconds; 30 by default).
pub(super) fn wait_limit() -> Duration {
    limit_from_env("FOLITECTA_SELFTEST_TIMEOUT", 30.0)
}

/// How long a whole interaction step may take
/// (`FOLITECTA_SELFTEST_STEP_TIMEOUT`, in seconds; 600 by default).
pub(super) fn step_limit() -> Duration {
    limit_from_env("FOLITECTA_SELFTEST_STEP_TIMEOUT", 600.0)
}

/// After a script without an answer the page is likely stuck: the next
/// scripts wait at most this long until one answers again.
const STALLED_WAIT: Duration = Duration::from_secs(3);

/// The first characters of a script on one line (to name it in the report).
fn snippet(js: &str) -> String {
    let flat = js.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut s: String = flat.chars().take(120).collect();
    if flat.chars().count() > 120 {
        s.push('…');
    }
    s
}

/// Run a script in the page and wait for its result, at most the time limit
/// (a few seconds while the page is not answering): a script left without an
/// answer is recorded against the current check or step, the run goes on.
pub(super) async fn eval(js: &str) -> Result<serde_json::Value, String> {
    let limit = if with(|p| p.stalled).unwrap_or(false) {
        wait_limit().min(STALLED_WAIT)
    } else {
        wait_limit()
    };
    match tokio::time::timeout(limit, document::eval(js)).await {
        Ok(result) => {
            with(|p| p.stalled = false);
            result.map_err(|e| e.to_string())
        }
        Err(_) => {
            let what = snippet(js);
            let current = with(|p| {
                p.stalled = true;
                p.timeouts.push(what.clone());
                p.current.clone()
            })
            .unwrap_or_default();
            let secs = limit.as_secs_f64();
            line(&format!("  no answer within {secs:.0} s ({current}): {what}"));
            Err(format!("no answer within {secs:.0} s: {what}"))
        }
    }
}

/// Every task of the interface, the run's own included, stops while the
/// interface thread is blocked or the page never confirms a change, and then
/// no time limit of the run can fire. A thread of its own notes in the log
/// every minute without progress and, once the run has been silent longer
/// than a step may take, writes the report of what was done and ends the
/// program.
fn watchdog() {
    std::thread::spawn(|| {
        let give_up = step_limit() + Duration::from_secs(60);
        let mut noted = 0;
        loop {
            std::thread::sleep(Duration::from_secs(5));
            let Some((quiet, finished)) = with(|p| (p.heard.elapsed(), p.finished)) else {
                return;
            };
            if finished {
                return;
            }
            let minutes = quiet.as_secs() / 60;
            if minutes > noted {
                noted = minutes;
                with(|p| {
                    let text = format!(
                        "  nothing done for {} s ({}): the interface may be stopped",
                        quiet.as_secs(),
                        p.current
                    );
                    write_line(p, &text);
                });
            } else if minutes == 0 {
                noted = 0;
            }
            if quiet > give_up {
                stop_run(quiet);
            }
        }
    });
}

/// Write the report of a run whose interface stopped, and end the program.
fn stop_run(quiet: Duration) -> ! {
    let report = with(|p| {
        let what = format!(
            "the interface did nothing for {} s during {}; the run was stopped",
            quiet.as_secs(),
            p.current
        );
        write_line(p, &what);
        let mut states = p.entries.clone();
        states.push(serde_json::json!({ "interaction": "the interface keeps answering", "pass": false, "detail": what }));
        (
            p.dir.clone(),
            p.problems + 1,
            serde_json::json!({ "total_problems": p.problems + 1, "complete": false, "states": states }),
        )
    });
    if let Some((dir, total, report)) = report {
        let _ = std::fs::write(
            dir.join("report.json"),
            serde_json::to_string_pretty(&report).unwrap_or_default(),
        );
        println!(
            "ui self-test stopped: {total} problem(s); report in {}",
            dir.display()
        );
    }
    std::process::exit(4);
}
