#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! Folitecta desktop application.

mod app;
mod clipboard;
mod commands;
mod features;
mod files;
mod i18n;
mod icons;
mod selftest;
mod state;
mod theme;
mod ui;
mod util;

use std::path::PathBuf;

use dioxus::desktop::tao::window::Icon;
use dioxus::desktop::{Config, LogicalSize, WindowBuilder, icon_from_memory};
use dioxus::prelude::*;

const ICON_PNG: &[u8] = include_bytes!("../assets/icon-256.png");

/// Files passed on the command line, provided to the UI through the launch context.
#[derive(Clone, Debug, Default)]
pub struct StartupFiles(pub Vec<PathBuf>);

/// Interface self-test settings (`--ui-selftest <dir>`), if requested.
#[derive(Clone, Debug, Default)]
pub struct SelfTest(pub Option<selftest::SelfTestConfig>);

fn print_usage() {
    println!(
        "Folitecta {}\n\nUsage: folitecta [FILE.pdf ...]\n       folitecta --features-md <output.md>\n       folitecta --ui-selftest <output-dir>\n       folitecta --version\n       folitecta --help",
        folitecta_core::VERSION
    );
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("Folitecta {}", folitecta_core::VERSION);
        return;
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_usage();
        return;
    }
    if let Some(i) = args.iter().position(|a| a == "--features-md") {
        let Some(path) = args.get(i + 1) else {
            eprintln!("--features-md needs an output path");
            std::process::exit(2);
        };
        match features::write_feature_matrix(path) {
            Ok(stats) => println!("wrote {path}: {stats}"),
            Err(e) => {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        return;
    }
    let selftest = args
        .iter()
        .position(|a| a == "--ui-selftest")
        .map(|i| selftest::SelfTestConfig {
            dir: PathBuf::from(args.get(i + 1).cloned().unwrap_or_else(|| "ui-selftest".into())),
        });
    if let Some(cfg) = &selftest {
        // Keep the user's settings untouched while the self-test changes options.
        // SAFETY: called before any other thread is started.
        unsafe { std::env::set_var("FOLITECTA_CONFIG_DIR", cfg.dir.join("config")) };
    }
    let skip_value = selftest.as_ref().map(|c| c.dir.display().to_string());
    let files: Vec<PathBuf> = args
        .into_iter()
        .filter(|a| !a.starts_with("--") && Some(a) != skip_value.as_ref())
        .map(PathBuf::from)
        .collect();

    let icon = icon_from_memory::<Icon>(ICON_PNG).ok();
    let window = WindowBuilder::new()
        .with_title("Folitecta")
        .with_inner_size(LogicalSize::new(1440.0, 900.0))
        .with_min_inner_size(LogicalSize::new(960.0, 620.0))
        .with_window_icon(icon.clone());
    let mut config = Config::new()
        .with_window(window)
        .with_menu(None::<dioxus::desktop::muda::Menu>)
        .with_custom_head(format!("<meta charset=\"utf-8\"><style>{}</style>", theme::CSS))
        .with_disable_context_menu(!cfg!(debug_assertions))
        .with_background_color((238, 240, 246, 255));
    if let Some(icon) = icon {
        config = config.with_icon(icon);
    }
    LaunchBuilder::desktop()
        .with_cfg(config)
        .with_context(StartupFiles(files))
        .with_context(SelfTest(selftest))
        .launch(app::App);
}
