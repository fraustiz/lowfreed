#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod tray;

use std::sync::{mpsc, Arc};

use eframe::egui;
use lowfreed::{
    layout::Definition,
    slider::Engine,
    system::{self, Settings},
};

fn main() -> eframe::Result {
    // `lowfreed [--tray] [définition.json]` : `--tray` démarre fenêtre masquée (lancement
    // au démarrage de Windows) ; un JSON charge une autre définition VIA que celle du Flow 2.
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let start_hidden = args.iter().any(|a| a == "--tray");
    args.retain(|a| a != "--tray" && !a.starts_with("-psn"));
    let def = match args.first().filter(|p| !p.starts_with('-')) {
        Some(path) => {
            let json = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path} : {e}"));
            Definition::parse(&json).unwrap_or_else(|e| panic!("{path} : {e:#}"))
        }
        None => Definition::flow2(),
    };
    // Lowfreed tourne déjà (zone de notification) : il vient d'afficher sa fenêtre.
    let Some(show_signal) = system::single_instance() else { return Ok(()) };

    let settings = Settings::load();
    let engine = Engine::start(def.vendor_id, def.product_id, def.matrix.rows, def.matrix.cols, settings.slider);
    let icon = icon();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(format!("Lowfreed — {}", def.name))
            .with_inner_size([1240.0, 860.0])
            .with_min_inner_size([900.0, 600.0])
            .with_icon(Arc::new(icon.clone())),
        ..Default::default()
    };
    eframe::run_native(
        "Lowfreed",
        options,
        Box::new(move |cc| {
            let (tx, rx) = mpsc::channel();
            tray::spawn(show_signal, icon, tx, cc.egui_ctx.clone());
            Ok(Box::new(app::App::new(cc, def, engine, settings, rx, start_hidden)))
        }),
    )
}

/// Icône générée : touche orange arrondie.
fn icon() -> egui::IconData {
    const N: u32 = 64;
    let mut rgba = Vec::with_capacity((N * N * 4) as usize);
    for y in 0..N {
        for x in 0..N {
            let (dx, dy) = ((x as f32 - 31.5).abs() - 20.0, (y as f32 - 31.5).abs() - 20.0);
            let d = (dx.max(0.0).powi(2) + dy.max(0.0).powi(2)).sqrt() - 10.0 + dx.max(dy).min(0.0);
            let alpha = (0.5 - d).clamp(0.0, 1.0);
            let inner = d < -5.0 && (y as f32) < 40.0;
            let (r, g, b) = if inner { (255, 140, 50) } else { (255, 106, 0) };
            rgba.extend([r, g, b, (alpha * 255.0) as u8]);
        }
    }
    egui::IconData { rgba, width: N, height: N }
}
