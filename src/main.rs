#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! RustPaint — a modern, cross-platform take on MS Paint.
//!
//! Everything is drawn with egui: no image assets, no platform specific UI
//! code. The same binary builds and behaves the same on Windows, macOS and
//! Linux.

mod app;
mod assets;
mod doc;
mod filedlg;
mod fonts;
mod geom;
mod history;
mod icons;
mod ir;
mod mask;
mod native;
mod paint;
mod prefs;
mod text;
mod theme;
mod tools;
mod widgets;

use eframe::egui;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([940.0, 640.0])
            .with_title("RustPaint")
            .with_app_id("rustpaint")
            .with_icon(icons::app_icon()),
        vsync: true,
        multisampling: 0,
        depth_buffer: 0,
        stencil_buffer: 0,
        ..Default::default()
    };

    eframe::run_native(
        "RustPaint",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}
