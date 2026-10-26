//! The command line of action sequences (FOL-L047):
//! `folitecta --run-actions <sequence.json> [--output <folder>] [--suffix <text>]
//! [--overwrite] <file or folder> ...` runs the sequence on every PDF file
//! (folders searched with their subfolders), the same way the window does:
//! one line of JSON per file — input, output, status (`done`, `failed`,
//! `skipped`), message and log — then a summary line. No window opens. The
//! exit status is 0 when every file is done, 1 when one is not, 2 when the
//! command itself is wrong.

use std::path::PathBuf;

use folitecta_core::actions::{batch_inputs, plan_batch, run_batch_item};
use folitecta_core::{ActionSequence, OutputRule};

/// On Windows the program has no console of its own: the commands of the
/// command line write to the one they were started from.
pub fn attach_console() {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

const USAGE: &str = "usage: folitecta --run-actions <sequence.json> [--output <folder>] [--suffix <text>] [--overwrite] <file or folder> ...";

/// Run the command line of `--run-actions` (at `at` in `args`); the exit status.
pub fn run_actions(args: &[String], at: usize) -> i32 {
    let fail = |why: String| {
        eprintln!("{why}\n{USAGE}");
        2
    };
    let Some(path) = args.get(at + 1) else {
        return fail("--run-actions needs a sequence file".into());
    };
    let seq = match std::fs::read_to_string(path)
        .map_err(|e| format!("{path}: {e}"))
        .and_then(|t| ActionSequence::from_json(&t).map_err(|e| format!("{path}: {e}")))
    {
        Ok(s) => s,
        Err(e) => return fail(e),
    };
    let mut rule = OutputRule::default();
    let mut inputs: Vec<PathBuf> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if i == at {
            i += 2;
            continue;
        }
        match args[i].as_str() {
            "--output" => {
                let Some(v) = args.get(i + 1) else {
                    return fail("--output needs a folder".into());
                };
                rule.folder = Some(PathBuf::from(v));
                i += 2;
            }
            "--suffix" => {
                let Some(v) = args.get(i + 1) else {
                    return fail("--suffix needs a text".into());
                };
                rule.suffix = v.clone();
                i += 2;
            }
            "--overwrite" => {
                rule.overwrite = true;
                i += 1;
            }
            other if other.starts_with("--") => return fail(format!("unknown option {other}")),
            other => {
                inputs.push(PathBuf::from(other));
                i += 1;
            }
        }
    }
    if inputs.is_empty() {
        return fail("no file or folder to work on".into());
    }
    let plan = plan_batch(&batch_inputs(&inputs), &rule);
    let (mut done, mut failed, mut skipped) = (0, 0, 0);
    for item in &plan {
        let r = run_batch_item(&seq, item, &rule);
        match r.status {
            "done" => done += 1,
            "failed" => failed += 1,
            _ => skipped += 1,
        }
        println!("{}", serde_json::to_string(&r).unwrap_or_default());
    }
    println!(
        "{}",
        serde_json::json!({ "sequence": seq.name, "files": plan.len(), "done": done, "failed": failed, "skipped": skipped })
    );
    if failed + skipped == 0 { 0 } else { 1 }
}
