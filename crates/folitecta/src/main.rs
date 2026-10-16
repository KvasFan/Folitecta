#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! Folitecta desktop application.

mod app;
mod commands;
mod features;
mod files;
mod i18n;
mod icons;
mod state;
mod ui;
mod util;

use std::path::PathBuf;

use dioxus::desktop::tao::window::Icon;
use dioxus::desktop::{Config, LogicalSize, WindowBuilder, icon_from_memory};
use dioxus::prelude::*;

const CSS: &str = include_str!("../assets/style.css");
const ICON_PNG: &[u8] = include_bytes!("../assets/icon-256.png");

/// Files passed on the command line, provided to the UI through the launch context.
#[derive(Clone, Debug, Default)]
pub struct StartupFiles(pub Vec<PathBuf>);

fn print_usage() {
    println!(
        "Folitecta {}\n\nUsage: folitecta [FILE.pdf ...]\n       folitecta --features-md <output.md>\n       folitecta --version\n       folitecta --help",
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
    let files: Vec<PathBuf> = args
        .into_iter()
        .filter(|a| !a.starts_with("--"))
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
        .with_custom_head(format!("<meta charset=\"utf-8\"><style>{CSS}</style>"))
        .with_disable_context_menu(!cfg!(debug_assertions))
        .with_background_color((238, 240, 246, 255));
    if let Some(icon) = icon {
        config = config.with_icon(icon);
    }
    LaunchBuilder::desktop()
        .with_cfg(config)
        .with_context(StartupFiles(files))
        .launch(app::App);
}
