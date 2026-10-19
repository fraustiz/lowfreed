//! Détection de la couche de base en service (Mac ou Windows sur le Flow 2).
//!
//! VIA ne dit pas quelle couche est active. On compare donc chaque touche que Windows reçoit
//! (hook clavier, code de scan indépendant de la disposition AZERTY/QWERTY) avec la matrice
//! du Flow 2 au même instant : une touche dont le keycode diffère entre les couches de base
//! (Alt/Win en bas à gauche, F11/F12…) révèle la couche en service.

use std::{
    collections::HashSet,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, Sender},
        Arc, Mutex, OnceLock,
    },
    time::{Duration, Instant},
};

use crate::via::Via;

/// Code de scan (jeu 1) avec `0xE000` pour les touches étendues.
type Scan = u16;

pub(crate) struct KeyDown {
    pub at: Instant,
    pub scan: Scan,
}

static EVENTS: OnceLock<Mutex<Sender<KeyDown>>> = OnceLock::new();

pub(crate) fn install(tx: Sender<KeyDown>) {
    let _ = EVENTS.set(Mutex::new(tx));
}

/// Appelée par le hook clavier pour chaque appui physique (ne bloque pas).
pub(crate) fn on_key_down(scan_code: u32, extended: bool) {
    if let Some(tx) = EVENTS.get() {
        let scan = (scan_code as u16 & 0xFF) | if extended { 0xE000 } else { 0 };
        let _ = tx.lock().unwrap().send(KeyDown { at: Instant::now(), scan });
    }
}

/// Au-delà, l'appui est trop ancien pour que la matrice le montre encore.
const MAX_LAG: Duration = Duration::from_millis(80);

pub(crate) fn run(
    rx: Receiver<KeyDown>,
    active: Arc<Mutex<Option<u8>>>,
    dirty: Arc<AtomicBool>,
    vendor_id: u16,
    product_id: u16,
    rows: u8,
    cols: u8,
) {
    let mut model: Option<(Via, Model)> = None;
    for key in rx {
        if key.at.elapsed() > MAX_LAG {
            continue;
        }
        if dirty.swap(false, Ordering::Relaxed) {
            model = None;
        }
        if model.is_none() {
            model = Via::open(vendor_id, product_id)
                .ok()
                .and_then(|via| Model::read(&via, rows, cols).map(|m| (via, m)));
        }
        let Some((via, m)) = &model else { continue };
        if !m.interesting.contains(&normalize(key.scan)) {
            continue; // touche identique sur toutes les couches de base : rien à apprendre
        }
        let Ok(state) = via.matrix_state(rows, cols) else {
            model = None;
            continue;
        };
        let pressed: Vec<usize> = state.iter().enumerate().filter(|(_, on)| **on).map(|(i, _)| i).collect();
        if let Some(layer) = m.infer(&pressed, key.scan) {
            *active.lock().unwrap() = Some(layer);
        }
    }
}

/// Keymap utile à la détection.
struct Model {
    keymap: Vec<u16>,
    per_layer: usize,
    /// Couches de base : 0 et les cibles de TO/DF/PDF.
    bases: Vec<u8>,
    /// Codes de scan qui diffèrent entre couches de base à une même position.
    interesting: HashSet<Scan>,
}

impl Model {
    fn read(via: &Via, rows: u8, cols: u8) -> Option<Self> {
        let layers = via.layer_count().ok()?;
        let keymap = via.read_keymap(layers, rows, cols).ok()?;
        Some(Self::new(keymap, rows as usize * cols as usize, layers))
    }

    fn new(keymap: Vec<u16>, per_layer: usize, layers: u8) -> Self {
        let mut bases: Vec<u8> = vec![0];
        for &k in &keymap {
            // TO(n), DF(n), PDF(n)
            if let 0x5200..=0x521F | 0x5240..=0x525F | 0x52E0..=0x52FF = k {
                let l = (k & 0x1F) as u8;
                if l < layers && !bases.contains(&l) {
                    bases.push(l);
                }
            }
        }
        let mut interesting = HashSet::new();
        for p in 0..per_layer {
            let scans: Vec<Option<Scan>> = bases.iter().map(|&l| scan_of(keymap[l as usize * per_layer + p])).collect();
            if scans.iter().any(|s| *s != scans[0]) {
                interesting.extend(scans.into_iter().flatten().map(normalize));
            }
        }
        Self { keymap, per_layer, bases, interesting }
    }

    fn at(&self, layer: u8, pos: usize) -> u16 {
        self.keymap[layer as usize * self.per_layer + pos]
    }

    /// Couche de base cohérente avec la touche reçue, si une seule l'est.
    fn infer(&self, pressed: &[usize], scan: Scan) -> Option<u8> {
        if pressed.is_empty() {
            return None;
        }
        // Une touche de couche (Fn…) est enfoncée : couche momentanée, pas une couche de base.
        let layer_key = |k: u16| matches!(k, 0x4000..=0x52FF);
        if pressed.iter().any(|&p| self.bases.iter().any(|&l| layer_key(self.at(l, p)))) {
            return None;
        }
        let scan = normalize(scan);
        let matching: Vec<u8> = self
            .bases
            .iter()
            .copied()
            .filter(|&l| pressed.iter().any(|&p| scan_of(self.at(l, p)).map(normalize) == Some(scan)))
            .collect();
        match matching[..] {
            [layer] => Some(layer),
            _ => None,
        }
    }
}

/// Maj droite arrive avec ou sans le bit « étendue » selon les versions de Windows.
fn normalize(scan: Scan) -> Scan {
    if scan & 0xFF == 0x36 { 0x36 } else { scan }
}

/// Code de scan que Windows reçoit pour un keycode basique (indépendant de la disposition).
fn scan_of(keycode: u16) -> Option<Scan> {
    const LETTERS: [u16; 26] = [
        0x1E, 0x30, 0x2E, 0x20, 0x12, 0x21, 0x22, 0x23, 0x17, 0x24, 0x25, 0x26, 0x32, 0x31, 0x18, 0x19, 0x10,
        0x13, 0x1F, 0x14, 0x16, 0x2F, 0x11, 0x2D, 0x15, 0x2C,
    ];
    let e = |s: u16| Some(0xE000 | s);
    match keycode {
        0x04..=0x1D => Some(LETTERS[(keycode - 0x04) as usize]),
        0x1E..=0x27 => Some(keycode - 0x1E + 0x02), // 1…9, 0
        0x28 => Some(0x1C),
        0x29 => Some(0x01),
        0x2A => Some(0x0E),
        0x2B => Some(0x0F),
        0x2C => Some(0x39),
        0x2D => Some(0x0C),
        0x2E => Some(0x0D),
        0x2F => Some(0x1A),
        0x30 => Some(0x1B),
        0x31 | 0x32 => Some(0x2B),
        0x33 => Some(0x27),
        0x34 => Some(0x28),
        0x35 => Some(0x29),
        0x36 => Some(0x33),
        0x37 => Some(0x34),
        0x38 => Some(0x35),
        0x39 => Some(0x3A),
        0x3A..=0x43 => Some(keycode - 0x3A + 0x3B), // F1…F10
        0x44 => Some(0x57),
        0x45 => Some(0x58),
        0x46 => e(0x37),
        0x47 => Some(0x46),
        0x49 => e(0x52),
        0x4A => e(0x47),
        0x4B => e(0x49),
        0x4C => e(0x53),
        0x4D => e(0x4F),
        0x4E => e(0x51),
        0x4F => e(0x4D),
        0x50 => e(0x4B),
        0x51 => e(0x50),
        0x52 => e(0x48),
        0x65 => e(0x5D),
        0xA8 => e(0x20),
        0xA9 => e(0x30),
        0xAA => e(0x2E),
        0xAB => e(0x19),
        0xAC => e(0x10),
        0xAD => e(0x24),
        0xAE => e(0x22),
        0xB1 => e(0x6C),
        0xB2 => e(0x21),
        0xB3 => e(0x6B),
        0xB4 => e(0x65),
        0xB5 => e(0x32),
        0xB6 => e(0x6A),
        0xB7 => e(0x69),
        0xB8 => e(0x68),
        0xB9 => e(0x67),
        0xBA => e(0x66),
        0xE0 => Some(0x1D),
        0xE1 => Some(0x2A),
        0xE2 => Some(0x38),
        0xE3 => e(0x5B),
        0xE4 => e(0x1D),
        0xE5 => Some(0x36),
        0xE6 => e(0x38),
        0xE7 => e(0x5C),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Keymap réduit : 2 positions × 4 couches, comme le Flow 2 (0 Mac, 1 Fn, 2 Windows).
    fn model() -> Model {
        // pos 0 : Alt sur Mac / Win sur Windows ; pos 1 : Fn (MO) ; pos 2 : A partout.
        let keymap = vec![
            0xE2, 0x5221, 0x04, // couche 0
            0x01, 0x5202, 0x01, // couche 1 : TO(2)
            0xE3, 0x5223, 0x04, // couche 2
            0x01, 0x5200, 0x01, // couche 3 : TO(0)
        ];
        Model::new(keymap, 3, 4)
    }

    #[test]
    fn infers_base_layer() {
        let m = model();
        assert_eq!(m.bases, vec![0, 2]);
        assert_eq!(m.infer(&[0], 0x38), Some(0)); // Alt reçu : couche Mac
        assert_eq!(m.infer(&[0], 0xE05B), Some(2)); // Win reçu : couche Windows
        assert_eq!(m.infer(&[2], 0x1E), None); // A : identique partout
        assert_eq!(m.infer(&[0, 1], 0x38), None); // Fn enfoncé : couche momentanée
        assert!(!m.interesting.contains(&0x1E));
    }
}
