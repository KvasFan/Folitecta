//! Hardware acceleration of the window's web view.
//!
//! The web views of Windows (WebView2) and macOS (WKWebView) composite on the
//! graphics card by themselves. On Linux, WebKitGTK does when its settings
//! allow it and its renderer can share buffers with the windowing system:
//! the window toolkit turns the shared-buffer renderer off on every Wayland
//! session (for the drivers where it shows blank windows), which leaves the
//! pages to be composited by the processor. Folitecta turns it on only where
//! it is known to be safe:
//!
//! - `FOLITECTA_GPU=off` (or `0`): no hardware acceleration at all;
//! - `FOLITECTA_GPU=on` (or `1`): hardware acceleration, shared buffers on;
//! - `FOLITECTA_GPU=no-shared-buffers`: hardware acceleration without them;
//! - otherwise: on when the machine has a render node of a graphics card in
//!   `/dev/dri`, its driver is not NVIDIA's (the known cause of blank
//!   windows: its module loaded, or the driver of a render node) and it is
//!   not a virtual machine (the processor's hypervisor flag, a hypervisor
//!   named in `/sys/hypervisor/type`, the driver of a virtual graphics card,
//!   or the maker or product of a known hypervisor); else the window
//!   toolkit's own setting, as in Folitecta 0.2.0.
//!
//! The window stays a window of the session's own system (Wayland or X11),
//! as before: the toolkit's request for X11 came after the window system had
//! been chosen and changed nothing; it is no longer made. What was chosen
//! and what the web process really loaded (graphics drivers) is reported by
//! [`report`], for the performance step of the self-test.

use std::sync::OnceLock;

/// The choice made at start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum Acceleration {
    /// No hardware acceleration (asked by `FOLITECTA_GPU=off`).
    Off,
    /// Hardware acceleration, without the shared-buffer renderer.
    NoSharedBuffers,
    /// Hardware acceleration with the shared-buffer renderer.
    On,
    /// The window toolkit's own setting, as in Folitecta 0.2.0 (the shared
    /// buffers off on Wayland, the web view's policy left as it is): where
    /// turning them on is not known to be safe.
    Toolkit,
}

/// What the machine tells about its graphics (Linux).
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Machine {
    /// The render nodes of graphics cards (`/dev/dri/renderD…`).
    pub render_nodes: Vec<String>,
    /// The kernel driver of each render node.
    pub drivers: Vec<String>,
    /// NVIDIA's kernel module is loaded (`/proc/driver/nvidia`, `/sys/module/nvidia`).
    pub nvidia_module: bool,
    /// The processor runs under a hypervisor (its `hypervisor` flag).
    pub hypervisor: bool,
    /// The hypervisor the kernel names (`/sys/hypervisor/type`, Xen).
    pub hypervisor_type: String,
    /// The maker and the product of the machine (`/sys/class/dmi/id`).
    pub vendor: String,
    pub product: String,
}

impl Machine {
    /// Read from the system.
    #[cfg(target_os = "linux")]
    fn read() -> Machine {
        let read = |p: &str| {
            std::fs::read_to_string(p)
                .map(|s| s.trim().to_string())
                .unwrap_or_default()
        };
        let mut nodes: Vec<String> = std::fs::read_dir("/dev/dri")
            .map(|dir| {
                dir.flatten()
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .filter(|n| n.starts_with("renderD"))
                    .collect()
            })
            .unwrap_or_default();
        nodes.sort();
        let drivers = nodes
            .iter()
            .filter_map(|n| std::fs::read_link(format!("/sys/class/drm/{n}/device/driver")).ok())
            .filter_map(|l| l.file_name().map(|f| f.to_string_lossy().to_string()))
            .collect();
        Machine {
            render_nodes: nodes,
            drivers,
            nvidia_module: std::path::Path::new("/proc/driver/nvidia").exists()
                || std::path::Path::new("/sys/module/nvidia").exists(),
            hypervisor: read("/proc/cpuinfo")
                .lines()
                .find(|l| l.starts_with("flags"))
                .is_some_and(|l| l.split_whitespace().any(|f| f == "hypervisor")),
            hypervisor_type: read("/sys/hypervisor/type"),
            vendor: read("/sys/class/dmi/id/sys_vendor"),
            product: read("/sys/class/dmi/id/product_name"),
        }
    }

    /// Read from the system (nothing to read elsewhere than on Linux).
    #[cfg(not(target_os = "linux"))]
    fn read() -> Machine {
        Machine::default()
    }

    /// A virtual machine: the hypervisor flag, a hypervisor the kernel
    /// names, the driver of a virtual graphics card, or the maker or product
    /// of a known hypervisor.
    fn is_virtual(&self) -> bool {
        // virtio_gpu, vmwgfx, vboxvideo, qxl, bochs-drm, cirrus.
        const DRIVERS: [&str; 6] = ["virtio", "vmwgfx", "vbox", "qxl", "bochs", "cirrus"];
        let virtual_card = self
            .drivers
            .iter()
            .any(|d| DRIVERS.iter().any(|v| d.to_ascii_lowercase().contains(v)));
        const NAMES: [&str; 12] = [
            "qemu",
            "kvm",
            "vmware",
            "virtualbox",
            "innotek",
            "xen",
            "bochs",
            "bhyve",
            "parallels",
            "virtual machine",
            "standard pc (",
            "google compute engine",
        ];
        let (vendor, product) = (
            self.vendor.to_ascii_lowercase(),
            self.product.to_ascii_lowercase(),
        );
        self.hypervisor
            || !self.hypervisor_type.trim().is_empty()
            || virtual_card
            || NAMES.iter().any(|n| vendor.contains(n) || product.contains(n))
    }

    /// The shared-buffer renderer is known to be safe here.
    fn safe(&self) -> bool {
        !self.render_nodes.is_empty()
            && !self.nvidia_module
            && !self
                .drivers
                .iter()
                .any(|d| d.to_ascii_lowercase().contains("nvidia"))
            && !self.is_virtual()
    }
}

/// The choice for what `FOLITECTA_GPU` asks (`asked`, in lower case) on this
/// system (`linux`) and machine.
fn decide(asked: &str, linux: bool, machine: &Machine) -> Acceleration {
    match asked {
        "off" | "0" | "no" | "false" => Acceleration::Off,
        "no-shared-buffers" => Acceleration::NoSharedBuffers,
        "on" | "1" | "yes" | "true" => Acceleration::On,
        _ if !linux || machine.safe() => Acceleration::On,
        _ => Acceleration::Toolkit,
    }
}

static MACHINE: OnceLock<Machine> = OnceLock::new();
static CHOICE: OnceLock<Acceleration> = OnceLock::new();
static SETTINGS: OnceLock<String> = OnceLock::new();

/// Decide before the window is made (the environment of the web process is
/// read when it starts).
pub fn choose() -> Acceleration {
    *CHOICE.get_or_init(|| {
        let asked = std::env::var("FOLITECTA_GPU")
            .unwrap_or_default()
            .to_ascii_lowercase();
        let machine = MACHINE.get_or_init(Machine::read);
        decide(&asked, cfg!(target_os = "linux"), machine)
    })
}

/// Set the environment for the choice (Linux), before any thread starts.
pub fn prepare_environment() {
    #[cfg(target_os = "linux")]
    {
        match choose() {
            Acceleration::On | Acceleration::Toolkit => {}
            Acceleration::NoSharedBuffers | Acceleration::Off => {
                // SAFETY: called at the start of main, before any other thread.
                unsafe { std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1") };
            }
        }
    }
}

/// Whether the window toolkit turns the shared buffers off on Wayland itself
/// (its own setting, as in 0.2.0).
pub fn toolkit_decides() -> bool {
    choose() == Acceleration::Toolkit
}

/// Apply the choice to the web view's settings, once the window exists.
pub fn apply(window: &dioxus::desktop::DesktopContext) {
    #[cfg(target_os = "linux")]
    {
        use dioxus::desktop::wry::WebViewExtUnix;
        use webkit2gtk::{HardwareAccelerationPolicy, SettingsExt, WebViewExt};
        let view = window.webview.webview();
        if let Some(settings) = WebViewExt::settings(&view) {
            let before = settings.hardware_acceleration_policy();
            let policy = match choose() {
                Acceleration::Off => HardwareAccelerationPolicy::Never,
                Acceleration::Toolkit => before,
                Acceleration::On | Acceleration::NoSharedBuffers => HardwareAccelerationPolicy::Always,
            };
            settings.set_hardware_acceleration_policy(policy);
            let _ = SETTINGS.set(format!(
                "policy {before:?} -> {:?}, webgl {}",
                settings.hardware_acceleration_policy(),
                settings.enables_webgl(),
            ));
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = window;
        let _ = SETTINGS.set("the web view composites on the graphics card by itself".into());
    }
}

/// The graphics libraries a process has loaded (Linux): drivers and GL.
#[cfg(target_os = "linux")]
fn graphics_libraries(pid: &str) -> Vec<String> {
    let Ok(maps) = std::fs::read_to_string(format!("/proc/{pid}/maps")) else {
        return Vec::new();
    };
    let mut out: Vec<String> = maps
        .lines()
        .filter_map(|l| l.split_whitespace().nth(5))
        .filter_map(|path| path.rsplit('/').next())
        .filter(|name| {
            let n = name.to_ascii_lowercase();
            n.contains("_dri")
                || n.starts_with("libgallium")
                || n.starts_with("libegl")
                || n.starts_with("libgl")
                || n.starts_with("libgbm")
                || n.starts_with("libvulkan")
                || n.starts_with("libdrm_")
                || n.contains("nvidia")
        })
        .map(str::to_string)
        .collect();
    out.sort();
    out.dedup();
    out
}

/// What was chosen, the web view's settings and, on Linux, the graphics
/// libraries the web processes of this program loaded.
pub fn report() -> serde_json::Value {
    // Changed only on Linux.
    #[cfg_attr(not(target_os = "linux"), allow(unused_mut))]
    let mut out = serde_json::json!({
        "choice": format!("{:?}", choose()),
        "settings": SETTINGS.get().cloned().unwrap_or_default(),
        "machine": MACHINE.get(),
    });
    #[cfg(target_os = "linux")]
    {
        let me = std::process::id().to_string();
        let mut web = Vec::new();
        if let Ok(dir) = std::fs::read_dir("/proc") {
            for e in dir.flatten() {
                let pid = e.file_name().to_string_lossy().to_string();
                if !pid.chars().all(|c| c.is_ascii_digit()) {
                    continue;
                }
                let Ok(status) = std::fs::read_to_string(format!("/proc/{pid}/status")) else {
                    continue;
                };
                let field = |key: &str| {
                    status
                        .lines()
                        .find(|l| l.starts_with(key))
                        .map(|l| l[key.len()..].trim().to_string())
                };
                if !field("Name:").is_some_and(|n| n.starts_with("WebKitWebProces")) {
                    continue;
                }
                // One of ours: this process is among its parents.
                let mut parent = field("PPid:");
                for _ in 0..6 {
                    match parent {
                        Some(p) if p == me => {
                            web.push(graphics_libraries(&pid));
                            break;
                        }
                        Some(p) if p != "0" && p != "1" => {
                            parent = std::fs::read_to_string(format!("/proc/{p}/status"))
                                .ok()
                                .and_then(|s| {
                                    s.lines()
                                        .find(|l| l.starts_with("PPid:"))
                                        .map(|l| l[5..].trim().to_string())
                                });
                        }
                        _ => break,
                    }
                }
            }
        }
        out["web_process_graphics"] = serde_json::json!(web);
        out["ui_process_graphics"] = serde_json::json!(graphics_libraries("self"));
        for var in [
            "GDK_BACKEND",
            "WEBKIT_DISABLE_DMABUF_RENDERER",
            "WEBKIT_DISABLE_COMPOSITING_MODE",
            "XDG_SESSION_TYPE",
        ] {
            out[var] = serde_json::json!(std::env::var(var).ok());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A desktop with an AMD card.
    fn desktop() -> Machine {
        Machine {
            render_nodes: vec!["renderD128".into()],
            drivers: vec!["amdgpu".into()],
            vendor: "ASUS".into(),
            product: "System Product Name".into(),
            ..Machine::default()
        }
    }

    #[test]
    fn the_shared_buffers_are_on_only_where_they_are_safe() {
        use Acceleration::*;
        assert_eq!(decide("", true, &desktop()), On);
        // Intel and other drivers of the kernel.
        let intel = Machine {
            drivers: vec!["i915".into()],
            ..desktop()
        };
        assert_eq!(decide("", true, &intel), On);
        // No render node: the toolkit's setting, as 0.2.0.
        let none = Machine {
            render_nodes: Vec::new(),
            drivers: Vec::new(),
            ..desktop()
        };
        assert_eq!(decide("", true, &none), Toolkit);
        // NVIDIA: its module loaded, or the driver of a render node.
        let module = Machine {
            nvidia_module: true,
            ..desktop()
        };
        assert_eq!(decide("", true, &module), Toolkit);
        let driver = Machine {
            render_nodes: vec!["renderD128".into(), "renderD129".into()],
            drivers: vec!["i915".into(), "nvidia".into()],
            ..desktop()
        };
        assert_eq!(decide("", true, &driver), Toolkit);
        // Virtual machines: the hypervisor flag, the maker or the product.
        let flagged = Machine {
            hypervisor: true,
            ..desktop()
        };
        assert_eq!(decide("", true, &flagged), Toolkit);
        for (vendor, product) in [
            ("QEMU", "Standard PC (Q35 + ICH9, 2009)"),
            ("innotek GmbH", "VirtualBox"),
            ("VMware, Inc.", "VMware Virtual Platform"),
            ("Microsoft Corporation", "Virtual Machine"),
            ("Xen", "HVM domU"),
        ] {
            let vm = Machine {
                drivers: vec!["virtio-pci".into()],
                vendor: vendor.into(),
                product: product.into(),
                ..desktop()
            };
            assert_eq!(decide("", true, &vm), Toolkit, "{vendor} {product}");
        }
        // The graphics card of a virtual machine that names no hypervisor.
        for driver in ["virtio_gpu", "vmwgfx", "vboxvideo", "qxl", "bochs-drm", "cirrus"] {
            let card = Machine {
                drivers: vec![driver.into()],
                vendor: "Example Cloud".into(),
                product: "vServer".into(),
                ..desktop()
            };
            assert_eq!(decide("", true, &card), Toolkit, "{driver}");
        }
        // A hypervisor the kernel names (Xen).
        let xen = Machine {
            hypervisor_type: "xen".into(),
            ..desktop()
        };
        assert_eq!(decide("", true, &xen), Toolkit);
        // A value of FOLITECTA_GPU it does not know: as if none.
        assert_eq!(decide("maybe", true, &desktop()), On);
        let virtio = Machine {
            drivers: vec!["virtio_gpu".into()],
            ..desktop()
        };
        assert_eq!(decide("maybe", true, &virtio), Toolkit);
        // What FOLITECTA_GPU asks comes first.
        assert_eq!(decide("on", true, &module), On);
        assert_eq!(decide("1", true, &none), On);
        assert_eq!(decide("off", true, &desktop()), Off);
        assert_eq!(decide("no-shared-buffers", true, &desktop()), NoSharedBuffers);
        // Windows and macOS composite on the graphics card by themselves.
        assert_eq!(decide("", false, &Machine::default()), On);
    }
}
