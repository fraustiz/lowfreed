//! Intégration Windows : réglages persistants, lancement au démarrage, instance unique.

use std::{fs, path::PathBuf, ptr::null};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use windows_sys::Win32::{
    Foundation::{GetLastError, ERROR_ALREADY_EXISTS, ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, HANDLE},
    System::{
        Registry::{RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ},
        Threading::{CreateEventW, CreateMutexW, OpenEventW, SetEvent, EVENT_MODIFY_STATE},
    },
};

use crate::slider::SliderConfig;

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub slider: SliderConfig,
    /// Fermer la fenêtre la réduit dans la zone de notification.
    pub close_to_tray: bool,
    /// Dernière couche de base détectée (présélectionnée au lancement).
    pub last_layer: Option<u8>,
}

impl Default for Settings {
    fn default() -> Self {
        Self { slider: SliderConfig::default(), close_to_tray: true, last_layer: None }
    }
}

impl Settings {
    fn path() -> Option<PathBuf> {
        std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("Lowfreed").join("settings.json"))
    }

    pub fn load() -> Self {
        Self::path()
            .and_then(|p| fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path().context("%APPDATA% introuvable")?;
        fs::create_dir_all(path.parent().unwrap())?;
        fs::write(&path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "Lowfreed";

/// Lancement automatique à l'ouverture de session (clé Run de l'utilisateur).
pub mod autostart {
    use super::*;

    /// Commande enregistrée : l'exécutable actuel, démarré dans la zone de notification.
    pub fn command() -> Result<String> {
        Ok(format!("\"{}\" --tray", std::env::current_exe()?.display()))
    }

    /// Commande actuellement enregistrée, s'il y en a une.
    pub fn registered() -> Option<String> {
        let (key, value) = (wide(RUN_KEY), wide(RUN_VALUE));
        let mut buf = vec![0u16; 1024];
        let mut size = (buf.len() * 2) as u32;
        // SAFETY : tampon et taille cohérents, chaînes terminées par 0.
        let err = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buf.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if err != ERROR_SUCCESS {
            return None;
        }
        let len = (size as usize / 2).saturating_sub(1);
        Some(String::from_utf16_lossy(&buf[..len]))
    }

    pub fn set(enabled: bool) -> Result<()> {
        let (key, value) = (wide(RUN_KEY), wide(RUN_VALUE));
        // SAFETY : chaînes terminées par 0, données REG_SZ avec leur 0 final.
        let err = unsafe {
            if enabled {
                let data = wide(&command()?);
                RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    key.as_ptr(),
                    value.as_ptr(),
                    REG_SZ,
                    data.as_ptr().cast(),
                    (data.len() * 2) as u32,
                )
            } else {
                match RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr()) {
                    ERROR_FILE_NOT_FOUND => ERROR_SUCCESS,
                    e => e,
                }
            }
        };
        if err != ERROR_SUCCESS {
            bail!("registre : erreur Windows {err}");
        }
        Ok(())
    }
}

const INSTANCE_MUTEX: &str = r"Local\Lowfreed.Instance";
const SHOW_EVENT: &str = r"Local\Lowfreed.Show";

/// Événement « montre ta fenêtre » de la première instance.
pub struct ShowSignal(pub HANDLE);

// SAFETY : un HANDLE d'événement Win32 s'utilise depuis n'importe quel fil.
unsafe impl Send for ShowSignal {}

/// Une seule instance à la fois (sinon deux hooks remplaceraient le slider en double).
/// Si Lowfreed tourne déjà, lui demande d'afficher sa fenêtre et renvoie `None`.
pub fn single_instance() -> Option<ShowSignal> {
    let (mutex, event) = (wide(INSTANCE_MUTEX), wide(SHOW_EVENT));
    // SAFETY : appels Win32 avec des noms terminés par 0 ; le mutex reste ouvert jusqu'à la
    // fin du processus, c'est voulu.
    unsafe {
        CreateMutexW(null(), 0, mutex.as_ptr());
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let ev = OpenEventW(EVENT_MODIFY_STATE, 0, event.as_ptr());
            if !ev.is_null() {
                SetEvent(ev);
            }
            return None;
        }
        Some(ShowSignal(CreateEventW(null(), 0, 0, event.as_ptr())))
    }
}
