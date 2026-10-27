//! Outil en ligne de commande : diagnostic, lecture du keymap, sauvegarde et restauration.

use std::{env, fs, process::ExitCode};

use anyhow::{bail, Context, Result};
use lowfreed::{backup::Backup, keycodes, layout::Definition, macros, via::Via};

const USAGE: &str = "\
Usage : lowfreed-cli [--definition <via.json>] <commande>

Commandes :
  info                 identité du clavier, protocole, couches, macros, rétroéclairage
  dump [couche]        keymap (toutes les couches ou une seule)
  macros               contenu des macros
  set <c> <l> <col> <kc>  assigne un keycode (ex. set 1 0 1 KC_F1)
  slider <action> [s]  remplace le slider pendant s secondes (volume, none/unbind,
                       scroll, hscroll, zoom, tabs, tracks, arrows) et affiche chaque cran
  light <0-255> [0|1]  règle la luminosité et le mode (0=fixe, 1=respiration)
  backup <fichier>     sauvegarde keymap + macros (format VIA .layout.json)
  restore <fichier>    réécrit keymap + macros depuis une sauvegarde";

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("erreur : {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(mut args: Vec<String>) -> Result<()> {
    let def = match args.iter().position(|a| a == "--definition") {
        Some(i) => {
            let path = args.get(i + 1).context("--definition attend un fichier")?.clone();
            args.drain(i..i + 2);
            Definition::parse(&fs::read_to_string(&path).with_context(|| path.clone())?)?
        }
        None => Definition::flow2(),
    };
    let Some(command) = args.first().map(String::as_str) else {
        println!("{USAGE}");
        return Ok(());
    };
    let via = match Via::open(def.vendor_id, def.product_id) {
        Ok(v) => v,
        Err(e) => {
            if let Ok(api) = hidapi::HidApi::new() {
                if api.device_list().any(|d| {
                    d.product_string()
                        .map(|p| p.contains("Flow2") || p.contains("Lofree"))
                        .unwrap_or(false)
                }) {
                    bail!(
                        "Flow 2 détecté en Bluetooth. Le protocole VIA n'est pas exposé en Bluetooth par le firmware : \
                         branchez le câble USB ou le récepteur 2.4G pour utiliser lowfreed."
                    );
                }
            }
            return Err(e);
        }
    };
    match command {
        "info" => info(&via),
        "dump" => dump(&via, &def, args.get(1).map(|l| l.parse()).transpose()?),
        "macros" => {
            let buf = via.read_macro_buffer(via.macro_buffer_size()?)?;
            for (i, m) in macros::decode_buffer(&buf, via.macro_count()? as usize).iter().enumerate() {
                println!("M{i:<2} {m}");
            }
            Ok(())
        }
        "set" => {
            let [layer, row, col, code] = args.get(1..5).context("set attend : couche ligne colonne keycode")? else {
                unreachable!()
            };
            let code = keycodes::parse(code, &def.custom_keycodes)
                .with_context(|| format!("keycode inconnu « {code} »"))?;
            via.set_keycode(layer.parse()?, row.parse()?, col.parse()?, code)?;
            println!("couche {layer}, touche {row},{col} → {}", keycodes::name(code));
            Ok(())
        }
        "slider" => slider_test(&def, args.get(1).map(String::as_str), args.get(2)),
        "light" => {
            use lowfreed::via::backlight;
            let b: u8 = args.get(1).context("light attend une luminosité (0-255)")?.parse()?;
            via.custom_set(backlight::CHANNEL, backlight::BRIGHTNESS, b)?;
            if let Some(mode_str) = args.get(2) {
                let effect = match mode_str.to_lowercase().as_str() {
                    "0" | "fixe" | "static" => 0u8,
                    "1" | "respiration" | "breathing" => 1u8,
                    other => bail!("mode inconnu « {other} » (attendu: 0/fixe ou 1/respiration)"),
                };
                via.custom_set(backlight::CHANNEL, backlight::EFFECT, effect)?;
            }
            let _ = via.custom_save(backlight::CHANNEL);
            let cur_b = via.custom_get(backlight::CHANNEL, backlight::BRIGHTNESS)?;
            let cur_e = via.custom_get(backlight::CHANNEL, backlight::EFFECT)?;
            println!(
                "Rétroéclairage : luminosité {cur_b}/255 ({:.0} %), mode {}",
                cur_b as f32 / 255.0 * 100.0,
                if cur_e == 0 { "fixe" } else { "respiration" }
            );
            Ok(())
        }
        "backup" => {
            let path = args.get(1).context("backup attend un fichier")?;
            let backup = Backup::read(&via, &def)?;
            fs::write(path, serde_json::to_string_pretty(&backup)?)?;
            println!("sauvegarde écrite : {path}");
            Ok(())
        }
        "restore" => {
            let path = args.get(1).context("restore attend un fichier")?;
            let backup: Backup = serde_json::from_str(&fs::read_to_string(path)?)?;
            backup.write(&via, &def)?;
            println!("clavier restauré depuis {path}");
            Ok(())
        }
        other => bail!("commande inconnue « {other} »\n\n{USAGE}"),
    }
}

fn slider_test(def: &Definition, action: Option<&str>, secs: Option<&String>) -> Result<()> {
    use lowfreed::slider::{Action, Dir, Engine, SliderConfig};
    let action = match action.context("slider attend une action")?.to_lowercase().as_str() {
        "volume" => Action::Volume,
        "none" | "unbind" | "off" | "disabled" => Action::None,
        "scroll" => Action::Scroll,
        "hscroll" => Action::HScroll,
        "zoom" => Action::Zoom,
        "tabs" => Action::Tabs,
        "tracks" => Action::Tracks,
        "arrows" => Action::Arrows,
        other => bail!("action inconnue « {other} » (attend : volume, none/unbind, scroll, hscroll, zoom, tabs, tracks, arrows)"),
    };
    let secs: u64 = secs.map(|s| s.parse()).transpose()?.unwrap_or(60);
    if lowfreed::system::single_instance().is_none() {
        bail!("Lowfreed est déjà lancé : quittez-le (icône → Quitter) avant ce test");
    }
    let engine = Engine::start(
        def.vendor_id,
        def.product_id,
        def.matrix.rows,
        def.matrix.cols,
        SliderConfig { action, ..Default::default() },
    );
    println!("Slider → {} pendant {secs} s", action.title());
    let start = std::time::Instant::now();
    let mut seen = None;
    let mut ready = false;
    let mut layer = None;
    let mut counters = (0, 0);
    let mut last_report = std::time::Instant::now();
    while start.elapsed().as_secs() < secs {
        let st = engine.status();
        if (st.hook_keys, st.hid_volume) != counters && last_report.elapsed().as_secs() >= 1 {
            counters = (st.hook_keys, st.hid_volume);
            last_report = std::time::Instant::now();
            println!(
                "{:6.2}s  diagnostic : {} appuis vus par le hook (dont {} volume, drapeaux 0x{:X}), {} « Volume ± » lus sur le Flow 2",
                start.elapsed().as_secs_f32(),
                counters.0, st.hook_volume, st.hook_volume_flags,
                counters.1
            );
        }
        if engine.active_layer() != layer {
            layer = engine.active_layer();
            println!("{:6.2}s  couche en service : {layer:?}", start.elapsed().as_secs_f32());
        }
        let s = engine.status();
        if !ready && s.device && s.hook {
            println!("prêt (Flow 2 détecté, hook clavier actif)");
            ready = true;
        }
        if let Some(last) = s.last.filter(|l| Some(l.at) != seen) {
            seen = Some(last.at);
            println!(
                "{:6.2}s  {} {}  → {}   (total remplacés : {})",
                last.at.duration_since(start).as_secs_f32(),
                if last.from_slider { "slider" } else { "touche volume" },
                if last.dir == Dir::Up { "+" } else { "−" },
                if last.replaced { "remplacé" } else { "laissé au volume" },
                s.replaced
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    Ok(())
}

fn info(via: &Via) -> Result<()> {
    let i = &via.info;
    let transport = if i.is_wireless() {
        "2.4 GHz (dongle sans fil)"
    } else {
        "USB (câble filaire)"
    };
    println!("Clavier      : {} ({:04X}:{:04X}, révision {:04X})", i.product, i.vendor_id, i.product_id, i.release);
    println!("Connexion    : {transport}");
    println!("Protocole VIA: {}", via.protocol_version()?);
    println!("Couches      : {}", via.layer_count()?);
    println!("Macros       : {} ({} octets)", via.macro_count()?, via.macro_buffer_size()?);
    println!("Allumé depuis: {} s", via.uptime_ms()? / 1000);
    use lowfreed::via::backlight::*;
    match via.custom_get(CHANNEL, BRIGHTNESS) {
        Ok(b) => println!(
            "Rétroéclairage: luminosité {b}/255, effet {}",
            via.custom_get(CHANNEL, EFFECT)?
        ),
        Err(e) => println!("Rétroéclairage: indisponible ({e})"),
    }
    Ok(())
}

fn dump(via: &Via, def: &Definition, only: Option<usize>) -> Result<()> {
    let layers = via.layer_count()?;
    let keymap = via.read_keymap(layers, def.matrix.rows, def.matrix.cols)?;
    for layer in 0..layers as usize {
        if only.is_some_and(|l| l != layer) {
            continue;
        }
        println!("── Couche {layer} ──");
        for row in 0..def.matrix.rows {
            let line: Vec<String> = (0..def.matrix.cols)
                .map(|col| {
                    let code = keymap[def.index(layer, row, col)];
                    let present = def.keys.iter().any(|k| (k.row, k.col) == (row, col));
                    if present { keycodes::name(code) } else { "·".into() }
                })
                .collect();
            println!("  {row}: {}", line.join("  "));
        }
    }
    Ok(())
}
