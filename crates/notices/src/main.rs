//! Writes the licence notices of the libraries built into the program.
//!
//! The release packages carry `THIRD-PARTY-LICENSES.txt` next to `LICENSE`:
//! the MIT, BSD and Apache licences of the libraries ask for their notices to
//! go with copies of the program, and the MPL-2.0 for the place of their
//! source. The libraries are those `cargo metadata` resolves for the target
//! systems as normal dependencies of the `folitecta` program (build and test
//! dependencies are left out); each one's licence texts are the licence,
//! copying and notice files its published source carries. A library without
//! such files takes those of another library from the same repository, or,
//! when its licence allows MIT, the MIT text with its authors; any other
//! library without a text stops the run, so that a new dependency cannot
//! leave the notices incomplete.
//!
//! ```text
//! folitecta-notices --target <triple> [--target <triple>...] --out <file> [--manifest-path <Cargo.toml>]
//! ```

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;
use std::process::{Command, ExitCode};

use serde_json::Value;

/// The program whose libraries are listed.
const PROGRAM: &str = "folitecta";

/// The MIT text with `{COPYRIGHT}` in place of its copyright line (the text
/// of Folitecta's own LICENSE).
const MIT: &str = include_str!("../texts/MIT.txt");

/// A library built into the program.
#[derive(Debug, Clone)]
struct Library {
    name: String,
    version: String,
    license: String,
    repository: String,
    authors: Vec<String>,
    /// The folder of its published source.
    dir: PathBuf,
    /// The `license-file` of its manifest, relative to `dir`.
    license_file: Option<String>,
}

impl Library {
    fn from_metadata(package: &Value) -> Option<Library> {
        let text = |key: &str| {
            package
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let manifest = PathBuf::from(package.get("manifest_path")?.as_str()?);
        Some(Library {
            name: text("name"),
            version: text("version"),
            license: text("license"),
            repository: text("repository"),
            authors: package
                .get("authors")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
                .unwrap_or_default(),
            dir: manifest.parent()?.to_path_buf(),
            license_file: package
                .get("license_file")
                .and_then(Value::as_str)
                .map(str::to_string),
        })
    }

    fn label(&self) -> String {
        format!("{} {}", self.name, self.version)
    }
}

/// One licence text and the libraries it is the text of.
struct Notice {
    /// Where the text comes from, for the heading.
    origin: String,
    text: String,
    libraries: BTreeSet<String>,
}

fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(summary) => {
            println!("{summary}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("folitecta-notices: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<String, String> {
    let mut targets = Vec::new();
    let mut out = None;
    let mut manifest = String::from("Cargo.toml");
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let mut value = || args.next().ok_or_else(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--target" => targets.push(value()?),
            "--out" => out = Some(PathBuf::from(value()?)),
            "--manifest-path" => manifest = value()?,
            _ => return Err(format!("unknown argument {arg}")),
        }
    }
    let out = out.ok_or("--out is needed")?;
    if targets.is_empty() {
        return Err("at least one --target is needed".into());
    }
    let mut libraries = BTreeMap::new();
    let mut everything = Vec::new();
    let mut version = String::new();
    for target in &targets {
        let metadata = cargo_metadata(&manifest, target)?;
        version = program_version(&metadata).unwrap_or_default();
        for library in libraries_of_program(&metadata)? {
            libraries.insert(library.label(), library);
        }
        everything.extend(all_packages(&metadata));
    }
    let libraries: Vec<Library> = libraries.into_values().collect();
    let notices = notices(&libraries, &everything)?;
    let text = document(&version, &targets, &libraries, &notices);
    std::fs::write(&out, text).map_err(|e| format!("cannot write {}: {e}", out.display()))?;
    Ok(format!(
        "{}: {} libraries, {} licence texts",
        out.display(),
        libraries.len(),
        notices.len()
    ))
}

fn cargo_metadata(manifest: &str, target: &str) -> Result<Value, String> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(cargo)
        .args([
            "metadata",
            "--format-version",
            "1",
            "--locked",
            "--manifest-path",
            manifest,
        ])
        .args(["--filter-platform", target])
        .output()
        .map_err(|e| format!("cannot run cargo metadata: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "cargo metadata for {target} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    serde_json::from_slice(&output.stdout).map_err(|e| format!("cargo metadata gave no JSON: {e}"))
}

fn program_version(metadata: &Value) -> Option<String> {
    metadata
        .get("packages")?
        .as_array()?
        .iter()
        .find(|p| p.get("name").and_then(Value::as_str) == Some(PROGRAM))?
        .get("version")?
        .as_str()
        .map(str::to_string)
}

fn all_packages(metadata: &Value) -> Vec<Library> {
    metadata
        .get("packages")
        .and_then(Value::as_array)
        .map(|p| p.iter().filter_map(Library::from_metadata).collect())
        .unwrap_or_default()
}

/// The libraries the program is built from: the normal dependencies of the
/// program, followed from one to the next, without the workspace's own
/// crates.
fn libraries_of_program(metadata: &Value) -> Result<Vec<Library>, String> {
    let ids = |key: &str| -> BTreeSet<&str> {
        metadata
            .get(key)
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default()
    };
    let members = ids("workspace_members");
    let packages: HashMap<&str, &Value> = metadata
        .get("packages")
        .and_then(Value::as_array)
        .ok_or("cargo metadata has no packages")?
        .iter()
        .filter_map(|p| Some((p.get("id")?.as_str()?, p)))
        .collect();
    let nodes: HashMap<&str, &Value> = metadata
        .pointer("/resolve/nodes")
        .and_then(Value::as_array)
        .ok_or("cargo metadata has no resolved graph")?
        .iter()
        .filter_map(|n| Some((n.get("id")?.as_str()?, n)))
        .collect();
    let root = packages
        .iter()
        .find(|(id, p)| members.contains(*id) && p.get("name").and_then(Value::as_str) == Some(PROGRAM))
        .map(|(id, _)| *id)
        .ok_or("the program is not in the workspace")?;
    let mut seen = BTreeSet::new();
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        let deps = nodes
            .get(id)
            .and_then(|n| n.get("deps"))
            .and_then(Value::as_array);
        for dep in deps.into_iter().flatten() {
            let normal = dep
                .get("dep_kinds")
                .and_then(Value::as_array)
                .is_some_and(|kinds| kinds.iter().any(|k| k.get("kind").is_none_or(Value::is_null)));
            if let (true, Some(pkg)) = (normal, dep.get("pkg").and_then(Value::as_str)) {
                stack.push(pkg);
            }
        }
    }
    Ok(seen
        .into_iter()
        .filter(|id| !members.contains(id))
        .filter_map(|id| packages.get(id).and_then(|p| Library::from_metadata(p)))
        .collect())
}

/// Whether a file name is that of a licence, copying, copyright or notice
/// file (`LICENSE`, `LICENSE-MIT`, `licence.txt`, `COPYING`, `NOTICE`...).
fn is_licence_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [
        "license",
        "licence",
        "copying",
        "copyright",
        "notice",
        "unlicense",
    ]
    .iter()
    .any(|stem| {
        lower
            .strip_prefix(stem)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with(['-', '_', '.', ' ']))
    })
}

/// The licence files of a library's source, in a stable order.
fn licence_files(library: &Library) -> Vec<PathBuf> {
    let mut files = BTreeSet::new();
    if let Ok(entries) = std::fs::read_dir(&library.dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if path.is_file() && is_licence_name(&name) {
                files.insert(path);
            } else if path.is_dir() && name.eq_ignore_ascii_case("licenses") {
                // A folder of licence texts (REUSE).
                for inner in std::fs::read_dir(&path).into_iter().flatten().flatten() {
                    if inner.path().is_file() {
                        files.insert(inner.path());
                    }
                }
            }
        }
    }
    if let Some(file) = &library.license_file {
        let path = library.dir.join(file);
        if path.is_file() {
            files.insert(path);
        }
    }
    files.into_iter().collect()
}

/// A repository address without the differences that do not name another
/// repository (letter case, `.git`, a trailing slash).
fn repository_key(url: &str) -> String {
    let url = url.trim().trim_end_matches('/').to_ascii_lowercase();
    url.strip_suffix(".git").unwrap_or(&url).to_string()
}

/// Whether a licence expression lets the library be used under MIT alone.
fn allows_mit(expression: &str) -> bool {
    let words: Vec<&str> = expression
        .split(|c: char| c.is_whitespace() || c == '(' || c == ')' || c == '/')
        .filter(|w| !w.is_empty())
        .collect();
    // An expression of alternatives only (no AND, no WITH), one of them MIT.
    words.contains(&"MIT") && !words.iter().any(|w| *w == "AND" || *w == "WITH")
}

/// The MIT text with the authors of a library as its copyright holders.
fn mit_text(library: &Library) -> String {
    let authors: Vec<String> = library
        .authors
        .iter()
        .map(|a| a.split('<').next().unwrap_or(a).trim().to_string())
        .filter(|a| !a.is_empty())
        .collect();
    let holders = if authors.is_empty() {
        format!("the authors of {}", library.name)
    } else {
        authors.join(", ")
    };
    MIT.replace("{COPYRIGHT}", &format!("Copyright (c) {holders}"))
}

/// A licence text as compared with others: the same words in the same lines.
fn normalised(text: &str) -> String {
    text.replace("\r\n", "\n")
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// The licence texts of the libraries, each text once with the libraries it
/// belongs to. `everything` are all the packages known, to find another
/// library of the same repository.
fn notices(libraries: &[Library], everything: &[Library]) -> Result<Vec<Notice>, String> {
    let mut by_text: BTreeMap<String, Notice> = BTreeMap::new();
    let mut missing = Vec::new();
    for library in libraries {
        let mut texts: Vec<(String, String)> = licence_files(library)
            .iter()
            .filter_map(|path| {
                let text = String::from_utf8_lossy(&std::fs::read(path).ok()?).to_string();
                let name = path
                    .strip_prefix(&library.dir)
                    .unwrap_or(path)
                    .display()
                    .to_string();
                Some((format!("{name} of {}", library.label()), text))
            })
            .collect();
        if texts.is_empty() && !library.repository.is_empty() {
            let key = repository_key(&library.repository);
            let sibling = everything
                .iter()
                .filter(|other| other.name != library.name && repository_key(&other.repository) == key)
                .find(|other| !licence_files(other).is_empty());
            if let Some(sibling) = sibling {
                for path in licence_files(sibling) {
                    if let Ok(bytes) = std::fs::read(&path) {
                        let name = path
                            .strip_prefix(&sibling.dir)
                            .unwrap_or(&path)
                            .display()
                            .to_string();
                        texts.push((
                            format!("{name} of {} (the same repository)", sibling.label()),
                            String::from_utf8_lossy(&bytes).to_string(),
                        ));
                    }
                }
            }
        }
        if texts.is_empty() && allows_mit(&library.license) {
            texts.push((
                format!(
                    "the MIT License of {} (its source carries no licence file)",
                    library.label()
                ),
                mit_text(library),
            ));
        }
        if texts.is_empty() {
            missing.push(format!("{} ({})", library.label(), library.license));
        }
        for (origin, text) in texts {
            by_text
                .entry(normalised(&text))
                .or_insert_with(|| Notice {
                    origin,
                    text: normalised(&text),
                    libraries: BTreeSet::new(),
                })
                .libraries
                .insert(library.label());
        }
    }
    if !missing.is_empty() {
        return Err(format!(
            "no licence text for {}: add one to crates/notices (or a clarification)",
            missing.join(", ")
        ));
    }
    let mut notices: Vec<Notice> = by_text.into_values().collect();
    notices.sort_by(|a, b| a.libraries.iter().next().cmp(&b.libraries.iter().next()));
    Ok(notices)
}

fn document(version: &str, targets: &[String], libraries: &[Library], notices: &[Notice]) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "Folitecta {version}: licences of the libraries in this program\n"
    ));
    out.push_str(&format!("Built for: {}\n\n", targets.join(", ")));
    out.push_str(
        "Folitecta is free software under the MIT License (see LICENSE). The program is built from\n\
         the libraries listed below, each under its own licence; their licence texts follow the\n\
         list. The source code of every library is published on crates.io at the address given\n\
         for it, where the libraries under the Mozilla Public License 2.0 can be obtained in\n\
         source form. The program also contains the Rust standard library (MIT OR Apache-2.0,\n\
         https://github.com/rust-lang/rust). The spelling dictionaries, the text recognition\n\
         models and the font in the dictionaries, ocr and fonts folders carry their own licence\n\
         files in those folders.\n\n",
    );
    out.push_str(&format!("LIBRARIES ({})\n\n", libraries.len()));
    for library in libraries {
        out.push_str(&format!(
            "{} — {} — https://crates.io/crates/{}/{}",
            library.label(),
            if library.license.is_empty() {
                "licence not stated"
            } else {
                &library.license
            },
            library.name,
            library.version
        ));
        if !library.repository.is_empty() {
            out.push_str(&format!(" — {}", library.repository));
        }
        out.push('\n');
    }
    out.push_str(&format!("\nLICENCE TEXTS ({})\n", notices.len()));
    for notice in notices {
        let users: Vec<&str> = notice.libraries.iter().map(String::as_str).collect();
        out.push_str(&format!(
            "\n{}\n{}\nText of: {}\n\n{}\n",
            "=".repeat(78),
            notice.origin,
            users.join(", "),
            notice.text
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::Path;

    fn library(name: &str, license: &str, repository: &str, dir: &Path) -> Library {
        Library {
            name: name.into(),
            version: "1.0.0".into(),
            license: license.into(),
            repository: repository.into(),
            authors: vec!["Ada Example <ada@example.org>".into()],
            dir: dir.to_path_buf(),
            license_file: None,
        }
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("folitecta-notices-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn licence_files_are_found_by_their_names() {
        for name in [
            "LICENSE",
            "LICENSE-MIT",
            "LICENSE_APACHE",
            "licence.txt",
            "COPYING",
            "NOTICE",
            "UNLICENSE",
        ] {
            assert!(is_licence_name(name), "{name}");
        }
        for name in ["README.md", "licenses.rs", "noticeboard", "Cargo.toml"] {
            assert!(!is_licence_name(name), "{name}");
        }
    }

    #[test]
    fn only_the_normal_dependencies_of_the_program_are_its_libraries() {
        let dir = scratch("graph");
        let manifest = |name: &str| dir.join(name).join("Cargo.toml").display().to_string();
        let package = |id: &str| json!({"id": id, "name": id, "version": "1.0.0", "license": "MIT", "manifest_path": manifest(id)});
        let normal = json!([{"kind": null, "target": null}]);
        let metadata = json!({
            "workspace_members": ["folitecta", "folitecta-core"],
            "packages": [package("folitecta"), package("folitecta-core"), package("a"), package("b"), package("c"), package("d"), package("e")],
            "resolve": {"nodes": [
                {"id": "folitecta", "deps": [
                    {"pkg": "folitecta-core", "dep_kinds": normal},
                    {"pkg": "b", "dep_kinds": [{"kind": "dev", "target": null}]},
                ]},
                {"id": "folitecta-core", "deps": [{"pkg": "a", "dep_kinds": normal}]},
                {"id": "a", "deps": [
                    {"pkg": "c", "dep_kinds": [{"kind": "build", "target": null}]},
                    {"pkg": "d", "dep_kinds": [{"kind": "build", "target": null}, {"kind": null, "target": null}]},
                ]},
                {"id": "b", "deps": [{"pkg": "e", "dep_kinds": normal}]},
                {"id": "c", "deps": []}, {"id": "d", "deps": []}, {"id": "e", "deps": []},
            ]}
        });
        let names: Vec<String> = libraries_of_program(&metadata)
            .unwrap()
            .into_iter()
            .map(|l| l.name)
            .collect();
        assert_eq!(names, ["a", "d"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_library_without_licence_files_takes_those_of_its_repository_or_the_mit_text() {
        let dir = scratch("texts");
        let with = dir.join("with");
        let without = dir.join("without");
        let alone = dir.join("alone");
        for d in [&with, &without, &alone] {
            std::fs::create_dir_all(d).unwrap();
        }
        std::fs::write(with.join("LICENSE"), "Copyright (c) The Siblings\nBSD terms\n").unwrap();
        let sibling = library("sib-core", "BSD-3-Clause", "https://github.com/x/sib", &with);
        let member = library(
            "sib-extra",
            "BSD-3-Clause",
            "https://github.com/X/sib.git/",
            &without,
        );
        let mit = library(
            "lonely",
            "MIT OR Apache-2.0",
            "https://github.com/x/lonely",
            &alone,
        );
        let libraries = vec![member.clone(), mit.clone()];
        let everything = vec![sibling, member, mit];
        let notices = notices(&libraries, &everything).unwrap();
        assert_eq!(notices.len(), 2);
        let bsd = notices.iter().find(|n| n.text.contains("BSD terms")).unwrap();
        assert!(bsd.libraries.contains("sib-extra 1.0.0"));
        assert!(
            bsd.origin.contains("sib-core 1.0.0 (the same repository)"),
            "{}",
            bsd.origin
        );
        let mit = notices
            .iter()
            .find(|n| n.text.starts_with("MIT License"))
            .unwrap();
        assert!(mit.text.contains("Copyright (c) Ada Example\n"), "{}", mit.text);
        assert!(!mit.text.contains("ada@example.org"));
        assert!(mit.text.contains("Permission is hereby granted"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_library_with_no_text_to_be_found_stops_the_run() {
        let dir = scratch("missing");
        let bsd = library("orphan", "BSD-3-Clause", "", &dir);
        let err = notices(std::slice::from_ref(&bsd), std::slice::from_ref(&bsd))
            .err()
            .unwrap();
        assert!(err.contains("orphan 1.0.0 (BSD-3-Clause)"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_same_text_is_written_once_for_all_its_libraries() {
        let dir = scratch("same");
        let (one, two) = (dir.join("one"), dir.join("two"));
        for d in [&one, &two] {
            std::fs::create_dir_all(d).unwrap();
            std::fs::write(d.join("LICENSE-APACHE"), "Apache License\r\nVersion 2.0   \n").unwrap();
        }
        let libraries = vec![
            library("one", "Apache-2.0", "https://a/one", &one),
            library("two", "Apache-2.0", "https://a/two", &two),
        ];
        let notices = notices(&libraries, &libraries).unwrap();
        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0].libraries.len(), 2);
        assert_eq!(notices[0].text, "Apache License\nVersion 2.0");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mit_is_allowed_only_as_an_alternative() {
        assert!(allows_mit("MIT"));
        assert!(allows_mit("MIT OR Apache-2.0"));
        assert!(allows_mit("Apache-2.0/MIT"));
        assert!(allows_mit("(MIT OR Apache-2.0)"));
        assert!(!allows_mit("MIT-0"));
        assert!(!allows_mit("(MIT OR Apache-2.0) AND Unicode-3.0"));
        assert!(!allows_mit("BSD-3-Clause"));
    }

    #[test]
    fn the_document_lists_every_library_with_its_source_and_every_text() {
        let dir = scratch("doc");
        std::fs::write(dir.join("LICENSE"), "Some licence\n").unwrap();
        let libraries = vec![library("lib", "MPL-2.0", "https://example.org/lib", &dir)];
        let notices = notices(&libraries, &libraries).unwrap();
        let text = document(
            "9.9.9",
            &["x86_64-unknown-linux-gnu".into()],
            &libraries,
            &notices,
        );
        assert!(text.starts_with("Folitecta 9.9.9: licences of the libraries in this program"));
        assert!(
            text.contains(
                "lib 1.0.0 — MPL-2.0 — https://crates.io/crates/lib/1.0.0 — https://example.org/lib"
            )
        );
        assert!(text.contains("LICENCE TEXTS (1)"));
        assert!(text.contains("Text of: lib 1.0.0\n\nSome licence\n"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
