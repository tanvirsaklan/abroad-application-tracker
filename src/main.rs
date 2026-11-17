#![forbid(unsafe_code)]
// Release builds on Windows run as a GUI app (no console window).
#![cfg_attr(all(not(debug_assertions), windows), windows_subsystem = "windows")]

mod db;
mod error;
mod export;
mod model;
mod reminders;
mod search;
mod sort;
mod stats;
mod store;
mod ui;

use dioxus::desktop::tao::window::Icon;
use dioxus::desktop::{Config, LogicalSize, WindowBuilder};

const CSS: &str = include_str!("../assets/style.css");

fn load_icon() -> Option<Icon> {
    let image = image::load_from_memory_with_format(
        include_bytes!("../assets/icon.ico"),
        image::ImageFormat::Ico,
    )
    .ok()?
    .into_rgba8();
    let (width, height) = image.dimensions();
    Icon::from_rgba(image.into_raw(), width, height).ok()
}

fn main() {
    let mut window = WindowBuilder::new()
        .with_title(ui::APP_TITLE)
        .with_inner_size(LogicalSize::new(1400.0, 950.0))
        .with_min_inner_size(LogicalSize::new(980.0, 680.0));
    if let Some(icon) = load_icon() {
        window = window.with_window_icon(Some(icon));
    }

    let config = Config::new()
        .with_window(window)
        .with_menu(None)
        .with_background_color((243, 245, 251, 255))
        .with_custom_head(format!("<style>{CSS}</style>"));

    dioxus::LaunchBuilder::desktop().with_cfg(config).launch(ui::App);
}
