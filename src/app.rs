//! Interface graphique (egui).

use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::{
        mpsc::{self, Receiver, Sender},
        Arc,
    },
    time::{Duration, Instant},
};

use anyhow::{bail, Result};
use eframe::egui::{
    self, vec2, Color32, CornerRadius, FontFamily, FontId, Painter, Rect, RichText, Sense, Stroke,
    StrokeKind,
};
use lowfreed::{
    backup::Backup,
    keycodes::{self, Category},
    layout::Definition,
    macros,
    slider::{Action, Dir, Engine},
    system::{autostart, Settings},
    update::{self, Updater},
    via::{backlight, Via},
};

use crate::tray::TrayCommand;

const ACCENT: Color32 = Color32::from_rgb(255, 106, 0);
const KEY: Color32 = Color32::from_rgb(46, 46, 51);
const KEY_TEXT: Color32 = Color32::from_rgb(232, 232, 236);
const DIM_TEXT: Color32 = Color32::from_rgb(120, 120, 128);
const ERROR: Color32 = Color32::from_rgb(235, 87, 87);
const OK: Color32 = Color32::from_rgb(110, 200, 120);

/// Rôle des couches dans le keymap d'usine du Flow 2 (lu sur le clavier).
const LAYER_NAMES: [&str; 6] = ["Mac", "Fn Mac", "Windows", "Fn Windows", "Système", "Système 2"];

const RECONNECT_EVERY: Duration = Duration::from_secs(2);
const TESTER_POLL: Duration = Duration::from_millis(30);

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Keymap,
    Macros,
    Lighting,
    Slider,
    Tester,
}

struct Device {
    via: Via,
    protocol: u16,
    layers: u8,
    keymap: Vec<u16>,
    macro_count: u8,
    macro_size: u16,
    /// Luminosité (0-255) et effet ; `None` si le firmware ne gère pas le canal.
    backlight: Option<(u8, u8)>,
}

impl Device {
    fn load(def: &Definition) -> Result<(Self, Vec<String>)> {
        let via = Via::open(def.vendor_id, def.product_id)?;
        let protocol = via.protocol_version()?;
        if protocol < 9 {
            bail!("protocole VIA {protocol} trop ancien");
        }
        let layers = via.layer_count()?;
        let keymap = via.read_keymap(layers, def.matrix.rows, def.matrix.cols)?;
        let macro_count = via.macro_count()?;
        let macro_size = via.macro_buffer_size()?;
        let texts = macros::decode_buffer(&via.read_macro_buffer(macro_size)?, macro_count as usize);
        let backlight = via
            .custom_get(backlight::CHANNEL, backlight::BRIGHTNESS)
            .ok()
            .map(|b| (b, via.custom_get(backlight::CHANNEL, backlight::EFFECT).unwrap_or(0)));
        let dev = Self { via, protocol, layers, keymap, macro_count, macro_size, backlight };
        Ok((dev, texts))
    }
}

/// Éditeur du sous-onglet « Avancé ».
#[derive(Default)]
struct Advanced {
    ctrl: bool,
    shift: bool,
    alt: bool,
    gui: bool,
    right: bool,
    base: String,
    layer: u8,
    raw: String,
}

impl Advanced {
    fn mods(&self) -> u8 {
        use keycodes::*;
        let mut m = 0;
        for (on, bit) in [(self.ctrl, MOD_CTRL), (self.shift, MOD_SHIFT), (self.alt, MOD_ALT), (self.gui, MOD_GUI)] {
            if on {
                m |= bit;
            }
        }
        if m != 0 && self.right {
            m |= MOD_RIGHT;
        }
        m
    }
}

enum Pending {
    Reset,
    Import(PathBuf, Backup),
}

enum ConnectResult {
    Success(Device, Vec<String>),
    NotFound,
    BluetoothDetected,
    WirelessSleeping,
    NotReady(String),
}

pub struct App {
    def: Definition,
    dev: Option<Device>,
    connecting: bool,
    bluetooth_detected: bool,
    connect_tx: Sender<ConnectResult>,
    connect_rx: Receiver<ConnectResult>,
    last_attempt: Option<Instant>,
    status: (String, Color32),
    tab: Tab,
    layer: usize,
    selected: Option<(u8, u8)>,
    category: Category,
    advanced: Advanced,
    macro_texts: Vec<String>,
    macro_saved: Vec<String>,
    pressed: Vec<bool>,
    seen: BTreeSet<(u8, u8)>,
    pending: Option<Pending>,
    engine: Engine,
    settings: Settings,
    tray_rx: Receiver<TrayCommand>,
    quitting: bool,
    /// Commande de lancement au démarrage enregistrée dans le registre, s'il y en a une.
    autostart: Option<String>,
    /// Démarrage `--tray` : eframe affiche la fenêtre après la première image, on la remasque.
    hide_frames: u8,
    /// Dernière couche active détectée et déjà présélectionnée.
    detected_layer: Option<u8>,
    updater: Updater,
    show_update_modal: bool,
}

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        def: Definition,
        engine: Engine,
        settings: Settings,
        tray_rx: Receiver<TrayCommand>,
        start_hidden: bool,
    ) -> Self {
        setup_style(&cc.egui_ctx);
        let (connect_tx, connect_rx) = mpsc::channel();
        let updater = Updater::new();
        updater.check(cc.egui_ctx.clone());
        Self {
            layer: settings.last_layer.unwrap_or(0) as usize,
            detected_layer: None,
            hide_frames: if start_hidden { 3 } else { 0 },
            engine,
            settings,
            tray_rx,
            quitting: false,
            autostart: autostart::registered(),
            def,
            dev: None,
            connecting: false,
            bluetooth_detected: false,
            connect_tx,
            connect_rx,
            last_attempt: None,
            status: ("Recherche du clavier…".into(), DIM_TEXT),
            tab: Tab::Keymap,
            selected: None,
            category: Category::Basic,
            advanced: Advanced::default(),
            macro_texts: Vec::new(),
            macro_saved: Vec::new(),
            pressed: Vec::new(),
            seen: BTreeSet::new(),
            pending: None,
            updater,
            show_update_modal: false,
        }
    }

    fn info(&mut self, msg: impl Into<String>) {
        self.status = (msg.into(), OK);
    }

    fn error(&mut self, msg: impl Into<String>) {
        self.status = (msg.into(), ERROR);
    }

    fn connect(&mut self, ctx: &egui::Context) {
        if self.connecting {
            return;
        }
        self.connecting = true;
        self.last_attempt = Some(Instant::now());
        let def = self.def.clone();
        let tx = self.connect_tx.clone();
        let egui_ctx = ctx.clone();
        std::thread::Builder::new()
            .name("device-connect".into())
            .spawn(move || {
                let res = match Device::load(&def) {
                    Ok((dev, texts)) => ConnectResult::Success(dev, texts),
                    Err(e) => {
                        let via_found = Via::open(def.vendor_id, def.product_id);
                        match via_found {
                            Ok(v) if v.info.is_wireless() => ConnectResult::WirelessSleeping,
                            Ok(_) => ConnectResult::NotReady(format!("{e:#}")),
                            Err(_) => {
                                let has_bt = lowfreed::via::with_hidapi(|api| {
                                    Ok(api.device_list().any(|d| {
                                        d.product_string()
                                            .map(|p| p.contains("Flow2") || p.contains("Lofree"))
                                            .unwrap_or(false)
                                    }))
                                })
                                .unwrap_or(false);
                                if has_bt {
                                    ConnectResult::BluetoothDetected
                                } else {
                                    ConnectResult::NotFound
                                }
                            }
                        }
                    }
                };
                let _ = tx.send(res);
                egui_ctx.request_repaint();
            })
            .expect("spawn device-connect");
    }

    /// Affiche l'erreur d'une opération ; se déconnecte si le clavier ne répond plus.
    fn check<T>(&mut self, what: &str, r: Result<T>) -> Option<T> {
        match r {
            Ok(v) => Some(v),
            Err(e) => {
                self.error(format!("{what} : {e:#}"));
                if self.dev.as_ref().is_some_and(|d| d.via.protocol_version().is_err()) {
                    self.dev = None;
                    self.last_attempt = Some(Instant::now());
                }
                None
            }
        }
    }

    fn custom_names(&self) -> &[String] {
        &self.def.custom_keycodes
    }

    fn assign(&mut self, code: u16) {
        let (Some((row, col)), Some(dev)) = (self.selected, &self.dev) else {
            self.error("Sélectionnez d'abord une touche sur le clavier");
            return;
        };
        let layer = self.layer;
        let r = dev.via.set_keycode(layer as u8, row, col, code);
        if self.check("Écriture de la touche", r).is_some() {
            let idx = self.def.index(layer, row, col);
            if let Some(dev) = &mut self.dev {
                dev.keymap[idx] = code;
            }
            self.engine.keymap_changed();
            self.info(format!(
                "Couche {layer}, touche {row},{col} → {} (enregistré dans le clavier)",
                keycodes::name(code)
            ));
        }
    }

    fn reload_keymap(&mut self) {
        let Some(dev) = &self.dev else { return };
        let r = dev.via.read_keymap(dev.layers, self.def.matrix.rows, self.def.matrix.cols);
        if let Some(km) = self.check("Lecture du keymap", r) {
            if let Some(dev) = &mut self.dev {
                dev.keymap = km;
            }
        }
        self.engine.keymap_changed();
    }

    fn export(&mut self) {
        let Some(dev) = &self.dev else { return };
        let Some(path) = rfd::FileDialog::new()
            .set_file_name("flow2-sauvegarde.layout.json")
            .add_filter("Sauvegarde VIA", &["json"])
            .save_file()
        else {
            return;
        };
        let backup = Backup::from_state(&self.def, &dev.keymap, self.macro_saved.clone());
        let r = serde_json::to_string_pretty(&backup)
            .map_err(anyhow::Error::from)
            .and_then(|json| std::fs::write(&path, json).map_err(Into::into));
        if self.check("Export", r).is_some() {
            self.info(format!("Sauvegarde écrite : {}", path.display()));
        }
    }

    fn pick_import(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Sauvegarde VIA", &["json"])
            .pick_file()
        else {
            return;
        };
        let r = std::fs::read_to_string(&path)
            .map_err(anyhow::Error::from)
            .and_then(|s| serde_json::from_str::<Backup>(&s).map_err(Into::into));
        let layers = self.dev.as_ref().map_or(0, |d| d.layers);
        let r = r.and_then(|b| b.keymap(&self.def, layers).map(|_| b));
        if let Some(backup) = self.check("Import", r) {
            self.pending = Some(Pending::Import(path, backup));
        }
    }

    // ───────────────────────────── rendu ─────────────────────────────

    fn header(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Lowfreed").size(20.0).strong().color(ACCENT));
            ui.label(RichText::new(format!("v{}", update::CURRENT_VERSION)).size(13.0).color(DIM_TEXT));

            match self.updater.status() {
                update::UpdateStatus::Available { ref version, .. } => {
                    if ui
                        .button(RichText::new(format!("⚡ Mise à jour {version} disponible")).color(ACCENT).strong())
                        .on_hover_text("Cliquez pour voir les détails et installer")
                        .clicked()
                    {
                        self.show_update_modal = true;
                    }
                }
                update::UpdateStatus::Downloading { ref version, progress, .. } => {
                    ui.label(
                        RichText::new(format!("⚡ Téléchargement {version} : {:.0} %", progress * 100.0))
                            .color(ACCENT),
                    );
                }
                update::UpdateStatus::ReadyToRestart { ref version } => {
                    if ui
                        .button(RichText::new(format!("✓ {version} installée — Redémarrer")).color(OK).strong())
                        .on_hover_text("Cliquez pour relancer Lowfreed avec la nouvelle version")
                        .clicked()
                    {
                        let _ = self.updater.restart();
                    }
                }
                _ => {}
            }

            match &self.dev {
                Some(d) => ui.label(format!(
                    "{}  ·  VIA v{}  ·  {} couches  ·  {} macros",
                    d.via.info.product, d.protocol, d.layers, d.macro_count
                )),
                None => ui.label(RichText::new("Clavier non connecté").color(DIM_TEXT)),
            };
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let connected = self.dev.is_some();
                if ui
                    .add_enabled(connected, egui::Button::new("Réinitialiser…"))
                    .on_hover_text("Restaure le keymap d'usine du firmware")
                    .clicked()
                {
                    self.pending = Some(Pending::Reset);
                }
                if ui
                    .add_enabled(connected, egui::Button::new("Importer…"))
                    .on_hover_text("Recharge une sauvegarde (.layout.json)")
                    .clicked()
                {
                    self.pick_import();
                }
                if ui
                    .add_enabled(connected, egui::Button::new("Exporter…"))
                    .on_hover_text("Sauvegarde keymap + macros dans un fichier")
                    .clicked()
                {
                    self.export();
                }
                if ui.button("Recharger").on_hover_text("Relit tout depuis le clavier").clicked() {
                    self.connect(ui.ctx());
                }
            });
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            for (tab, title) in [
                (Tab::Keymap, "Keymap"),
                (Tab::Macros, "Macros"),
                (Tab::Lighting, "Rétroéclairage"),
                (Tab::Slider, "Slider"),
                (Tab::Tester, "Testeur de touches"),
            ] {
                ui.selectable_value(&mut self.tab, tab, RichText::new(title).size(15.0));
            }
        });
        ui.add_space(4.0);
    }

    fn layer_bar(&mut self, ui: &mut egui::Ui, layers: u8) {
        let active = self.detected_layer.map(|l| l as usize);
        ui.horizontal(|ui| {
            ui.label("Couche :");
            for l in 0..layers as usize {
                let name = LAYER_NAMES.get(l).copied().unwrap_or("");
                let mut text = if name.is_empty() { format!("{l}") } else { format!("{l} · {name}") };
                let mut hover = "Rôle d'après le keymap d'usine du Flow 2".to_string();
                if active == Some(l) {
                    text = format!("● {text}");
                    hover.push_str("\nCouche en service sur le clavier (détectée)");
                }
                ui.selectable_value(&mut self.layer, l, text).on_hover_text(hover);
            }
            if active.is_none() {
                ui.label(
                    RichText::new("couche en service inconnue : appuyez sur Alt ou Win du Flow 2").color(DIM_TEXT),
                );
            }
        });
    }

    fn keymap_tab(&mut self, ui: &mut egui::Ui) {
        let Some(dev) = &self.dev else {
            not_connected(ui, self.bluetooth_detected);
            return;
        };
        let layers = dev.layers;
        let layer = self.layer;
        self.layer_bar(ui, layers);
        ui.add_space(6.0);

        let Some(dev) = &self.dev else { return };
        let custom = self.def.custom_keycodes.clone();
        let selected = self.selected;
        let clicked = keyboard(ui, &self.def, |row, col| {
            let code = dev.keymap[self.def.index(layer, row, col)];
            let mut v = KeyVisual::for_code(code, &custom);
            if selected == Some((row, col)) {
                v.fill = mix(v.fill, ACCENT, 0.25);
                v.stroke = Stroke::new(2.5, ACCENT);
            }
            v
        });
        if let Some(k) = clicked {
            self.selected = if self.selected == Some(k) { None } else { Some(k) };
        }

        ui.add_space(8.0);
        match self.selected {
            Some((row, col)) => {
                let code = self.dev.as_ref().map_or(0, |d| d.keymap[self.def.index(layer, row, col)]);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("Touche {row},{col} — couche {layer} :")).strong());
                    ui.label(keycodes::describe(code, &custom).replace('\n', " — "));
                });
            }
            None => {
                ui.label(RichText::new("Cliquez une touche, puis choisissez sa nouvelle fonction ci-dessous.").color(DIM_TEXT));
            }
        }
        ui.separator();
        self.picker(ui);
    }

    fn picker(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            for cat in Category::ALL {
                ui.selectable_value(&mut self.category, cat, cat.title());
            }
        });
        ui.add_space(4.0);
        let Some(dev) = &self.dev else { return };
        if self.category == Category::Advanced {
            self.advanced_editor(ui);
            return;
        }
        let custom = self.custom_names().to_vec();
        let mut codes = keycodes::category_keycodes(self.category, dev.layers, dev.macro_count, &custom);
        if self.category == Category::Special {
            // Keycodes Lofree présents dans le keymap (fonctions sans fil, etc.).
            let present: BTreeSet<u16> =
                dev.keymap.iter().copied().filter(|k| (0x7E00..=0x7E3F).contains(k)).collect();
            codes.extend(present.into_iter().filter(|k| !codes.contains(k)).collect::<Vec<_>>());
        }
        let mut chosen = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                for code in codes {
                    let label = keycodes::label(code, &custom);
                    let text = if label.is_empty() { "∅".to_string() } else { label };
                    let resp = ui
                        .add(egui::Button::new(RichText::new(text).size(12.5)).min_size(vec2(56.0, 42.0)))
                        .on_hover_text(keycodes::describe(code, &custom));
                    if resp.clicked() {
                        chosen = Some(code);
                    }
                }
            });
            if self.category == Category::Special {
                ui.add_space(6.0);
                ui.label(
                    RichText::new(
                        "« KB n » : fonctions propres au firmware Lofree (connexions sans fil, etc.), \
                         non documentées. Les fonctions QMK dépendent de ce que le firmware a compilé.",
                    )
                    .color(DIM_TEXT),
                );
            }
            if self.category == Category::Macros {
                ui.add_space(6.0);
                ui.label(RichText::new("Le contenu des macros se règle dans l'onglet « Macros ».").color(DIM_TEXT));
            }
        });
        if let Some(code) = chosen {
            self.assign(code);
        }
    }

    fn advanced_editor(&mut self, ui: &mut egui::Ui) {
        let custom = self.custom_names().to_vec();
        let layers = self.dev.as_ref().map_or(1, |d| d.layers);
        let mut chosen = None;
        let a = &mut self.advanced;
        egui::Grid::new("advanced").num_columns(2).spacing(vec2(12.0, 8.0)).show(ui, |ui| {
            ui.label("Modificateurs");
            ui.horizontal(|ui| {
                ui.checkbox(&mut a.ctrl, "Ctrl");
                ui.checkbox(&mut a.shift, "Shift");
                ui.checkbox(&mut a.alt, "Alt");
                ui.checkbox(&mut a.gui, "Win/Cmd");
                ui.checkbox(&mut a.right, "côté droit");
            });
            ui.end_row();

            ui.label("Touche de base");
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut a.base).hint_text("ex. A, KC_ESC, f5").desired_width(140.0));
                let base = keycodes::parse(&a.base, &custom).filter(|k| *k <= 0xFF);
                match base {
                    Some(k) => ui.label(RichText::new(keycodes::name(k)).color(OK)),
                    None if a.base.trim().is_empty() => ui.label(""),
                    None => ui.label(RichText::new("touche simple inconnue").color(ERROR)),
                };
            });
            ui.end_row();

            ui.label("Couche (Layer-Tap)");
            ui.add(egui::DragValue::new(&mut a.layer).range(0..=layers.saturating_sub(1).min(15)));
            ui.end_row();
        });
        let mods = a.mods() as u16;
        let base = keycodes::parse(&a.base, &custom).filter(|k| *k <= 0xFF);
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            let combo = base.filter(|_| mods != 0).map(|k| mods << 8 | k);
            if ui
                .add_enabled(combo.is_some(), egui::Button::new("Modificateurs + touche"))
                .on_hover_text("Ex. Ctrl+C sur une seule touche")
                .clicked()
            {
                chosen = combo;
            }
            let mt = base.filter(|_| mods != 0).map(|k| 0x2000 | mods << 8 | k);
            if ui
                .add_enabled(mt.is_some(), egui::Button::new("Mod-Tap"))
                .on_hover_text("Tapée : la touche ; maintenue : les modificateurs")
                .clicked()
            {
                chosen = mt;
            }
            let lt = base.map(|k| 0x4000 | (a.layer as u16) << 8 | k);
            if ui
                .add_enabled(lt.is_some(), egui::Button::new(format!("Layer-Tap couche {}", a.layer)))
                .on_hover_text("Tapée : la touche ; maintenue : la couche")
                .clicked()
            {
                chosen = lt;
            }
        });
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.label("Keycode libre");
            ui.add(
                egui::TextEdit::singleline(&mut a.raw)
                    .hint_text("ex. LCTL(LSFT(KC_ESC)), TT(1), 0x7E0B")
                    .desired_width(260.0),
            );
            let parsed = keycodes::parse(&a.raw, &custom);
            if ui.add_enabled(parsed.is_some(), egui::Button::new("Appliquer")).clicked() {
                chosen = parsed;
            }
            if let Some(k) = parsed {
                ui.label(RichText::new(format!("{}  (0x{k:04X})", keycodes::name(k))).color(OK));
            } else if !a.raw.trim().is_empty() {
                ui.label(RichText::new("keycode non reconnu").color(ERROR));
            }
        });
        if let Some(code) = chosen {
            self.assign(code);
        }
    }

    fn macros_tab(&mut self, ui: &mut egui::Ui) {
        let Some(dev) = &self.dev else {
            not_connected(ui, self.bluetooth_detected);
            return;
        };
        let size = dev.macro_size as usize;
        ui.label(
            RichText::new(
                "Texte tapé tel quel · {KC_A} touche · {KC_LCTL,KC_C} raccourci · {+KC_LSFT} appui · \
                 {-KC_LSFT} relâche · {250} pause en ms · \\{ accolade",
            )
            .color(DIM_TEXT),
        );
        ui.label(
            RichText::new(
                "Le texte est tapé comme sur un clavier US : si Windows est en AZERTY, préférez des {KC_…}. \
                 Assignez ensuite M0…M15 à une touche (Keymap → Macros).",
            )
            .color(DIM_TEXT),
        );
        ui.add_space(6.0);

        let encoded = macros::encode_buffer(&self.macro_texts);
        let changed = self.macro_texts != self.macro_saved;
        ui.horizontal(|ui| {
            let ok_size = encoded.as_ref().is_ok_and(|b| b.len() <= size);
            if ui
                .add_enabled(changed && ok_size, egui::Button::new(RichText::new("Enregistrer dans le clavier").strong()))
                .clicked()
            {
                if let (Ok(buf), Some(dev)) = (&encoded, &self.dev) {
                    let r = dev.via.write_macro_buffer(dev.macro_size, buf);
                    if self.check("Écriture des macros", r).is_some() {
                        self.macro_saved = self.macro_texts.clone();
                        self.info("Macros enregistrées dans le clavier");
                    }
                }
            }
            if ui.add_enabled(changed, egui::Button::new("Annuler les modifications")).clicked() {
                self.macro_texts = self.macro_saved.clone();
            }
            match &encoded {
                Ok(buf) => {
                    let color = if buf.len() <= size { DIM_TEXT } else { ERROR };
                    ui.label(RichText::new(format!("{} / {size} octets", buf.len())).color(color));
                }
                Err(e) => {
                    ui.label(RichText::new(format!("{e:#}")).color(ERROR));
                }
            }
        });
        ui.add_space(6.0);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            egui::Grid::new("macros").num_columns(2).spacing(vec2(10.0, 6.0)).show(ui, |ui| {
                for (i, text) in self.macro_texts.iter_mut().enumerate() {
                    ui.label(RichText::new(format!("M{i}")).strong().monospace());
                    let err = macros::encode(text).err();
                    ui.vertical(|ui| {
                        ui.add(
                            egui::TextEdit::multiline(text)
                                .desired_rows(1)
                                .desired_width(f32::INFINITY)
                                .font(egui::TextStyle::Monospace),
                        );
                        if let Some(e) = err {
                            ui.label(RichText::new(format!("{e:#}")).color(ERROR).small());
                        }
                    });
                    ui.end_row();
                }
            });
        });
    }

    fn lighting_tab(&mut self, ui: &mut egui::Ui) {
        let Some(dev) = &self.dev else {
            not_connected(ui, self.bluetooth_detected);
            return;
        };
        let Some((mut brightness, effect)) = dev.backlight else {
            ui.label("Le firmware ne pilote pas le rétroéclairage par VIA.");
            return;
        };
        let mut breathing = effect != 0;
        ui.add_space(8.0);
        let mut changed_brightness = false;
        let mut changed_effect = false;

        ui.label(
            RichText::new(
                "Le Flow 2 intègre un rétroéclairage blanc sous chaque touche, piloté directement par son firmware. \
                 Les réglages sont appliqués en direct au clavier.",
            )
            .color(DIM_TEXT),
        );
        ui.add_space(10.0);

        egui::Grid::new("lighting").num_columns(2).spacing(vec2(16.0, 14.0)).show(ui, |ui| {
            ui.label(RichText::new("Luminosité").strong());
            ui.horizontal(|ui| {
                let resp = ui.add(egui::Slider::new(&mut brightness, 0..=255).custom_formatter(|v, _| {
                    format!("{:.0} %", v / 255.0 * 100.0)
                }));
                changed_brightness = resp.changed();

                ui.add_space(8.0);
                for &(label, val) in &[
                    ("0%", 0u8),
                    ("25%", 64u8),
                    ("50%", 128u8),
                    ("75%", 191u8),
                    ("100%", 255u8),
                ] {
                    if ui.button(label).clicked() {
                        brightness = val;
                        changed_brightness = true;
                    }
                }
            });
            ui.end_row();

            ui.label(RichText::new("Effet natif").strong());
            ui.horizontal(|ui| {
                if ui.radio_value(&mut breathing, false, "Fixe").changed() {
                    changed_effect = true;
                }
                if ui.radio_value(&mut breathing, true, "Respiration").changed() {
                    changed_effect = true;
                }
            });
            ui.end_row();
        });

        if changed_brightness {
            let r = dev.via.custom_set(backlight::CHANNEL, backlight::BRIGHTNESS, brightness);
            if self.check("Luminosité", r).is_some() {
                if let Some(dev) = &mut self.dev {
                    dev.backlight = Some((brightness, effect));
                }
            }
        }
        if changed_effect {
            let Some(dev) = &self.dev else { return };
            let value = breathing as u8;
            let r = dev.via.custom_set(backlight::CHANNEL, backlight::EFFECT, value);
            if self.check("Effet", r).is_some() {
                if let Some(dev) = &mut self.dev {
                    dev.backlight = Some((brightness, value));
                }
            }
        }

        ui.add_space(16.0);
        if ui.button(RichText::new("Enregistrer dans la mémoire du clavier").strong()).clicked() {
            let Some(dev) = &self.dev else { return };
            let r = dev.via.custom_save(backlight::CHANNEL);
            if self.check("Enregistrement du rétroéclairage", r).is_some() {
                self.info("Rétroéclairage enregistré (conservé après débranchement)");
            }
        }

        ui.add_space(8.0);
        ui.label(
            RichText::new(
                "« Enregistrer » sauvegarde la luminosité et le mode dans la mémoire flash permanente du clavier. \
                 Sur la couche Mac, F5/F6 règlent aussi la luminosité.",
            )
            .color(DIM_TEXT),
        );
    }

    fn save_settings(&mut self) {
        let r = self.settings.save();
        self.check("Enregistrement des réglages", r);
    }

    fn slider_tab(&mut self, ui: &mut egui::Ui) {
        #[cfg(target_os = "macos")]
        {
            ui.label(
                RichText::new(
                    "Le slider latéral du Flow 2 envoie « Volume + / − » (figé dans le firmware Lofree). \
                     Lowfreed l'intercepte via l'Accessibilité macOS pour le remplacer par l'action de votre choix tant que l'application tourne.",
                )
                .color(DIM_TEXT),
            );
            ui.add_space(8.0);
        }
        #[cfg(windows)]
        {
            ui.label(
                RichText::new(
                    "Le slider latéral envoie « Volume + / − » (et « Luminosité écran ± » avec Fn). Ce comportement \
                     est figé dans le firmware Lofree : VIA ne le voit pas. Lowfreed le remplace donc côté Windows, \
                     tant qu'il tourne (fenêtre ouverte ou icône dans la zone de notification). \
                     F11/F12 et les autres touches volume du keymap gardent le volume.",
                )
                .color(DIM_TEXT),
            );
            ui.add_space(8.0);
        }

        let status = self.engine.status();
        let trusted = lowfreed::slider::is_accessibility_trusted();

        ui.horizontal(|ui| {
            let (txt, color) = match (status.device, status.hook) {
                #[cfg(target_os = "macos")]
                _ if trusted && status.hook => ("● Interception active (Accessibilité accordée)", OK),
                #[cfg(target_os = "macos")]
                _ if !trusted => ("● Autorisation d'Accessibilité requise", ACCENT),
                (true, true) => ("● Slider surveillé", OK),
                (false, _) => ("● Flow 2 non détecté (USB / 2.4G)", DIM_TEXT),
                (true, false) => ("● Hook clavier indisponible", ERROR),
            };
            ui.label(RichText::new(txt).color(color));
            #[cfg(target_os = "macos")]
            if !trusted {
                if ui
                    .button("Autoriser dans Réglages Système…")
                    .on_hover_text("Ouvre Réglages Système > Confidentialité et sécurité > Accessibilité")
                    .clicked()
                {
                    lowfreed::slider::open_accessibility_settings();
                }
            }
            if let Some(last) = status.last {
                let ago = last.at.elapsed().as_secs_f32();
                let dir = if last.dir == Dir::Up { "+" } else { "−" };
                let prefix = if last.is_fn {
                    "Fn + slider"
                } else if last.from_slider {
                    "slider"
                } else {
                    "touche volume"
                };
                let done = if last.replaced { "bloqué / remplacé" } else { "laissé au système" };
                ui.label(RichText::new(format!("dernier : {prefix} {dir}, {done}, il y a {ago:.0} s")).color(DIM_TEXT));
            }
            ui.label(RichText::new(format!("{} crans interceptés", status.replaced)).color(DIM_TEXT));
        });
        ui.ctx().request_repaint_after(Duration::from_millis(300));
        ui.add_space(10.0);

        let mut cfg = self.settings.slider;
        ui.label(RichText::new("Action du slider (seul)").strong().size(16.0));
        ui.add_space(4.0);
        egui::Grid::new("slider-actions-normal").num_columns(2).spacing(vec2(16.0, 6.0)).show(ui, |ui| {
            for action in Action::ALL_NORMAL {
                ui.radio_value(&mut cfg.action, action, action.title());
                ui.label(RichText::new(action.detail()).color(DIM_TEXT));
                ui.end_row();
            }
        });
        ui.add_space(8.0);
        ui.add_enabled_ui(cfg.action != Action::Volume && cfg.action != Action::None, |ui| {
            ui.checkbox(&mut cfg.invert, "Inverser le sens");
            ui.horizontal(|ui| {
                ui.label("Sensibilité : une action tous les");
                ui.add(egui::Slider::new(&mut cfg.step, 1..=4).suffix(" cran(s)"));
            });
        });

        ui.add_space(12.0);
        ui.separator();
        ui.label(RichText::new("Action du slider avec touche Fn").strong().size(16.0));
        ui.add_space(4.0);
        egui::Grid::new("slider-actions-fn").num_columns(2).spacing(vec2(16.0, 6.0)).show(ui, |ui| {
            for action in Action::ALL_FN {
                ui.radio_value(&mut cfg.fn_action, action, action.title());
                ui.label(RichText::new(action.detail()).color(DIM_TEXT));
                ui.end_row();
            }
        });
        ui.add_space(8.0);
        ui.add_enabled_ui(cfg.fn_action != Action::Brightness && cfg.fn_action != Action::None, |ui| {
            ui.checkbox(&mut cfg.fn_invert, "Inverser le sens (Fn)");
            ui.horizontal(|ui| {
                ui.label("Sensibilité : une action tous les");
                ui.add(egui::Slider::new(&mut cfg.fn_step, 1..=4).suffix(" cran(s)"));
            });
        });

        if cfg != self.settings.slider {
            self.settings.slider = cfg;
            self.engine.set_config(cfg);
            self.save_settings();
        }

        ui.add_space(12.0);
        ui.separator();
        ui.label(RichText::new("Arrière-plan").strong().size(16.0));
        ui.add_space(4.0);
        if ui
            .checkbox(
                &mut self.settings.close_to_tray,
                "Fermer la fenêtre la réduit dans la zone de notification (sinon Lowfreed s'arrête)",
            )
            .changed()
        {
            self.save_settings();
        }
        let current = autostart::command().ok();
        let mut enabled = self.autostart.is_some();
        let autostart_label = if cfg!(target_os = "macos") {
            "Lancer Lowfreed à l'ouverture de session macOS (LaunchAgent)"
        } else {
            "Lancer Lowfreed au démarrage de Windows, dans la zone de notification"
        };
        if ui
            .checkbox(&mut enabled, autostart_label)
            .changed()
        {
            let r = autostart::set(enabled);
            self.check("Démarrage automatique", r);
            self.autostart = autostart::registered();
        }
        if let (Some(registered), Some(current)) = (self.autostart.clone(), current) {
            if registered != current {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("Le démarrage lance un autre exécutable : {registered}")).color(ERROR),
                    );
                    if ui.button("Utiliser celui-ci").clicked() {
                        let r = autostart::set(true);
                        self.check("Démarrage automatique", r);
                        self.autostart = autostart::registered();
                    }
                });
            }
        }
        ui.label(
            RichText::new("Icône près de l'horloge : clic gauche pour ouvrir, clic droit → Quitter.").color(DIM_TEXT),
        );

        ui.add_space(8.0);
        ui.separator();
        ui.label(RichText::new("Mises à jour").strong().size(16.0));
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(format!("Version installée : v{}", update::CURRENT_VERSION));
            ui.add_space(8.0);
            match self.updater.status() {
                update::UpdateStatus::Idle => {
                    if ui.button("Vérifier les mises à jour").clicked() {
                        self.updater.check(ui.ctx().clone());
                    }
                }
                update::UpdateStatus::Checking => {
                    ui.spinner();
                    ui.label(RichText::new("Recherche sur GitHub…").color(DIM_TEXT));
                }
                update::UpdateStatus::UpToDate => {
                    ui.label(RichText::new("● À jour").color(OK));
                    if ui.button("Revérifier").clicked() {
                        self.updater.check(ui.ctx().clone());
                    }
                }
                update::UpdateStatus::Available { ref version, .. } => {
                    ui.label(RichText::new(format!("● Version {version} disponible !")).color(ACCENT));
                    if ui.button(RichText::new("Mettre à jour").strong().color(ACCENT)).clicked() {
                        self.updater.install(ui.ctx().clone());
                    }
                    if ui.button("Voir les nouveautés").clicked() {
                        self.show_update_modal = true;
                    }
                }
                update::UpdateStatus::Downloading { ref version, progress, downloaded, total } => {
                    let mb_down = downloaded as f32 / (1024.0 * 1024.0);
                    let mb_tot = total as f32 / (1024.0 * 1024.0);
                    ui.label(format!("Téléchargement {version} :"));
                    ui.add(egui::ProgressBar::new(progress).text(format!("{mb_down:.1} / {mb_tot:.1} Mo")));
                }
                update::UpdateStatus::Applying => {
                    ui.spinner();
                    ui.label(RichText::new("Installation de la mise à jour…").color(ACCENT));
                }
                update::UpdateStatus::ReadyToRestart { ref version } => {
                    ui.label(RichText::new(format!("● Version {version} installée !")).color(OK));
                    if ui.button(RichText::new("Redémarrer l'application").strong().color(OK)).clicked() {
                        let _ = self.updater.restart();
                    }
                }
                update::UpdateStatus::Error(ref e) => {
                    ui.label(RichText::new(format!("Erreur : {e}")).color(ERROR));
                    if ui.button("Réessayer").clicked() {
                        self.updater.check(ui.ctx().clone());
                    }
                }
            }
        });
    }

    fn tester_tab(&mut self, ui: &mut egui::Ui) {
        let Some(dev) = &self.dev else {
            not_connected(ui, self.bluetooth_detected);
            return;
        };
        let (rows, cols) = (self.def.matrix.rows, self.def.matrix.cols);
        let r = dev.via.matrix_state(rows, cols);
        if let Some(state) = self.check("Lecture de la matrice", r) {
            for (i, &on) in state.iter().enumerate() {
                if on {
                    self.seen.insert(((i / cols as usize) as u8, (i % cols as usize) as u8));
                }
            }
            self.pressed = state;
        }
        ui.ctx().request_repaint_after(TESTER_POLL);

        let Some(dev) = &self.dev else { return };
        ui.horizontal(|ui| {
            ui.label(format!(
                "Appuyez sur les touches : {} / {} testées.",
                self.seen.len(),
                self.def.keys.len()
            ));
            if ui.button("Effacer").clicked() {
                self.seen.clear();
            }
        });
        ui.add_space(6.0);
        let custom = self.def.custom_keycodes.clone();
        let layer = self.layer;
        keyboard(ui, &self.def, |row, col| {
            let i = row as usize * cols as usize + col as usize;
            let code = dev.keymap[self.def.index(layer, row, col)];
            let mut v = KeyVisual::for_code(code, &custom);
            v.stroke = Stroke::NONE;
            if self.pressed.get(i).copied().unwrap_or(false) {
                v.fill = ACCENT;
                v.text = Color32::BLACK;
            } else if self.seen.contains(&(row, col)) {
                v.fill = Color32::from_rgb(38, 92, 52);
            }
            v
        });
        ui.add_space(6.0);
        ui.label(
            RichText::new("Libellés de la couche sélectionnée dans l'onglet Keymap. La touche Fn fait partie de la matrice.")
                .color(DIM_TEXT),
        );
    }

    fn pending_modal(&mut self, ctx: &egui::Context) {
        let Some(pending) = &self.pending else { return };
        let (title, body) = match pending {
            Pending::Reset => (
                "Réinitialiser le keymap ?".to_string(),
                "Toutes les couches reviennent au keymap d'usine du firmware. Les macros ne sont pas effacées. \
                 Pensez à exporter une sauvegarde avant."
                    .to_string(),
            ),
            Pending::Import(path, _) => (
                "Importer la sauvegarde ?".to_string(),
                format!("Le keymap et les macros du clavier seront remplacés par le contenu de\n{}", path.display()),
            ),
        };
        let mut confirm = false;
        let mut cancel = false;
        let resp = egui::Modal::new(egui::Id::unique("pending")).show(ctx, |ui| {
            ui.set_width(420.0);
            ui.heading(title);
            ui.add_space(6.0);
            ui.label(body);
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button(RichText::new("Confirmer").strong().color(ACCENT)).clicked() {
                    confirm = true;
                }
                if ui.button("Annuler").clicked() {
                    cancel = true;
                }
            });
        });
        if cancel || resp.should_close() {
            self.pending = None;
        }
        if !confirm {
            return;
        }
        let Some(pending) = self.pending.take() else { return };
        let Some(dev) = &self.dev else { return };
        match pending {
            Pending::Reset => {
                let r = dev.via.reset_keymap();
                if self.check("Réinitialisation", r).is_some() {
                    self.reload_keymap();
                    self.info("Keymap d'usine restauré");
                }
            }
            Pending::Import(path, backup) => {
                let r = backup.write(&dev.via, &self.def);
                if self.check("Import", r).is_some() {
                    self.engine.keymap_changed();
                    self.connect(ctx);
                    self.info(format!("Sauvegarde importée : {}", path.display()));
                }
            }
        }
    }

    fn update_modal(&mut self, ctx: &egui::Context) {
        if !self.show_update_modal {
            return;
        }
        let status = self.updater.status();
        let (version, notes, url, is_downloading, progress, is_ready) = match status {
            update::UpdateStatus::Available { ref version, ref notes, ref url, .. } => {
                (version.clone(), notes.clone(), url.clone(), false, 0.0, false)
            }
            update::UpdateStatus::Downloading { ref version, progress, .. } => {
                (version.clone(), String::new(), String::new(), true, progress, false)
            }
            update::UpdateStatus::ReadyToRestart { ref version } => {
                (version.clone(), String::new(), String::new(), false, 1.0, true)
            }
            _ => {
                self.show_update_modal = false;
                return;
            }
        };

        let mut close = false;
        let resp = egui::Modal::new(egui::Id::unique("update-modal")).show(ctx, |ui| {
            ui.set_width(480.0);
            if is_ready {
                ui.heading(format!("Mise à jour {version} installée !"));
                ui.add_space(8.0);
                ui.label("La nouvelle version a été installée avec succès. Redémarrez l'application pour l'appliquer.");
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    if ui.button(RichText::new("Redémarrer maintenant").strong().color(OK)).clicked() {
                        let _ = self.updater.restart();
                    }
                    if ui.button("Plus tard").clicked() {
                        close = true;
                    }
                });
            } else if is_downloading {
                ui.heading(format!("Téléchargement de {version}…"));
                ui.add_space(8.0);
                ui.add(egui::ProgressBar::new(progress).show_percentage());
                ui.add_space(8.0);
                ui.label(RichText::new("Veuillez patienter pendant le téléchargement et l'installation.").color(DIM_TEXT));
            } else {
                ui.heading(format!("Mise à jour {version} disponible"));
                ui.add_space(6.0);
                ui.label(format!("Une nouvelle version de Lowfreed est disponible (version actuelle : v{}).", update::CURRENT_VERSION));
                if !notes.is_empty() {
                    ui.add_space(8.0);
                    ui.label(RichText::new("Notes de version :").strong());
                    egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                        ui.label(RichText::new(&notes).size(12.0));
                    });
                }
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    if ui.button(RichText::new("Installer la mise à jour").strong().color(ACCENT)).clicked() {
                        self.updater.install(ctx.clone());
                    }
                    if !url.is_empty() {
                        ui.hyperlink_to("Voir sur GitHub", &url);
                    }
                    if ui.button("Plus tard").clicked() {
                        close = true;
                    }
                });
            }
        });

        if close || resp.should_close() {
            self.show_update_modal = false;
        }
    }
}

impl eframe::App for App {
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        if self.settings.close_to_tray && !self.quitting {
            let mut close_caught = false;
            for viewport in raw_input.viewports.values_mut() {
                if viewport.events.contains(&egui::ViewportEvent::Close) {
                    close_caught = true;
                    viewport.events.retain(|e| !matches!(e, egui::ViewportEvent::Close));
                }
            }
            if close_caught {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
        }
    }

    /// Appelée aussi quand la fenêtre est masquée : commandes de la zone de notification
    /// et fermeture → réduction.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.hide_frames > 0 {
            self.hide_frames -= 1;
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            ctx.request_repaint();
        }
        // Couche active détectée sur le clavier : on la présélectionne une fois par changement.
        if let Some(layer) = self.engine.active_layer() {
            if self.detected_layer != Some(layer) {
                self.detected_layer = Some(layer);
                self.layer = layer as usize;
                if self.settings.last_layer != Some(layer) {
                    self.settings.last_layer = Some(layer);
                    self.save_settings();
                }
            }
        }
        while let Ok(cmd) = self.tray_rx.try_recv() {
            match cmd {
                TrayCommand::Open => {
                    #[cfg(target_os = "macos")]
                    lowfreed::system::macos_app::activate_app();
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                TrayCommand::Quit => {
                    self.quitting = true;
                    #[cfg(target_os = "macos")]
                    lowfreed::system::macos_app::set_quitting(true);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    #[cfg(target_os = "macos")]
                    std::process::exit(0);
                }
            }
        }
        if self.settings.close_to_tray && !self.quitting {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if ctx.input(|i| i.viewport().close_requested()) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if self.settings.close_to_tray && !self.quitting {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if ui.ctx().input(|i| i.viewport().close_requested()) {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Visible(false));
            }
        }
        while let Ok(res) = self.connect_rx.try_recv() {
            self.connecting = false;
            match res {
                ConnectResult::Success(dev, texts) => {
                    self.bluetooth_detected = false;
                    self.layer = self.layer.min(dev.layers.saturating_sub(1) as usize);
                    let transport = dev.via.info.transport_label();
                    self.info(format!(
                        "{} connecté ({}) — protocole VIA {}, {} couches",
                        dev.via.info.product, transport, dev.protocol, dev.layers
                    ));
                    if dev.protocol < 12 {
                        self.error(format!(
                            "Protocole VIA {} : les keycodes avancés peuvent être mal interprétés",
                            dev.protocol
                        ));
                    }
                    self.macro_saved = texts.clone();
                    self.macro_texts = texts;
                    self.dev = Some(dev);
                }
                ConnectResult::WirelessSleeping => {
                    self.dev = None;
                    self.bluetooth_detected = false;
                    self.status = (
                        "Récepteur 2.4G détecté — clavier en veille ou éteint (appuyez sur une touche)".into(),
                        DIM_TEXT,
                    );
                }
                ConnectResult::NotReady(e) => {
                    self.dev = None;
                    self.bluetooth_detected = false;
                    self.status = (format!("Flow 2 filaire non prêt ({e})"), DIM_TEXT);
                }
                ConnectResult::BluetoothDetected => {
                    self.dev = None;
                    self.bluetooth_detected = true;
                    self.status = (
                        "Flow 2 détecté en Bluetooth — branchez le câble USB ou le dongle 2.4G pour configurer (VIA indisponible en Bluetooth)".into(),
                        DIM_TEXT,
                    );
                }
                ConnectResult::NotFound => {
                    self.dev = None;
                    self.bluetooth_detected = false;
                    self.status = (
                        "Flow 2 non détecté — connectez le récepteur 2.4G ou le câble USB".into(),
                        DIM_TEXT,
                    );
                }
            }
        }

        if self.dev.is_none()
            && !self.connecting
            && self.last_attempt.is_none_or(|t| t.elapsed() >= RECONNECT_EVERY)
        {
            self.connect(ui.ctx());
        }
        if self.dev.is_none() {
            ui.ctx().request_repaint_after(RECONNECT_EVERY);
        }

        egui::Panel::top("header").show(ui, |ui| self.header(ui));
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.add_space(3.0);
            ui.label(RichText::new(&self.status.0).color(self.status.1));
            ui.add_space(3.0);
        });
        egui::CentralPanel::default().show(ui, |ui| match self.tab {
            Tab::Keymap => {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.keymap_tab(ui));
            }
            Tab::Macros => self.macros_tab(ui),
            Tab::Lighting => {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.lighting_tab(ui));
            }
            Tab::Slider => {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.slider_tab(ui));
            }
            Tab::Tester => {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.tester_tab(ui));
            }
        });
        let ctx = ui.ctx().clone();
        self.pending_modal(&ctx);
        self.update_modal(&ctx);
    }
}

fn not_connected(ui: &mut egui::Ui, is_bluetooth: bool) {
    ui.add_space(40.0);
    ui.vertical_centered(|ui| {
        if is_bluetooth {
            ui.heading("Flow 2 détecté en Bluetooth");
            ui.add_space(8.0);
            ui.label("Le protocole de configuration VIA n'est pas actif en liaison Bluetooth.");
            ui.add_space(6.0);
            ui.label("Pour configurer vos touches, macros et rétroéclairage :");
            ui.add_space(8.0);
            ui.label(RichText::new("1. Branchez le câble USB-C (et passez le commutateur du clavier sur Câble)").strong());
            ui.label(RichText::new("— OU —").color(DIM_TEXT));
            ui.label(RichText::new("2. Branchez le récepteur sans-fil USB 2.4 GHz (dongle Lofree)").strong());
            ui.add_space(14.0);
            ui.label(RichText::new("Détection automatique dès le branchement…").color(DIM_TEXT));
        } else {
            ui.heading("Flow 2 non détecté");
            ui.label("Branchez le clavier avec le câble USB ou son récepteur 2.4G.");
            ui.label(RichText::new("Nouvelle tentative automatique toutes les 2 secondes.").color(DIM_TEXT));
        }
    });
}

// ─────────────────────────── dessin du clavier ───────────────────────────

struct KeyVisual {
    label: String,
    fill: Color32,
    text: Color32,
    stroke: Stroke,
}

impl KeyVisual {
    fn for_code(code: u16, custom: &[String]) -> Self {
        let (fill, text) = match code {
            0x0000 => (Color32::from_rgb(30, 30, 33), DIM_TEXT),
            0x0001 => (Color32::from_rgb(34, 34, 38), DIM_TEXT),
            0x7C00 | 0x7C03 => (Color32::from_rgb(96, 34, 34), KEY_TEXT),
            0x4000..=0x52FF => (Color32::from_rgb(36, 58, 86), KEY_TEXT),
            0x7700..=0x777F => (Color32::from_rgb(30, 74, 70), KEY_TEXT),
            0x7800..=0x78FF => (Color32::from_rgb(82, 70, 30), KEY_TEXT),
            0x7E00..=0x7FFF => (Color32::from_rgb(70, 50, 90), KEY_TEXT),
            0x00A5..=0x00DF | 0x0100..=0x3FFF => (Color32::from_rgb(56, 52, 60), KEY_TEXT),
            _ => (KEY, KEY_TEXT),
        };
        let stroke = match code {
            0x7C00 | 0x7C03 => Stroke::new(1.5, ERROR),
            _ => Stroke::new(1.0, Color32::from_rgb(70, 70, 78)),
        };
        Self { label: keycodes::label(code, custom), fill, text, stroke }
    }
}

/// Dessine le clavier ; renvoie la touche cliquée.
fn keyboard(
    ui: &mut egui::Ui,
    def: &Definition,
    visual: impl Fn(u8, u8) -> KeyVisual,
) -> Option<(u8, u8)> {
    let unit = (ui.available_width() / def.width).clamp(34.0, 64.0);
    let (area, _) = ui.allocate_exact_size(vec2(def.width * unit, def.height * unit), Sense::hover());
    let painter = ui.painter_at(area);
    let mut clicked = None;
    for k in &def.keys {
        let rect = Rect::from_min_size(area.min + vec2(k.x * unit, k.y * unit), vec2(k.w * unit, k.h * unit))
            .shrink(unit * 0.05);
        let resp = ui.interact(rect, ui.scope_id().with(("key", k.row, k.col)), Sense::click());
        let v = visual(k.row, k.col);
        let fill = if resp.hovered() { mix(v.fill, Color32::WHITE, 0.08) } else { v.fill };
        let radius = CornerRadius::same((unit * 0.12) as u8);
        painter.rect(rect, radius, fill, v.stroke, StrokeKind::Inside);
        draw_label(&painter, rect, &v.label, unit, v.text);
        if resp.clicked() {
            clicked = Some((k.row, k.col));
        }
    }
    clicked
}

/// Libellé centré, réduit si besoin pour tenir dans la touche.
fn draw_label(painter: &Painter, rect: Rect, label: &str, unit: f32, color: Color32) {
    if label.is_empty() {
        return;
    }
    let lines: Vec<&str> = label.split('\n').collect();
    let mut size = if lines.len() > 1 { unit * 0.21 } else { unit * 0.27 };
    let max_w = rect.width() - unit * 0.14;
    let layout = |size: f32| -> Vec<Arc<egui::Galley>> {
        lines
            .iter()
            .map(|l| painter.layout_no_wrap(l.to_string(), FontId::new(size, FontFamily::Proportional), color))
            .collect()
    };
    let mut galleys = layout(size);
    let widest = galleys.iter().map(|g| g.size().x).fold(0.0, f32::max);
    if widest > max_w {
        size = (size * max_w / widest).max(7.0);
        galleys = layout(size);
    }
    let total_h: f32 = galleys.iter().map(|g| g.size().y).sum();
    let mut y = rect.center().y - total_h / 2.0;
    for g in galleys {
        let x = rect.center().x - g.size().x / 2.0;
        let h = g.size().y;
        painter.galley(egui::pos2(x, y), g, color);
        y += h;
    }
}

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

fn setup_style(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let mut load = |name: &str, path: &str| {
        std::fs::read(path).ok().map(|bytes| {
            fonts.font_data.insert(name.to_string(), Arc::new(egui::FontData::from_owned(bytes)));
        })
    };
    let ui_font = load("segoe", r"C:\Windows\Fonts\segoeui.ttf").is_some();
    let symbols = load("segoe-symbol", r"C:\Windows\Fonts\seguisym.ttf").is_some();
    if ui_font {
        fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "segoe".into());
    }
    if symbols {
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            fonts.families.entry(family).or_default().push("segoe-symbol".into());
        }
    }
    ctx.set_fonts(fonts);
    ctx.set_theme(egui::Theme::Dark);
    let mut visuals = egui::Visuals::dark();
    visuals.selection.bg_fill = ACCENT.linear_multiply(0.55);
    visuals.selection.stroke = Stroke::new(1.0, ACCENT);
    visuals.hyperlink_color = ACCENT;
    ctx.set_visuals_of(egui::Theme::Dark, visuals);
}
