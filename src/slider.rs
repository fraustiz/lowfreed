//! Remplacement de l'action du slider latéral du Flow 2.
//!
//! Le firmware envoie « Volume + / − » (page Consumer, usages 0xE9 / 0xEA, rapport n°4 de
//! l'interface 2) quand on fait glisser le slider, et « Luminosité écran ± » avec Fn.
//! VIA ne l'expose pas : le slider n'est ni dans la matrice, ni un encodeur, ni une valeur
//! personnalisée. On le remplace donc côté Windows :
//! - un fil lit les rapports Consumer du Flow 2 ; à chaque « Volume ± », il interroge la
//!   matrice VIA : si aucune touche de volume du keymap (F11/F12…) n'est enfoncée, l'appui
//!   vient du slider ;
//! - un hook clavier bas niveau reçoit la touche virtuelle VK_VOLUME_UP/DOWN que Windows en
//!   tire, la bloque si elle vient du slider et déclenche l'action choisie.

use std::{
    collections::VecDeque,
    ptr::null,
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc, Condvar, Mutex, OnceLock,
    },
    thread,
    time::{Duration, Instant},
};

use hidapi::{HidApi, HidDevice};
use serde::{Deserialize, Serialize};
use windows_sys::Win32::{
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        Input::KeyboardAndMouse::{
            SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
            KEYEVENTF_KEYUP, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_WHEEL, MOUSEINPUT, VIRTUAL_KEY, VK_CONTROL,
            VK_LEFT, VK_MEDIA_NEXT_TRACK, VK_MEDIA_PREV_TRACK, VK_RIGHT, VK_SHIFT, VK_TAB,
            VK_VOLUME_DOWN, VK_VOLUME_UP,
        },
        WindowsAndMessaging::{
            CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW, TranslateMessage,
            HC_ACTION, KBDLLHOOKSTRUCT, LLKHF_EXTENDED, LLKHF_INJECTED, MSG, WH_KEYBOARD_LL, WM_KEYDOWN,
            WM_SYSKEYDOWN,
        },
    },
};

use crate::{layer_detect, via::Via};

const CONSUMER_PAGE: u16 = 0x0C;
/// Rapports de contrôle consommateur (volume) :
/// - rapport n°3 sur le récepteur sans fil 2.4G (MI_00 Col 3)
/// - rapport n°4 en USB filaire (interface 2 Col 3)
const CONSUMER_REPORT_IDS: [u8; 2] = [3, 4];
const USAGE_VOLUME_UP: u16 = 0xE9;
const USAGE_VOLUME_DOWN: u16 = 0xEA;
const KC_VOLU: u16 = 0xA9;
const KC_VOLD: u16 = 0xAA;
/// Attente maximale, dans le hook, du rapport HID correspondant à une touche volume.
const HOOK_WAIT: Duration = Duration::from_millis(60);
/// Au-delà, un appui HID non apparié est périmé.
const PRESS_MAX_AGE: Duration = Duration::from_millis(200);
/// Pause qui sépare deux gestes (remet à zéro le compteur de sensibilité).
const GESTURE_GAP: Duration = Duration::from_millis(600);
const RECONNECT_EVERY: Duration = Duration::from_secs(2);

/// Ce que fait le slider.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum Action {
    /// Comportement d'origine : rien n'est intercepté.
    #[default]
    Volume,
    Scroll,
    HScroll,
    Zoom,
    Tabs,
    Tracks,
    Arrows,
}

impl Action {
    pub const ALL: [Action; 7] = [
        Action::Volume,
        Action::Scroll,
        Action::HScroll,
        Action::Zoom,
        Action::Tabs,
        Action::Tracks,
        Action::Arrows,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Action::Volume => "Volume (d'origine)",
            Action::Scroll => "Défilement vertical",
            Action::HScroll => "Défilement horizontal",
            Action::Zoom => "Zoom",
            Action::Tabs => "Onglet suivant / précédent",
            Action::Tracks => "Piste suivante / précédente",
            Action::Arrows => "Flèches → / ←",
        }
    }

    pub fn detail(self) -> &'static str {
        match self {
            Action::Volume => "Le slider règle le volume de Windows, comme sans Lowfreed.",
            Action::Scroll => "Comme la molette de la souris, sous le pointeur.",
            Action::HScroll => "Molette horizontale, sous le pointeur.",
            Action::Zoom => "Ctrl + molette : navigateur, documents, images…",
            Action::Tabs => "Ctrl+Tab / Ctrl+Maj+Tab dans le navigateur, l'éditeur…",
            Action::Tracks => "Touches multimédia piste suivante / précédente.",
            Action::Arrows => "Flèches droite / gauche (avance/recul dans YouTube, VLC…).",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct SliderConfig {
    pub action: Action,
    /// Inverse le sens du slider.
    pub invert: bool,
    /// Nombre de crans du slider par action (1 = chaque cran).
    pub step: u8,
}

impl Default for SliderConfig {
    fn default() -> Self {
        Self { action: Action::Volume, invert: false, step: 1 }
    }
}

/// Sens du slider : `Up` correspond au « Volume + » d'origine.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dir {
    Up,
    Down,
}

impl Dir {
    fn flip(self) -> Self {
        match self {
            Dir::Up => Dir::Down,
            Dir::Down => Dir::Up,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LastEvent {
    pub at: Instant,
    pub dir: Dir,
    /// `true` : slider ; `false` : touche volume du keymap (laissée passer).
    pub from_slider: bool,
    pub replaced: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Status {
    pub device: bool,
    pub hook: bool,
    pub replaced: u64,
    pub last: Option<LastEvent>,
    /// Diagnostic : appuis vus par le hook (dont touches volume, avec les drapeaux du
    /// dernier), « Volume ± » lus sur le Flow 2.
    pub hook_keys: u64,
    pub hook_volume: u64,
    pub hook_volume_flags: u32,
    pub hid_volume: u64,
}

struct Press {
    at: Instant,
    dir: Dir,
    from_slider: bool,
}

struct Shared {
    config: Mutex<SliderConfig>,
    presses: Mutex<VecDeque<Press>>,
    arrived: Condvar,
    status: Mutex<Status>,
    keymap_dirty: AtomicBool,
    /// Touche volume dont l'appui a été bloqué : on bloque aussi son relâchement.
    swallowed: [AtomicBool; 2],
    actions: Mutex<Sender<Dir>>,
    /// Couche de base en service, déduite par [`layer_detect`].
    active_layer: Arc<Mutex<Option<u8>>>,
    layer_keymap_dirty: Arc<AtomicBool>,
    hook_keys: AtomicU64,
    hid_volume: AtomicU64,
    hook_volume: AtomicU64,
    hook_volume_flags: AtomicU32,
}

static SHARED: OnceLock<Arc<Shared>> = OnceLock::new();

/// Moteur d'arrière-plan : remplacement du slider et détection de la couche active
/// (un seul par processus, à cause du hook clavier global).
pub struct Engine {
    shared: Arc<Shared>,
}

impl Engine {
    pub fn start(vendor_id: u16, product_id: u16, rows: u8, cols: u8, config: SliderConfig) -> Self {
        let (tx, rx) = mpsc::channel();
        let shared = SHARED
            .get_or_init(|| {
                Arc::new(Shared {
                    config: Mutex::new(config),
                    presses: Mutex::new(VecDeque::new()),
                    arrived: Condvar::new(),
                    status: Mutex::new(Status::default()),
                    keymap_dirty: AtomicBool::new(false),
                    swallowed: [AtomicBool::new(false), AtomicBool::new(false)],
                    actions: Mutex::new(tx),
                    active_layer: Arc::new(Mutex::new(None)),
                    layer_keymap_dirty: Arc::new(AtomicBool::new(false)),
                    hook_keys: AtomicU64::new(0),
                    hid_volume: AtomicU64::new(0),
                    hook_volume: AtomicU64::new(0),
                    hook_volume_flags: AtomicU32::new(0),
                })
            })
            .clone();
        let (key_tx, key_rx) = mpsc::channel();
        layer_detect::install(key_tx);
        let (active, dirty) = (shared.active_layer.clone(), shared.layer_keymap_dirty.clone());
        thread::Builder::new()
            .name("layer-detect".into())
            .spawn(move || layer_detect::run(key_rx, active, dirty, vendor_id, product_id, rows, cols))
            .expect("fil layer-detect");
        let s = shared.clone();
        thread::Builder::new()
            .name("slider-hid".into())
            .spawn(move || reader(s, vendor_id, product_id, rows, cols))
            .expect("fil slider-hid");
        thread::Builder::new()
            .name("slider-hook".into())
            .spawn(hook_thread)
            .expect("fil slider-hook");
        let s = shared.clone();
        thread::Builder::new()
            .name("slider-action".into())
            .spawn(move || worker(s, rx))
            .expect("fil slider-action");
        Self { shared }
    }

    pub fn config(&self) -> SliderConfig {
        *self.shared.config.lock().unwrap()
    }

    pub fn set_config(&self, config: SliderConfig) {
        *self.shared.config.lock().unwrap() = config;
    }

    pub fn status(&self) -> Status {
        let mut s = self.shared.status.lock().unwrap().clone();
        s.hook_keys = self.shared.hook_keys.load(Ordering::Relaxed);
        s.hid_volume = self.shared.hid_volume.load(Ordering::Relaxed);
        s.hook_volume = self.shared.hook_volume.load(Ordering::Relaxed);
        s.hook_volume_flags = self.shared.hook_volume_flags.load(Ordering::Relaxed);
        s
    }

    /// À appeler après une modification du keymap : les touches volume ont pu bouger.
    pub fn keymap_changed(&self) {
        self.shared.keymap_dirty.store(true, Ordering::Relaxed);
        self.shared.layer_keymap_dirty.store(true, Ordering::Relaxed);
    }

    /// Couche de base en service sur le clavier, une fois détectée (appui sur Alt, Win, F11…).
    pub fn active_layer(&self) -> Option<u8> {
        *self.shared.active_layer.lock().unwrap()
    }
}

// ─────────────────────────── lecture HID ───────────────────────────

fn open(api: &HidApi, vendor_id: u16, product_id: u16) -> Option<(HidDevice, Via)> {
    let consumer = api
        .device_list()
        .find(|d| d.vendor_id() == vendor_id && d.product_id() == product_id && d.usage_page() == CONSUMER_PAGE)?
        .open_device(api)
        .ok()?;
    let via = Via::open(vendor_id, product_id).ok()?;
    Some((consumer, via))
}

/// Positions de matrice qui portent un keycode de volume, sur n'importe quelle couche.
fn volume_keys(via: &Via, rows: u8, cols: u8) -> Vec<usize> {
    let per_layer = rows as usize * cols as usize;
    let Ok(layers) = via.layer_count() else { return Vec::new() };
    let Ok(keymap) = via.read_keymap(layers, rows, cols) else { return Vec::new() };
    let mut keys: Vec<usize> = keymap
        .iter()
        .enumerate()
        .filter(|(_, k)| matches!(**k, KC_VOLU | KC_VOLD))
        .map(|(i, _)| i % per_layer)
        .collect();
    keys.sort_unstable();
    keys.dedup();
    keys
}

fn reader(shared: Arc<Shared>, vendor_id: u16, product_id: u16, rows: u8, cols: u8) {
    loop {
        let found = HidApi::new().ok().and_then(|api| open(&api, vendor_id, product_id));
        let Some((consumer, via)) = found else {
            shared.status.lock().unwrap().device = false;
            thread::sleep(RECONNECT_EVERY);
            continue;
        };
        let mut keys = volume_keys(&via, rows, cols);
        shared.keymap_dirty.store(false, Ordering::Relaxed);
        shared.status.lock().unwrap().device = true;

        let mut last_matrix_check = Instant::now() - Duration::from_secs(1);
        let mut last_matrix_from_slider = false;

        loop {
            if shared.keymap_dirty.swap(false, Ordering::Relaxed) {
                keys = volume_keys(&via, rows, cols);
            }
            let mut buf = [0u8; 16];
            let n = match consumer.read_timeout(&mut buf, 250) {
                Ok(n) => n,
                Err(_) => break, // clavier débranché : on se reconnectera
            };
            if n < 3 || !CONSUMER_REPORT_IDS.contains(&buf[0]) {
                continue;
            }
            let dir = match u16::from_le_bytes([buf[1], buf[2]]) {
                USAGE_VOLUME_UP => Dir::Up,
                USAGE_VOLUME_DOWN => Dir::Down,
                _ => continue,
            };
            // Une touche volume du keymap enfoncée à cet instant : ce n'est pas le slider.
            // Si la matrice ne répond pas, on laisse passer (le volume reste utilisable).
            // En 2.4G lors d'une rafale de crans du slider, on réutilise l'état lu s'il date de moins de 30 ms.
            let now = Instant::now();
            let from_slider = if now.saturating_duration_since(last_matrix_check) < Duration::from_millis(30)
                && last_matrix_from_slider
            {
                true
            } else {
                let s = via
                    .matrix_state(rows, cols)
                    .is_ok_and(|m| !keys.iter().any(|&i| m.get(i).copied().unwrap_or(false)));
                last_matrix_check = now;
                last_matrix_from_slider = s;
                s
            };
            let mut presses = shared.presses.lock().unwrap();
            presses.retain(|p| p.at.elapsed() < PRESS_MAX_AGE);
            presses.push_back(Press { at: Instant::now(), dir, from_slider });
            drop(presses);
            shared.arrived.notify_all();
            shared.hid_volume.fetch_add(1, Ordering::Relaxed);
        }
        shared.status.lock().unwrap().device = false;
    }
}

// ─────────────────────────── hook clavier ───────────────────────────

fn hook_thread() {
    // SAFETY : appels Win32 classiques ; le hook vit tant que ce fil pompe ses messages.
    unsafe {
        let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), GetModuleHandleW(null()), 0);
        if let Some(shared) = SHARED.get() {
            shared.status.lock().unwrap().hook = !hook.is_null();
        }
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

unsafe extern "system" fn hook_proc(code: i32, wparam: usize, lparam: isize) -> isize {
    if code == HC_ACTION as i32 {
        // SAFETY : pour WH_KEYBOARD_LL, lparam pointe sur un KBDLLHOOKSTRUCT valide.
        let kb = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
        let down = wparam as u32 == WM_KEYDOWN || wparam as u32 == WM_SYSKEYDOWN;
        if down {
            if kb.flags & LLKHF_INJECTED == 0 {
                layer_detect::on_key_down(kb.scanCode, kb.flags & LLKHF_EXTENDED != 0);
            }
            if let Some(shared) = SHARED.get() {
                shared.hook_keys.fetch_add(1, Ordering::Relaxed);
            }
        }
        let dir = match kb.vkCode as VIRTUAL_KEY {
            VK_VOLUME_UP => Some(Dir::Up),
            VK_VOLUME_DOWN => Some(Dir::Down),
            _ => None,
        };
        // Les touches volume d'un contrôle multimédia peuvent arriver marquées « injectées »
        // par Windows ; Lowfreed n'en injecte jamais, on les traite toutes.
        if let (Some(dir), Some(shared)) = (dir, SHARED.get()) {
            if down {
                shared.hook_volume.fetch_add(1, Ordering::Relaxed);
                shared.hook_volume_flags.store(kb.flags, Ordering::Relaxed);
            }
            if shared.filter(dir, down) {
                return 1; // bloquée : Windows ne change pas le volume
            }
        }
    }
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
}

impl Shared {
    /// Décide si une touche volume doit être bloquée (et déclenche l'action de remplacement).
    fn filter(&self, dir: Dir, down: bool) -> bool {
        let slot = &self.swallowed[dir as usize];
        if !down {
            return slot.swap(false, Ordering::Relaxed);
        }
        if self.config.lock().unwrap().action == Action::Volume {
            return false;
        }
        // Le rapport HID peut arriver juste après la touche virtuelle : on l'attend un peu.
        let deadline = Instant::now() + HOOK_WAIT;
        let mut presses = self.presses.lock().unwrap();
        let press = loop {
            presses.retain(|p| p.at.elapsed() < PRESS_MAX_AGE);
            if let Some(i) = presses.iter().position(|p| p.dir == dir) {
                break presses.remove(i);
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break None; // pas un appui du Flow 2 (autre clavier, autre périphérique)
            }
            presses = self.arrived.wait_timeout(presses, left).unwrap().0;
        };
        drop(presses);
        let Some(press) = press else { return false };

        let mut status = self.status.lock().unwrap();
        status.last = Some(LastEvent {
            at: press.at,
            dir,
            from_slider: press.from_slider,
            replaced: press.from_slider,
        });
        if !press.from_slider {
            return false;
        }
        status.replaced += 1;
        drop(status);
        slot.store(true, Ordering::Relaxed);
        let _ = self.actions.lock().unwrap().send(dir);
        true
    }
}

// ─────────────────────────── actions ───────────────────────────

fn worker(shared: Arc<Shared>, rx: Receiver<Dir>) {
    let mut gesture = Gesture::default();
    for dir in rx {
        let config = *shared.config.lock().unwrap();
        if let Some(dir) = gesture.feed(dir, config, Instant::now()) {
            perform(config.action, dir);
        }
    }
}

/// Applique le sens et la sensibilité : une action tous les `step` crans d'un même geste.
#[derive(Default)]
struct Gesture {
    count: u8,
    last: Option<(Instant, Dir)>,
}

impl Gesture {
    fn feed(&mut self, dir: Dir, config: SliderConfig, now: Instant) -> Option<Dir> {
        let dir = if config.invert { dir.flip() } else { dir };
        let same = self
            .last
            .is_some_and(|(at, d)| d == dir && now.duration_since(at) < GESTURE_GAP);
        self.count = if same { self.count + 1 } else { 1 };
        self.last = Some((now, dir));
        if self.count >= config.step.max(1) {
            self.count = 0;
            Some(dir)
        } else {
            None
        }
    }
}

fn key(vk: VIRTUAL_KEY, up: bool, extended: bool) -> INPUT {
    let mut flags = if up { KEYEVENTF_KEYUP } else { 0 };
    if extended {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: flags, time: 0, dwExtraInfo: 0 },
        },
    }
}

fn wheel(horizontal: bool, delta: i32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: delta as u32,
                dwFlags: if horizontal { MOUSEEVENTF_HWHEEL } else { MOUSEEVENTF_WHEEL },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn tap(vk: VIRTUAL_KEY, extended: bool) -> Vec<INPUT> {
    vec![key(vk, false, extended), key(vk, true, extended)]
}

fn perform(action: Action, dir: Dir) {
    let up = dir == Dir::Up;
    let delta = if up { 120 } else { -120 };
    let inputs = match action {
        Action::Volume => return,
        Action::Scroll => vec![wheel(false, delta)],
        Action::HScroll => vec![wheel(true, delta)],
        Action::Zoom => vec![key(VK_CONTROL, false, false), wheel(false, delta), key(VK_CONTROL, true, false)],
        Action::Tabs => {
            let mut v = vec![key(VK_CONTROL, false, false)];
            if !up {
                v.push(key(VK_SHIFT, false, false));
            }
            v.extend(tap(VK_TAB, false));
            if !up {
                v.push(key(VK_SHIFT, true, false));
            }
            v.push(key(VK_CONTROL, true, false));
            v
        }
        Action::Tracks => tap(if up { VK_MEDIA_NEXT_TRACK } else { VK_MEDIA_PREV_TRACK }, true),
        Action::Arrows => tap(if up { VK_RIGHT } else { VK_LEFT }, true),
    };
    // SAFETY : tableau d'INPUT initialisés, taille exacte.
    unsafe {
        SendInput(inputs.len() as u32, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gesture_step_and_invert() {
        let t0 = Instant::now();
        let ms = |n| t0 + Duration::from_millis(n);
        let cfg = SliderConfig { action: Action::Scroll, invert: false, step: 2 };
        let mut g = Gesture::default();
        assert_eq!(g.feed(Dir::Up, cfg, ms(0)), None);
        assert_eq!(g.feed(Dir::Up, cfg, ms(30)), Some(Dir::Up));
        assert_eq!(g.feed(Dir::Up, cfg, ms(60)), None);
        // Changement de sens : nouveau geste.
        assert_eq!(g.feed(Dir::Down, cfg, ms(90)), None);
        assert_eq!(g.feed(Dir::Down, cfg, ms(120)), Some(Dir::Down));
        // Pause : nouveau geste.
        assert_eq!(g.feed(Dir::Down, cfg, ms(1000)), None);

        let inv = SliderConfig { invert: true, step: 1, ..cfg };
        assert_eq!(Gesture::default().feed(Dir::Up, inv, ms(0)), Some(Dir::Down));
    }
}
