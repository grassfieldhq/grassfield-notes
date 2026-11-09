#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// Index loops read more clearly than iterator chains for pixel and per-character arrays.
#![allow(clippy::needless_range_loop)]

mod app;
mod attach;
mod export;
mod filepicker;
mod fonts;
mod i18n;
mod icons;
mod platform;
mod safefs;
mod secrets;
mod sketch;
mod sync;
mod markdown;
mod model;
mod theme;
#[cfg(test)]
mod tests;

use eframe::egui;
use std::path::PathBuf;
use std::sync::Arc;

use fonts::find_font;

fn setup_fonts(ctx: &egui::Context) {
    let mut defs = egui::FontDefinitions::default();
    let defaults = defs
        .families
        .get(&egui::FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    let mono_defaults = defs
        .families
        .get(&egui::FontFamily::Monospace)
        .cloned()
        .unwrap_or_default();

    let mut add = |key: &str, names: &[&str]| -> bool {
        if let Some(bytes) = find_font(names) {
            defs
                .font_data
                .insert(key.to_string(), Arc::new(egui::FontData::from_owned(bytes)));
            true
        } else {
            false
        }
    };
    let regular = add("gf-regular", fonts::REGULAR);
    let bold = add("gf-bold", fonts::BOLD);
    let italic = add("gf-italic", fonts::ITALIC);
    let serif = add("gf-serif", fonts::SERIF);
    let mono = add("gf-mono", fonts::MONO);

    let with = |first: Option<&str>, rest: &Vec<String>| -> Vec<String> {
        let mut v: Vec<String> = first.map(|s| vec![s.to_string()]).unwrap_or_default();
        v.extend(rest.iter().cloned());
        v
    };
    let reg_key = regular.then_some("gf-regular");
    defs
        .families
        .insert(egui::FontFamily::Proportional, with(reg_key, &defaults));
    defs.families.insert(
        egui::FontFamily::Name("bold".into()),
        with(if bold { Some("gf-bold") } else { reg_key }, &defaults),
    );
    defs.families.insert(
        egui::FontFamily::Name("italic".into()),
        with(if italic { Some("gf-italic") } else { reg_key }, &defaults),
    );
    defs.families.insert(
        egui::FontFamily::Name("serif".into()),
        with(if serif { Some("gf-serif") } else { reg_key }, &defaults),
    );
    let mut mono_list = with(mono.then_some("gf-mono"), &mono_defaults);
    mono_list.extend(defaults.iter().cloned());
    defs.families.insert(egui::FontFamily::Monospace, mono_list);
    ctx.set_fonts(defs);
}

fn main() -> eframe::Result<()> {
    // git runs this same program to ask for the sync token (GIT_ASKPASS): answer and stop
    if std::env::var_os(sync::ASKPASS_ENV).is_some() {
        std::process::exit(sync::askpass(std::env::args().nth(1).as_deref().unwrap_or("")));
    }
    let args: Vec<String> = std::env::args().collect();
    let mut screenshot: Option<PathBuf> = None;
    // any other argument is a Markdown file to open on its own
    let mut files: Vec<PathBuf> = Vec::new();
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--screenshot" && i + 1 < args.len() {
            screenshot = Some(PathBuf::from(&args[i + 1]));
            i += 1;
        } else if let Some(p) = args[i].strip_prefix("--screenshot=") {
            screenshot = Some(PathBuf::from(p));
        } else if args[i] == "--write-icon" && i + 1 < args.len() {
            // used by the release build to make the macOS / Linux app icons
            let size = 1024;
            let ok = image::RgbaImage::from_raw(size, size, icons::rgba(0, size))
                .map(|img| img.save(&args[i + 1]).is_ok())
                .unwrap_or(false);
            std::process::exit(if ok { 0 } else { 1 });
        } else if args[i] == "--version" || args[i] == "-V" {
            println!("Grassfield Notes {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        } else if let Some(p) = args[i].strip_prefix("file://") {
            files.push(PathBuf::from(p.replace("%20", " ")));
        } else if !args[i].starts_with('-') {
            // (macOS may add its own "-psn_…" argument: anything starting with "-" is not a file)
            files.push(PathBuf::from(&args[i]));
        }
        i += 1;
    }

    if screenshot.is_some() {
        // screenshot runs work in a throwaway folder and never touch the real notes
        let tmp = std::env::temp_dir().join(format!("grassfield-shot-{}", std::process::id()));
        std::env::set_var("GRASSFIELD_DATA", tmp.join("library.json"));
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Grassfield Notes")
            .with_app_id("grassfield-notes")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([720.0, 480.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Grassfield Notes",
        options,
        Box::new(move |cc| {
            setup_fonts(&cc.egui_ctx);
            Ok(Box::new(app::GrassApp::new(&cc.egui_ctx, screenshot, files)))
        }),
    )
}
