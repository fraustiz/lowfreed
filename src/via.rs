//! Client du protocole VIA (raw HID, rapports de 32 octets, page d'usage 0xFF60).
//!
//! Le Flow 2 expose ce protocole sur son interface USB n°1 en mode filaire,
//! et sur son interface n°2 via le dongle 2.4G. Les commandes et leurs
//! réponses ont été vérifiées sur le clavier (protocole v12, 6 couches, 16 macros).

use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use hidapi::{HidApi, HidDevice};

pub const REPORT_LEN: usize = 32;
/// Charge utile maximale d'une requête de lecture/écriture de tampon.
const CHUNK: usize = 28;
const USAGE_PAGE: u16 = 0xFF60;
const USAGE: u16 = 0x61;
const TIMEOUT: Duration = Duration::from_millis(1000);

mod cmd {
    pub const GET_PROTOCOL_VERSION: u8 = 0x01;
    pub const GET_KEYBOARD_VALUE: u8 = 0x02;
    pub const SET_KEYCODE: u8 = 0x05;
    pub const KEYMAP_RESET: u8 = 0x06;
    pub const CUSTOM_SET_VALUE: u8 = 0x07;
    pub const CUSTOM_GET_VALUE: u8 = 0x08;
    pub const CUSTOM_SAVE: u8 = 0x09;
    pub const MACRO_GET_COUNT: u8 = 0x0C;
    pub const MACRO_GET_BUFFER_SIZE: u8 = 0x0D;
    pub const MACRO_GET_BUFFER: u8 = 0x0E;
    pub const MACRO_SET_BUFFER: u8 = 0x0F;
    pub const GET_LAYER_COUNT: u8 = 0x11;
    pub const KEYMAP_GET_BUFFER: u8 = 0x12;
    pub const KEYMAP_SET_BUFFER: u8 = 0x13;
    pub const UNHANDLED: u8 = 0xFF;
}

mod value {
    pub const UPTIME: u8 = 0x01;
    pub const SWITCH_MATRIX_STATE: u8 = 0x03;
}

/// Canal « qmk_backlight » des valeurs personnalisées VIA (rétroéclairage blanc du Flow 2).
pub mod backlight {
    pub const CHANNEL: u8 = 1;
    pub const BRIGHTNESS: u8 = 1;
    pub const EFFECT: u8 = 2;
}

#[derive(Clone, Debug)]
pub struct DeviceInfo {
    pub vendor_id: u16,
    pub product_id: u16,
    pub product: String,
    pub release: u16,
}

impl DeviceInfo {
    /// `true` si connecté via le dongle sans fil 2.4 GHz (révision >= 0x2000, e.g. 0x2004).
    pub fn is_wireless(&self) -> bool {
        self.release >= 0x2000
    }

    pub fn transport_label(&self) -> &'static str {
        if self.is_wireless() {
            "2.4G"
        } else {
            "USB"
        }
    }
}

pub struct Via {
    dev: HidDevice,
    pub info: DeviceInfo,
}

impl Via {
    /// Ouvre l'interface VIA du clavier `vid:pid`.
    pub fn open(vendor_id: u16, product_id: u16) -> Result<Self> {
        let api = HidApi::new().context("initialisation HID impossible")?;
        let found = api
            .device_list()
            .find(|d| {
                d.vendor_id() == vendor_id
                    && d.product_id() == product_id
                    && d.usage_page() == USAGE_PAGE
                    && d.usage() == USAGE
            })
            .ok_or_else(|| anyhow!("clavier {vendor_id:04X}:{product_id:04X} introuvable"))?;
        let info = DeviceInfo {
            vendor_id,
            product_id,
            product: found.product_string().unwrap_or("?").to_string(),
            release: found.release_number(),
        };
        let dev = found
            .open_device(&api)
            .context("ouverture de l'interface VIA impossible")?;
        Ok(Self { dev, info })
    }

    /// Envoie une requête et attend la réponse dont les `echo` premiers octets
    /// reprennent ceux de la requête.
    ///
    /// Windows diffuse les rapports d'entrée à tous les programmes qui ont ouvert
    /// l'interface : si un autre outil (usevia.app dans un navigateur…) parle au
    /// clavier en même temps, ses réponses arrivent ici aussi et sont ignorées.
    fn transact(&self, req: &[u8], echo: usize) -> Result<[u8; REPORT_LEN]> {
        let mut out = [0u8; REPORT_LEN + 1]; // octet 0 = numéro de rapport (aucun)
        out[1..1 + req.len()].copy_from_slice(req);
        self.dev.write(&out).context("écriture HID échouée")?;

        let deadline = Instant::now() + TIMEOUT;
        let mut last_write = Instant::now();
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                bail!("pas de réponse du clavier (commande 0x{:02X})", req[0]);
            }
            // En 2.4G, si le clavier était en veille radio, le premier paquet peut
            // être manqué lors du réveil : si aucune réponse après 200 ms, on réémet.
            if last_write.elapsed() >= Duration::from_millis(200) {
                let _ = self.dev.write(&out);
                last_write = Instant::now();
            }
            let poll = left.min(Duration::from_millis(100));
            let mut resp = [0u8; REPORT_LEN];
            let n = self
                .dev
                .read_timeout(&mut resp, poll.as_millis().max(1) as i32)
                .context("lecture HID échouée")?;
            if n == 0 {
                continue;
            }
            if resp[0] == cmd::UNHANDLED && resp[1..echo] == req[1..echo] {
                bail!("commande 0x{:02X} non prise en charge par le firmware", req[0]);
            }
            if resp[..echo] == req[..echo] {
                return Ok(resp);
            }
        }
    }

    pub fn protocol_version(&self) -> Result<u16> {
        let r = self.transact(&[cmd::GET_PROTOCOL_VERSION], 1)?;
        Ok(u16::from_be_bytes([r[1], r[2]]))
    }

    pub fn layer_count(&self) -> Result<u8> {
        Ok(self.transact(&[cmd::GET_LAYER_COUNT], 1)?[1])
    }

    /// Temps écoulé depuis la mise sous tension, en millisecondes.
    pub fn uptime_ms(&self) -> Result<u32> {
        let r = self.transact(&[cmd::GET_KEYBOARD_VALUE, value::UPTIME], 2)?;
        Ok(u32::from_be_bytes([r[2], r[3], r[4], r[5]]))
    }

    /// Lit tout le keymap : `layers × rows × cols` keycodes, dans cet ordre.
    pub fn read_keymap(&self, layers: u8, rows: u8, cols: u8) -> Result<Vec<u16>> {
        let len = layers as usize * rows as usize * cols as usize * 2;
        let bytes = self.read_buffer(cmd::KEYMAP_GET_BUFFER, len)?;
        Ok(bytes.as_chunks::<2>().0.iter().map(|b| u16::from_be_bytes(*b)).collect())
    }

    /// Réécrit tout le keymap (même ordre que [`Self::read_keymap`]).
    pub fn write_keymap(&self, keymap: &[u16]) -> Result<()> {
        let bytes: Vec<u8> = keymap.iter().flat_map(|k| k.to_be_bytes()).collect();
        self.write_buffer(cmd::KEYMAP_SET_BUFFER, 0, &bytes)
    }

    pub fn set_keycode(&self, layer: u8, row: u8, col: u8, keycode: u16) -> Result<()> {
        let [hi, lo] = keycode.to_be_bytes();
        self.transact(&[cmd::SET_KEYCODE, layer, row, col, hi, lo], 6)?;
        Ok(())
    }

    /// Restaure le keymap d'usine du firmware.
    pub fn reset_keymap(&self) -> Result<()> {
        self.transact(&[cmd::KEYMAP_RESET], 1)?;
        Ok(())
    }

    pub fn macro_count(&self) -> Result<u8> {
        Ok(self.transact(&[cmd::MACRO_GET_COUNT], 1)?[1])
    }

    pub fn macro_buffer_size(&self) -> Result<u16> {
        let r = self.transact(&[cmd::MACRO_GET_BUFFER_SIZE], 1)?;
        Ok(u16::from_be_bytes([r[1], r[2]]))
    }

    pub fn read_macro_buffer(&self, size: u16) -> Result<Vec<u8>> {
        self.read_buffer(cmd::MACRO_GET_BUFFER, size as usize)
    }

    /// Écrit le tampon de macros comme le fait VIA : le dernier octet du tampon
    /// sert de drapeau « écriture en cours » pendant la copie.
    pub fn write_macro_buffer(&self, size: u16, data: &[u8]) -> Result<()> {
        if data.len() > size as usize {
            bail!("macros trop longues : {} octets pour {} disponibles", data.len(), size);
        }
        let last = size - 1;
        self.write_buffer(cmd::MACRO_SET_BUFFER, last, &[0xFF])?;
        self.write_buffer(cmd::MACRO_SET_BUFFER, 0, data)?;
        self.write_buffer(cmd::MACRO_SET_BUFFER, last, &[0x00])
    }

    pub fn custom_get(&self, channel: u8, id: u8) -> Result<u8> {
        Ok(self.transact(&[cmd::CUSTOM_GET_VALUE, channel, id], 3)?[3])
    }

    pub fn custom_set(&self, channel: u8, id: u8, value: u8) -> Result<()> {
        self.transact(&[cmd::CUSTOM_SET_VALUE, channel, id, value], 3)?;
        Ok(())
    }

    /// Enregistre en mémoire permanente les valeurs du canal.
    pub fn custom_save(&self, channel: u8) -> Result<()> {
        self.transact(&[cmd::CUSTOM_SAVE, channel], 2)?;
        Ok(())
    }

    /// État instantané de la matrice : `rows × cols` booléens (touche enfoncée).
    pub fn matrix_state(&self, rows: u8, cols: u8) -> Result<Vec<bool>> {
        let bytes_per_row = (cols as usize).div_ceil(8);
        let rows_per_req = 28 / bytes_per_row;
        let mut state = Vec::with_capacity(rows as usize * cols as usize);
        let mut offset = 0usize;
        while offset < rows as usize {
            let r = self.transact(
                &[cmd::GET_KEYBOARD_VALUE, value::SWITCH_MATRIX_STATE, offset as u8],
                2,
            )?;
            let n = rows_per_req.min(rows as usize - offset);
            for row in 0..n {
                let start = 3 + row * bytes_per_row;
                let bits = r[start..start + bytes_per_row]
                    .iter()
                    .fold(0u32, |acc, b| (acc << 8) | *b as u32);
                state.extend((0..cols).map(|c| bits >> c & 1 == 1));
            }
            offset += n;
        }
        Ok(state)
    }

    fn read_buffer(&self, command: u8, len: usize) -> Result<Vec<u8>> {
        let mut out = Vec::with_capacity(len);
        while out.len() < len {
            let offset = out.len() as u16;
            let size = CHUNK.min(len - out.len());
            let [hi, lo] = offset.to_be_bytes();
            let r = self.transact(&[command, hi, lo, size as u8], 4)?;
            out.extend_from_slice(&r[4..4 + size]);
        }
        Ok(out)
    }

    fn write_buffer(&self, command: u8, start: u16, data: &[u8]) -> Result<()> {
        for (i, chunk) in data.chunks(CHUNK).enumerate() {
            let offset = start + (i * CHUNK) as u16;
            let [hi, lo] = offset.to_be_bytes();
            let mut req = vec![command, hi, lo, chunk.len() as u8];
            req.extend_from_slice(chunk);
            self.transact(&req, 4)?;
        }
        Ok(())
    }
}
