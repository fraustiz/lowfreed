//! Sauvegarde complète (keymap + macros) au format de fichier de VIA (`.layout.json`).

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::{keycodes, layout::Definition, macros, via::Via};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Backup {
    pub name: String,
    pub vendor_product_id: u32,
    #[serde(default)]
    pub macros: Vec<String>,
    /// Une liste par couche, touches dans l'ordre de la matrice (ligne puis colonne).
    pub layers: Vec<Vec<String>>,
}

impl Backup {
    pub fn read(via: &Via, def: &Definition) -> Result<Self> {
        let layers = via.layer_count()?;
        let keymap = via.read_keymap(layers, def.matrix.rows, def.matrix.cols)?;
        let count = via.macro_count()?;
        let buf = via.read_macro_buffer(via.macro_buffer_size()?)?;
        Ok(Self::from_state(def, &keymap, macros::decode_buffer(&buf, count as usize)))
    }

    pub fn from_state(def: &Definition, keymap: &[u16], macros: Vec<String>) -> Self {
        let per_layer = def.matrix.rows as usize * def.matrix.cols as usize;
        Self {
            name: def.name.clone(),
            vendor_product_id: (def.vendor_id as u32) << 16 | def.product_id as u32,
            macros,
            layers: keymap
                .chunks(per_layer)
                .map(|l| l.iter().map(|k| keycodes::name(*k)).collect())
                .collect(),
        }
    }

    /// Keymap à plat, validé contre la définition et le nombre de couches du clavier.
    pub fn keymap(&self, def: &Definition, layers: u8) -> Result<Vec<u16>> {
        let expected = (def.vendor_id as u32) << 16 | def.product_id as u32;
        if self.vendor_product_id != expected {
            bail!(
                "sauvegarde d'un autre clavier ({:08X}, attendu {expected:08X})",
                self.vendor_product_id
            );
        }
        if self.layers.len() != layers as usize {
            bail!("{} couches dans la sauvegarde, {layers} sur le clavier", self.layers.len());
        }
        let per_layer = def.matrix.rows as usize * def.matrix.cols as usize;
        let mut out = Vec::with_capacity(per_layer * layers as usize);
        for (l, layer) in self.layers.iter().enumerate() {
            if layer.len() != per_layer {
                bail!("couche {l} : {} touches au lieu de {per_layer}", layer.len());
            }
            for name in layer {
                out.push(
                    keycodes::parse(name, &def.custom_keycodes)
                        .with_context(|| format!("couche {l} : keycode inconnu « {name} »"))?,
                );
            }
        }
        Ok(out)
    }

    /// Écrit keymap et macros dans le clavier.
    pub fn write(&self, via: &Via, def: &Definition) -> Result<()> {
        let keymap = self.keymap(def, via.layer_count()?)?;
        let count = via.macro_count()? as usize;
        let mut texts = self.macros.clone();
        texts.resize(count, String::new());
        texts.truncate(count);
        let buf = macros::encode_buffer(&texts)?;
        via.write_keymap(&keymap)?;
        via.write_macro_buffer(via.macro_buffer_size()?, &buf)
    }
}
