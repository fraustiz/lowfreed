//! Définition VIA du clavier (JSON) et géométrie des touches (format KLE).

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::Value;

/// Définition VIA du Flow 2 100 % ANSI fournie par Lofree.
pub const FLOW2_JSON: &str = include_str!("../assets/flow2_100_ansi.json");

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawDefinition {
    name: String,
    vendor_id: String,
    product_id: String,
    matrix: Matrix,
    #[serde(default)]
    custom_keycodes: Vec<CustomKeycode>,
    layouts: RawLayouts,
}

#[derive(Deserialize, Clone, Copy)]
pub struct Matrix {
    pub rows: u8,
    pub cols: u8,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CustomKeycode {
    #[serde(default)]
    short_name: Option<String>,
    name: String,
}

#[derive(Deserialize)]
struct RawLayouts {
    keymap: Vec<Vec<Value>>,
}

/// Position d'une touche, en unités de touche (1u = largeur d'une lettre).
#[derive(Clone, Copy, Debug)]
pub struct KeyGeom {
    pub row: u8,
    pub col: u8,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Clone)]
pub struct Definition {
    pub name: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub matrix: Matrix,
    /// Noms des keycodes propres au clavier (CUSTOM(0), CUSTOM(1)…).
    pub custom_keycodes: Vec<String>,
    pub keys: Vec<KeyGeom>,
    pub width: f32,
    pub height: f32,
}

impl Definition {
    pub fn parse(json: &str) -> Result<Self> {
        let raw: RawDefinition = serde_json::from_str(json).context("JSON VIA invalide")?;
        let hex = |s: &str| {
            u16::from_str_radix(s.trim_start_matches("0x").trim_start_matches("0X"), 16)
                .with_context(|| format!("identifiant USB invalide : {s}"))
        };
        let keys = parse_kle(&raw.layouts.keymap)?;
        for k in &keys {
            if k.row >= raw.matrix.rows || k.col >= raw.matrix.cols {
                bail!("touche {},{} hors de la matrice", k.row, k.col);
            }
        }
        let width = keys.iter().map(|k| k.x + k.w).fold(0.0, f32::max);
        let height = keys.iter().map(|k| k.y + k.h).fold(0.0, f32::max);
        Ok(Self {
            name: raw.name,
            vendor_id: hex(&raw.vendor_id)?,
            product_id: hex(&raw.product_id)?,
            matrix: raw.matrix,
            custom_keycodes: raw
                .custom_keycodes
                .into_iter()
                .map(|c| c.short_name.unwrap_or(c.name))
                .collect(),
            keys,
            width,
            height,
        })
    }

    pub fn flow2() -> Self {
        Self::parse(FLOW2_JSON).expect("définition Flow 2 intégrée invalide")
    }

    /// Index d'une touche dans le keymap à plat (couche, ligne, colonne).
    pub fn index(&self, layer: usize, row: u8, col: u8) -> usize {
        let m = self.matrix;
        (layer * m.rows as usize + row as usize) * m.cols as usize + col as usize
    }
}

/// Lit un layout Keyboard Layout Editor : chaque objet modifie la touche suivante,
/// chaque chaîne est une touche dont la première étiquette vaut « ligne,colonne ».
fn parse_kle(rows: &[Vec<Value>]) -> Result<Vec<KeyGeom>> {
    let mut keys = Vec::new();
    let mut y = 0.0f32;
    for row in rows {
        let (mut x, mut w, mut h) = (0.0f32, 1.0f32, 1.0f32);
        for item in row {
            match item {
                Value::Object(props) => {
                    let f = |k: &str| props.get(k).and_then(Value::as_f64).map(|v| v as f32);
                    x += f("x").unwrap_or(0.0);
                    y += f("y").unwrap_or(0.0);
                    w = f("w").unwrap_or(w);
                    h = f("h").unwrap_or(h);
                }
                Value::String(label) => {
                    let first = label.split('\n').next().unwrap_or("");
                    let (r, c) = first
                        .split_once(',')
                        .with_context(|| format!("étiquette de touche invalide : {label:?}"))?;
                    keys.push(KeyGeom {
                        row: r.trim().parse()?,
                        col: c.trim().parse()?,
                        x,
                        y,
                        w,
                        h,
                    });
                    x += w;
                    w = 1.0;
                    h = 1.0;
                }
                _ => bail!("élément de layout inattendu : {item}"),
            }
        }
        y += 1.0;
    }
    Ok(keys)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flow2_layout() {
        let d = Definition::flow2();
        assert_eq!((d.vendor_id, d.product_id), (0x388D, 0x0003));
        assert_eq!(d.keys.len(), 100);
        assert_eq!(d.custom_keycodes, vec!["TRIGGER"]);
        // Touche « 3,12 » placée sur la rangée des fonctions, entre F12 et Suppr.
        let k = d.keys.iter().find(|k| (k.row, k.col) == (3, 12)).unwrap();
        assert_eq!((k.x, k.y), (13.0, 0.0));
        assert_eq!((d.width, d.height), (19.0, 6.0));
    }
}
