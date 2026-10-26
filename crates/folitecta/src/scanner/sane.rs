//! SANE through its command `scanimage`: the devices it lists, the options
//! of a device (`-A`), a picture of the flatbed on its output, the sheets
//! of a feeder as files (`--batch`).

use std::path::PathBuf;
use std::process::{Command, Stdio};

use folitecta_core::ScannedPicture;

use super::{Capabilities, Device, ScanSettings, is_feeder};

fn scanimage() -> Command {
    Command::new("scanimage")
}

fn run(cmd: &mut Command) -> Result<std::process::Output, String> {
    cmd.stdin(Stdio::null()).output().map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => "scanimage (SANE) is not installed".to_string(),
        _ => e.to_string(),
    })
}

/// The lines of `scanimage -f "%d\t%v %m%n"`.
pub(super) fn parse_devices(out: &str) -> Vec<Device> {
    out.lines()
        .filter_map(|l| {
            let (id, name) = l.split_once('\t')?;
            let id = id.trim();
            (!id.is_empty()).then(|| Device {
                id: id.to_string(),
                name: if name.trim().is_empty() {
                    id.to_string()
                } else {
                    name.trim().to_string()
                },
            })
        })
        .collect()
}

pub fn devices() -> Result<Vec<Device>, String> {
    let out = run(scanimage().args(["-f", "%d\t%v %m%n"]))?;
    Ok(parse_devices(&String::from_utf8_lossy(&out.stdout)))
}

/// The values of a list option of `scanimage -A` (`--mode Lineart|Gray|Color [Color]`).
fn list_values(line: &str) -> Vec<String> {
    let rest = line.split_whitespace().skip(1).collect::<Vec<_>>().join(" ");
    let rest = rest.split(" [").next().unwrap_or_default();
    rest.split('|')
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .collect()
}

/// The sources, modes and resolutions in the output of `scanimage -A`.
pub(super) fn parse_capabilities(out: &str) -> Capabilities {
    let mut caps = Capabilities::default();
    for line in out.lines().map(str::trim) {
        if line.starts_with("--source ") {
            caps.sources = list_values(line);
        } else if line.starts_with("--mode ") {
            caps.modes = list_values(line);
        } else if line.starts_with("--resolution ") {
            let values: String = line
                .trim_start_matches("--resolution ")
                .split(" [")
                .next()
                .unwrap_or_default()
                .replace("dpi", "");
            caps.resolutions = if let Some((a, b)) = values.split_once("..") {
                // A range: the usual steps inside it.
                let lo: u32 = a.trim().parse().unwrap_or(75);
                let hi: u32 = b
                    .split_whitespace()
                    .next()
                    .unwrap_or("600")
                    .parse()
                    .unwrap_or(600);
                [75, 100, 150, 200, 300, 400, 600, 1200]
                    .into_iter()
                    .filter(|r| *r >= lo && *r <= hi)
                    .collect()
            } else {
                values.split('|').filter_map(|v| v.trim().parse().ok()).collect()
            };
        }
    }
    caps
}

pub fn capabilities(device: &str) -> Result<Capabilities, String> {
    let out = run(scanimage().args(["-d", device, "-A"]))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(parse_capabilities(&String::from_utf8_lossy(&out.stdout)))
}

fn settings_args(s: &ScanSettings) -> Vec<String> {
    let mut args = vec!["--format=png".to_string()];
    if !s.device.is_empty() {
        args.extend(["-d".into(), s.device.clone()]);
    }
    if !s.source.is_empty() {
        args.push(format!("--source={}", s.source));
    }
    if !s.mode.is_empty() {
        args.push(format!("--mode={}", s.mode));
    }
    if s.dpi > 0 {
        args.push(format!("--resolution={}", s.dpi));
    }
    args
}

pub fn scan(s: &ScanSettings) -> Result<Vec<ScannedPicture>, String> {
    let dpi = s.dpi as f32;
    if !is_feeder(&s.source) {
        let out = run(scanimage().args(settings_args(s)))?;
        if !out.status.success() || out.stdout.is_empty() {
            return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
        }
        return Ok(vec![ScannedPicture {
            data: out.stdout,
            dpi,
        }]);
    }
    // The sheets of the feeder, one file each, in a folder of this scan.
    let dir = std::env::temp_dir().join(format!("folitecta-scan-{}-{}", std::process::id(), stamp()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let pattern = dir.join("page%04d.png");
    let out = run(scanimage()
        .args(settings_args(s))
        .arg(format!("--batch={}", pattern.display())));
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|r| r.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    files.sort();
    let pictures: Vec<ScannedPicture> = files
        .iter()
        .filter_map(|f| std::fs::read(f).ok())
        .map(|data| ScannedPicture { data, dpi })
        .collect();
    let _ = std::fs::remove_dir_all(&dir);
    let out = out?;
    if pictures.is_empty() {
        let why = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if why.is_empty() {
            "no sheet was scanned".into()
        } else {
            why
        });
    }
    Ok(pictures)
}

fn stamp() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn devices_and_options_are_read() {
        let devices = parse_devices("genesys:libusb:001:004\tCanon LiDE 220\nnet:host:escl:x\t\n\n");
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].name, "Canon LiDE 220");
        assert_eq!(devices[1].name, "net:host:escl:x");
        let caps = parse_capabilities(
            "All options specific to device `x':\n  Scan mode:\n    --mode Lineart|Gray|Color [Color]\n        Selects the scan mode.\n    --source Flatbed|ADF|ADF Duplex [Flatbed]\n    --resolution 75|150|300|600dpi [75]\n",
        );
        assert_eq!(caps.modes, ["Lineart", "Gray", "Color"]);
        assert_eq!(caps.sources, ["Flatbed", "ADF", "ADF Duplex"]);
        assert_eq!(caps.resolutions, [75, 150, 300, 600]);
        let range = parse_capabilities("    --resolution 50..1200dpi (in steps of 1) [75]\n");
        assert_eq!(range.resolutions, [75, 100, 150, 200, 300, 400, 600, 1200]);
        assert!(is_feeder("ADF Duplex") && is_feeder("Automatic Document Feeder") && !is_feeder("Flatbed"));
    }
}
