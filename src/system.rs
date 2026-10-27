//! Intégration système : réglages persistants, lancement au démarrage, instance unique.

use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
#[cfg(windows)]
use anyhow::bail;
use serde::{Deserialize, Serialize};

#[cfg(windows)]
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
        #[cfg(windows)]
        {
            std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("Lowfreed").join("settings.json"))
        }
        #[cfg(target_os = "macos")]
        {
            std::env::var_os("HOME").map(|h| {
                PathBuf::from(h)
                    .join("Library")
                    .join("Application Support")
                    .join("Lowfreed")
                    .join("settings.json")
            })
        }
        #[cfg(all(not(windows), not(target_os = "macos")))]
        {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
                .map(|d| d.join("Lowfreed").join("settings.json"))
        }
    }

    pub fn load() -> Self {
        Self::path()
            .and_then(|p| fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path().context("dossier de configuration introuvable")?;
        fs::create_dir_all(path.parent().unwrap())?;
        fs::write(&path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}

#[cfg(windows)]
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
#[cfg(windows)]
const RUN_VALUE: &str = "Lowfreed";

/// Lancement automatique à l'ouverture de session (clé Run de l'utilisateur sur Windows).
#[cfg(windows)]
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

/// Lancement automatique à l'ouverture de session sur macOS via LaunchAgent.
#[cfg(target_os = "macos")]
pub mod autostart {
    use super::*;

    fn plist_path() -> Option<PathBuf> {
        std::env::var_os("HOME").map(|h| {
            PathBuf::from(h)
                .join("Library")
                .join("LaunchAgents")
                .join("com.fraustiz.lowfreed.plist")
        })
    }

    pub fn command() -> Result<String> {
        Ok(format!("\"{}\" --tray", std::env::current_exe()?.display()))
    }

    pub fn registered() -> Option<String> {
        let path = plist_path()?;
        if !path.exists() {
            return None;
        }
        let content = fs::read_to_string(&path).ok()?;
        let mut parts = content.split("<string>");
        parts.next()?; // avant premier <string>
        parts.next()?; // com.fraustiz.lowfreed
        let exe_part = parts.next()?;
        let exe = exe_part.split("</string>").next()?.trim();
        Some(format!("\"{exe}\" --tray"))
    }

    pub fn set(enabled: bool) -> Result<()> {
        let path = plist_path().context("~/Library/LaunchAgents introuvable")?;
        if enabled {
            let exe = std::env::current_exe()?;
            let plist = format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.fraustiz.lowfreed</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
        <string>--tray</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>ProcessType</key>
    <string>Interactive</string>
</dict>
</plist>
"#,
                exe.display()
            );
            fs::create_dir_all(path.parent().unwrap())?;
            fs::write(&path, plist)?;
        } else if path.exists() {
            fs::remove_file(&path)?;
        }
        Ok(())
    }
}

#[cfg(all(not(windows), not(target_os = "macos")))]
pub mod autostart {
    use super::*;

    pub fn command() -> Result<String> {
        Ok(format!("\"{}\" --tray", std::env::current_exe()?.display()))
    }

    pub fn registered() -> Option<String> {
        None
    }

    pub fn set(_enabled: bool) -> Result<()> {
        Ok(())
    }
}

#[cfg(windows)]
const INSTANCE_MUTEX: &str = r"Local\Lowfreed.Instance";
#[cfg(windows)]
const SHOW_EVENT: &str = r"Local\Lowfreed.Show";

/// Événement « montre ta fenêtre » de la première instance.
#[cfg(windows)]
pub struct ShowSignal(pub HANDLE);

#[cfg(windows)]
unsafe impl Send for ShowSignal {}

/// Une seule instance à la fois (sinon deux hooks remplaceraient le slider en double).
/// Si Lowfreed tourne déjà, lui demande d'afficher sa fenêtre et renvoie `None`.
#[cfg(windows)]
pub fn single_instance() -> Option<ShowSignal> {
    let (mutex, event) = (wide(INSTANCE_MUTEX), wide(SHOW_EVENT));
    // SAFETY : appels Win32 avec des noms terminés par 0 ; le mutex reste ouvert jusqu'à la
    // fin du processus, c'est voulu.
    unsafe {
        CreateMutexW(std::ptr::null(), 0, mutex.as_ptr());
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let ev = OpenEventW(EVENT_MODIFY_STATE, 0, event.as_ptr());
            if !ev.is_null() {
                SetEvent(ev);
            }
            return None;
        }
        Some(ShowSignal(CreateEventW(std::ptr::null(), 0, 0, event.as_ptr())))
    }
}

#[cfg(not(windows))]
pub struct ShowSignal {
    pub rx: std::sync::mpsc::Receiver<()>,
}

#[cfg(not(windows))]
pub fn single_instance() -> Option<ShowSignal> {
    use std::io::{Read, Write};
    use std::os::unix::net::{UnixListener, UnixStream};

    let sock_path = std::env::temp_dir().join(format!("lowfreed-{}.sock", unsafe { libc::getuid() }));

    // Si une instance tourne déjà, on lui demande d'afficher sa fenêtre et on quitte
    if let Ok(mut stream) = UnixStream::connect(&sock_path) {
        let _ = stream.write_all(b"show\n");
        let _ = stream.flush();
        return None;
    }

    // Nettoyage d'une ancienne socket orpheline
    let _ = fs::remove_file(&sock_path);

    let listener = UnixListener::bind(&sock_path).ok()?;
    let (tx, rx) = std::sync::mpsc::channel();

    std::thread::Builder::new()
        .name("single-instance".into())
        .spawn(move || {
            for stream in listener.incoming() {
                if let Ok(mut s) = stream {
                    let mut buf = [0u8; 16];
                    let _ = s.read(&mut buf);
                    let _ = tx.send(());
                }
            }
        })
        .expect("fil single-instance");

    Some(ShowSignal { rx })
}

#[cfg(target_os = "macos")]
pub mod macos_app {
    use std::ffi::c_void;

    extern "C" {
        fn objc_getClass(name: *const u8) -> *mut c_void;
        fn object_getClass(obj: *mut c_void) -> *mut c_void;
        fn class_getName(cls: *mut c_void) -> *const std::ffi::c_char;
        fn sel_registerName(name: *const u8) -> *mut c_void;
        fn class_addMethod(cls: *mut c_void, name: *mut c_void, imp: *const (), types: *const u8) -> bool;
        fn class_replaceMethod(cls: *mut c_void, name: *mut c_void, imp: *const (), types: *const u8) -> *mut c_void;
        fn objc_msgSend();
    }

    static QUITTING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

    pub fn set_quitting(val: bool) {
        QUITTING.store(val, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn is_quitting() -> bool {
        QUITTING.load(std::sync::atomic::Ordering::SeqCst)
    }

    unsafe extern "C" fn should_app_terminate(_this: *mut c_void, _sel: *mut c_void, _app: *mut c_void) -> usize {
        if is_quitting() {
            eprintln!("[Lowfreed] applicationShouldTerminate: quitting -> NSTerminateNow");
            1 // NSTerminateNow
        } else {
            eprintln!("[Lowfreed] applicationShouldTerminate: background active -> NSTerminateCancel");
            0 // NSTerminateCancel
        }
    }

    static OPEN_TRIGGER: std::sync::Mutex<Option<std::sync::Arc<dyn Fn() + Send + Sync>>> =
        std::sync::Mutex::new(None);

    pub fn set_open_trigger(f: std::sync::Arc<dyn Fn() + Send + Sync>) {
        if let Ok(mut guard) = OPEN_TRIGGER.lock() {
            *guard = Some(f);
        }
    }

    pub fn trigger_open() {
        if let Ok(guard) = OPEN_TRIGGER.lock() {
            if let Some(f) = guard.as_ref() {
                f();
            }
        }
    }

    unsafe extern "C" fn did_become_active(
        _this: *mut c_void,
        _sel: *mut c_void,
        _notif: *mut c_void,
    ) {
        show_main_window();
        trigger_open();
    }

    unsafe extern "C" fn should_handle_reopen(
        _this: *mut c_void,
        _sel: *mut c_void,
        _app: *mut c_void,
        _has_visible: bool,
    ) -> bool {
        show_main_window();
        trigger_open();
        true
    }

    unsafe extern "C" fn should_terminate(_this: *mut c_void, _sel: *mut c_void, _app: *mut c_void) -> bool {
        false
    }

    /// Empêche macOS de tuer l'application en arrière-plan (Automatic Termination, Sudden Termination,
    /// et fermeture de la dernière fenêtre) lorsque celle-ci est réduite dans la zone de notification.
    pub fn prevent_background_termination() {
        unsafe {
            // 1. Désactiver Sudden Termination et Automatic Termination sur NSProcessInfo
            let cls_proc = objc_getClass(b"NSProcessInfo\0".as_ptr());
            let sel_proc = sel_registerName(b"processInfo\0".as_ptr());
            let sel_dis_sudden = sel_registerName(b"disableSuddenTermination\0".as_ptr());
            let sel_dis_auto = sel_registerName(b"disableAutomaticTermination:\0".as_ptr());
            let cls_str = objc_getClass(b"NSString\0".as_ptr());
            let sel_utf8 = sel_registerName(b"stringWithUTF8String:\0".as_ptr());

            let f_send: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
                std::mem::transmute(objc_msgSend as *const ());
            let proc = f_send(cls_proc, sel_proc);
            if !proc.is_null() {
                f_send(proc, sel_dis_sudden);

                let f_str: unsafe extern "C" fn(*mut c_void, *mut c_void, *const u8) -> *mut c_void =
                    std::mem::transmute(objc_msgSend as *const ());
                let reason = f_str(cls_str, sel_utf8, b"Lowfreed menu bar background service\0".as_ptr());
                let f_dis: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) =
                    std::mem::transmute(objc_msgSend as *const ());
                f_dis(proc, sel_dis_auto, reason);
            }

            // 2. Intercepter applicationShouldTerminateAfterLastWindowClosed: sur le delegate NSApp
            let cls_nsapp = objc_getClass(b"NSApplication\0".as_ptr());
            let sel_shared = sel_registerName(b"sharedApplication\0".as_ptr());
            let sel_del = sel_registerName(b"delegate\0".as_ptr());
            let app = f_send(cls_nsapp, sel_shared);
            if !app.is_null() {
                let del = f_send(app, sel_del);
                if !del.is_null() {
                    let del_cls = object_getClass(del);
                    let sel_term = sel_registerName(b"applicationShouldTerminateAfterLastWindowClosed:\0".as_ptr());
                    let types = b"B@:@\0".as_ptr(); // returns BOOL (B), self (@), _cmd (:), sender (@)
                    let added = class_addMethod(del_cls, sel_term, should_terminate as *const (), types);
                    if !added {
                        class_replaceMethod(del_cls, sel_term, should_terminate as *const (), types);
                    }
                    let sel_app_term = sel_registerName(b"applicationShouldTerminate:\0".as_ptr());
                    let types_app_term = b"Q@:@\0".as_ptr();
                    let added_app_term = class_addMethod(del_cls, sel_app_term, should_app_terminate as *const (), types_app_term);
                    if !added_app_term {
                        class_replaceMethod(del_cls, sel_app_term, should_app_terminate as *const (), types_app_term);
                    }
                    let sel_reopen = sel_registerName(b"applicationShouldHandleReopen:hasVisibleWindows:\0".as_ptr());
                    let types_reopen = b"B@:@B\0".as_ptr();
                    let added_reopen = class_addMethod(del_cls, sel_reopen, should_handle_reopen as *const (), types_reopen);
                    if !added_reopen {
                        class_replaceMethod(del_cls, sel_reopen, should_handle_reopen as *const (), types_reopen);
                    }
                    let sel_active = sel_registerName(b"applicationDidBecomeActive:\0".as_ptr());
                    let types_active = b"v@:@\0".as_ptr();
                    let added_active = class_addMethod(del_cls, sel_active, did_become_active as *const (), types_active);
                    if !added_active {
                        class_replaceMethod(del_cls, sel_active, did_become_active as *const (), types_active);
                    }
                    let name = std::ffi::CStr::from_ptr(class_getName(del_cls)).to_string_lossy();
                    eprintln!("[Lowfreed] prevent_background_termination: hooked {name}, added={added}, added_app_term={added_app_term}, added_reopen={added_reopen}, added_active={added_active}");
                }
            }
        }
    }

    /// Active l'application et la place au premier plan lors du clic sur l'icône de la barre des menus.
    pub fn activate_app() {
        show_main_window();
    }

    /// Affiche et active la fenêtre principale (utilisé lors du clic sur le tray ou réouverture Dock).
    pub fn show_main_window() {
        unsafe {
            let cls_nsapp = objc_getClass(b"NSApplication\0".as_ptr());
            let sel_shared = sel_registerName(b"sharedApplication\0".as_ptr());
            let f_send: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
                std::mem::transmute(objc_msgSend as *const ());
            let app = f_send(cls_nsapp, sel_shared);
            if app.is_null() {
                return;
            }

            let sel_windows = sel_registerName(b"windows\0".as_ptr());
            let windows = f_send(app, sel_windows);
            if !windows.is_null() {
                let sel_count = sel_registerName(b"count\0".as_ptr());
                let f_count: unsafe extern "C" fn(*mut c_void, *mut c_void) -> usize =
                    std::mem::transmute(objc_msgSend as *const ());
                let count = f_count(windows, sel_count);

                let sel_obj_at = sel_registerName(b"objectAtIndex:\0".as_ptr());
                let f_obj_at: unsafe extern "C" fn(*mut c_void, *mut c_void, usize) -> *mut c_void =
                    std::mem::transmute(objc_msgSend as *const ());

                let sel_set_vis = sel_registerName(b"setIsVisible:\0".as_ptr());
                let f_set_vis: unsafe extern "C" fn(*mut c_void, *mut c_void, bool) =
                    std::mem::transmute(objc_msgSend as *const ());

                let sel_make_key = sel_registerName(b"makeKeyAndOrderFront:\0".as_ptr());
                let f_make_key: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) =
                    std::mem::transmute(objc_msgSend as *const ());

                let sel_order_front = sel_registerName(b"orderFrontRegardless\0".as_ptr());
                let sel_demin = sel_registerName(b"deminiaturize:\0".as_ptr());

                for i in 0..count {
                    let win = f_obj_at(windows, sel_obj_at, i);
                    if !win.is_null() {
                        let win_cls = object_getClass(win);
                        let name = std::ffi::CStr::from_ptr(class_getName(win_cls)).to_bytes();
                        if !name.starts_with(b"NSStatusBarWindow") {
                            f_send(win, sel_demin);
                            f_set_vis(win, sel_set_vis, true);
                            f_make_key(win, sel_make_key, std::ptr::null_mut());
                            f_send(win, sel_order_front);
                        }
                    }
                }
            }

            let sel_activate = sel_registerName(b"activateIgnoringOtherApps:\0".as_ptr());
            let f_act: unsafe extern "C" fn(*mut c_void, *mut c_void, bool) =
                std::mem::transmute(objc_msgSend as *const ());
            f_act(app, sel_activate, true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_default_and_round_trip() {
        let s = Settings::default();
        let json = serde_json::to_string(&s).unwrap();
        let s2: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(s, s2);
    }

    #[test]
    #[cfg(not(windows))]
    fn unix_single_instance() {
        let s1 = single_instance();
        assert!(s1.is_some());
        let s2 = single_instance();
        assert!(s2.is_none());
        // Clean up socket file
        let sock_path = std::env::temp_dir().join(format!("lowfreed-{}.sock", unsafe { libc::getuid() }));
        let _ = fs::remove_file(&sock_path);
    }
}
