#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;

use std::sync::Arc;

use eframe::egui;
use lowfreed::layout::Definition;

fn main() -> eframe::Result {
    // `lowfreed <définition.json>` charge une autre définition VIA que celle du Flow 2.
    let def = match std::env::args().nth(1) {
        Some(path) => {
            let json = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path} : {e}"));
            Definition::parse(&json).unwrap_or_else(|e| panic!("{path} : {e:#}"))
        }
        None => Definition::flow2(),
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(format!("Lowfreed — {}", def.name))
            .with_inner_size([1240.0, 860.0])
            .with_min_inner_size([900.0, 600.0])
            .with_icon(Arc::new(icon())),
        ..Default::default()
    };
    eframe::run_native("Lowfreed", options, Box::new(|cc| Ok(Box::new(app::App::new(cc, def)))))
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
