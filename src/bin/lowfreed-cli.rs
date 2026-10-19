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
    let via = Via::open(def.vendor_id, def.product_id)?;
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

fn info(via: &Via) -> Result<()> {
    let i = &via.info;
    println!("Clavier      : {} ({:04X}:{:04X}, révision {:04X})", i.product, i.vendor_id, i.product_id, i.release);
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
