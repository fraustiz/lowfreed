//! Keycodes QMK au format du protocole VIA v12 : noms, libellés de touche et
//! catégories du sélecteur.

/// Keycode à nom fixe : `name` sert dans les sauvegardes et les macros,
/// `label` s'affiche sur la touche (`\n` sépare deux lignes).
pub struct Named {
    pub code: u16,
    pub name: &'static str,
    pub label: &'static str,
}

macro_rules! named {
    ($( $code:literal $name:literal $label:literal ),* $(,)?) => {
        &[ $( Named { code: $code, name: $name, label: $label } ),* ]
    };
}

pub const BASIC: &[Named] = named![
    0x00 "KC_NO" "",
    0x01 "KC_TRNS" "▽",
    0x04 "KC_A" "A", 0x05 "KC_B" "B", 0x06 "KC_C" "C", 0x07 "KC_D" "D",
    0x08 "KC_E" "E", 0x09 "KC_F" "F", 0x0A "KC_G" "G", 0x0B "KC_H" "H",
    0x0C "KC_I" "I", 0x0D "KC_J" "J", 0x0E "KC_K" "K", 0x0F "KC_L" "L",
    0x10 "KC_M" "M", 0x11 "KC_N" "N", 0x12 "KC_O" "O", 0x13 "KC_P" "P",
    0x14 "KC_Q" "Q", 0x15 "KC_R" "R", 0x16 "KC_S" "S", 0x17 "KC_T" "T",
    0x18 "KC_U" "U", 0x19 "KC_V" "V", 0x1A "KC_W" "W", 0x1B "KC_X" "X",
    0x1C "KC_Y" "Y", 0x1D "KC_Z" "Z",
    0x1E "KC_1" "!\n1", 0x1F "KC_2" "@\n2", 0x20 "KC_3" "#\n3", 0x21 "KC_4" "$\n4",
    0x22 "KC_5" "%\n5", 0x23 "KC_6" "^\n6", 0x24 "KC_7" "&\n7", 0x25 "KC_8" "*\n8",
    0x26 "KC_9" "(\n9", 0x27 "KC_0" ")\n0",
    0x28 "KC_ENT" "Enter", 0x29 "KC_ESC" "Esc", 0x2A "KC_BSPC" "Bksp", 0x2B "KC_TAB" "Tab",
    0x2C "KC_SPC" "Space", 0x2D "KC_MINS" "_\n-", 0x2E "KC_EQL" "+\n=", 0x2F "KC_LBRC" "{\n[",
    0x30 "KC_RBRC" "}\n]", 0x31 "KC_BSLS" "|\n\\", 0x32 "KC_NUHS" "ISO #", 0x33 "KC_SCLN" ":\n;",
    0x34 "KC_QUOT" "\"\n'", 0x35 "KC_GRV" "~\n`", 0x36 "KC_COMM" "<\n,", 0x37 "KC_DOT" ">\n.",
    0x38 "KC_SLSH" "?\n/", 0x39 "KC_CAPS" "Caps",
    0x3A "KC_F1" "F1", 0x3B "KC_F2" "F2", 0x3C "KC_F3" "F3", 0x3D "KC_F4" "F4",
    0x3E "KC_F5" "F5", 0x3F "KC_F6" "F6", 0x40 "KC_F7" "F7", 0x41 "KC_F8" "F8",
    0x42 "KC_F9" "F9", 0x43 "KC_F10" "F10", 0x44 "KC_F11" "F11", 0x45 "KC_F12" "F12",
    0x46 "KC_PSCR" "PrtSc", 0x47 "KC_SCRL" "ScrLk", 0x48 "KC_PAUS" "Pause", 0x49 "KC_INS" "Ins",
    0x4A "KC_HOME" "Home", 0x4B "KC_PGUP" "PgUp", 0x4C "KC_DEL" "Del", 0x4D "KC_END" "End",
    0x4E "KC_PGDN" "PgDn", 0x4F "KC_RGHT" "→", 0x50 "KC_LEFT" "←", 0x51 "KC_DOWN" "↓",
    0x52 "KC_UP" "↑",
    0x53 "KC_NUM" "Num", 0x54 "KC_PSLS" "/", 0x55 "KC_PAST" "*", 0x56 "KC_PMNS" "-",
    0x57 "KC_PPLS" "+", 0x58 "KC_PENT" "Enter", 0x59 "KC_P1" "1", 0x5A "KC_P2" "2",
    0x5B "KC_P3" "3", 0x5C "KC_P4" "4", 0x5D "KC_P5" "5", 0x5E "KC_P6" "6",
    0x5F "KC_P7" "7", 0x60 "KC_P8" "8", 0x61 "KC_P9" "9", 0x62 "KC_P0" "0",
    0x63 "KC_PDOT" ".", 0x64 "KC_NUBS" "ISO \\", 0x65 "KC_APP" "Menu", 0x66 "KC_KB_POWER" "Power",
    0x67 "KC_PEQL" "=",
    0x68 "KC_F13" "F13", 0x69 "KC_F14" "F14", 0x6A "KC_F15" "F15", 0x6B "KC_F16" "F16",
    0x6C "KC_F17" "F17", 0x6D "KC_F18" "F18", 0x6E "KC_F19" "F19", 0x6F "KC_F20" "F20",
    0x70 "KC_F21" "F21", 0x71 "KC_F22" "F22", 0x72 "KC_F23" "F23", 0x73 "KC_F24" "F24",
    0x74 "KC_EXEC" "Exec", 0x75 "KC_HELP" "Help", 0x76 "KC_MENU" "Menu", 0x77 "KC_SLCT" "Select",
    0x78 "KC_STOP" "Stop", 0x79 "KC_AGIN" "Again", 0x7A "KC_UNDO" "Undo", 0x7B "KC_CUT" "Cut",
    0x7C "KC_COPY" "Copy", 0x7D "KC_PSTE" "Paste", 0x7E "KC_FIND" "Find",
    0x7F "KC_KB_MUTE" "Mute\n(kb)", 0x80 "KC_KB_VOLUME_UP" "Vol+\n(kb)", 0x81 "KC_KB_VOLUME_DOWN" "Vol-\n(kb)",
    0x82 "KC_LCAP" "LCaps", 0x83 "KC_LNUM" "LNum", 0x84 "KC_LSCR" "LScrl",
    0x85 "KC_PCMM" ",", 0x86 "KC_KP_EQUAL_AS400" "=\n400",
    0x87 "KC_INT1" "Ro", 0x88 "KC_INT2" "Kana", 0x89 "KC_INT3" "¥", 0x8A "KC_INT4" "Henk",
    0x8B "KC_INT5" "MHen", 0x8C "KC_INT6" "Int6", 0x8D "KC_INT7" "Int7", 0x8E "KC_INT8" "Int8",
    0x8F "KC_INT9" "Int9",
    0x90 "KC_LNG1" "Lang1", 0x91 "KC_LNG2" "Lang2", 0x92 "KC_LNG3" "Lang3", 0x93 "KC_LNG4" "Lang4",
    0x94 "KC_LNG5" "Lang5", 0x95 "KC_LNG6" "Lang6", 0x96 "KC_LNG7" "Lang7", 0x97 "KC_LNG8" "Lang8",
    0x98 "KC_LNG9" "Lang9",
    0x99 "KC_ERAS" "Erase", 0x9A "KC_SYRQ" "SysRq", 0x9B "KC_CNCL" "Cancel", 0x9C "KC_CLR" "Clear",
    0x9D "KC_PRIR" "Prior", 0x9E "KC_RETN" "Return", 0x9F "KC_SEPR" "Sep", 0xA0 "KC_OUT" "Out",
    0xA1 "KC_OPER" "Oper", 0xA2 "KC_CLAG" "ClrAg", 0xA3 "KC_CRSL" "CrSel", 0xA4 "KC_EXSL" "ExSel",
    0xA5 "KC_PWR" "Power", 0xA6 "KC_SLEP" "Sleep", 0xA7 "KC_WAKE" "Wake",
    0xA8 "KC_MUTE" "Mute", 0xA9 "KC_VOLU" "Vol+", 0xAA "KC_VOLD" "Vol-",
    0xAB "KC_MNXT" "Next", 0xAC "KC_MPRV" "Prev", 0xAD "KC_MSTP" "Stop", 0xAE "KC_MPLY" "Play",
    0xAF "KC_MSEL" "Media", 0xB0 "KC_EJCT" "Eject", 0xB1 "KC_MAIL" "Mail", 0xB2 "KC_CALC" "Calc",
    0xB3 "KC_MYCM" "PC", 0xB4 "KC_WSCH" "Search", 0xB5 "KC_WHOM" "Web\nHome", 0xB6 "KC_WBAK" "Web\nBack",
    0xB7 "KC_WFWD" "Web\nFwd", 0xB8 "KC_WSTP" "Web\nStop", 0xB9 "KC_WREF" "Web\nRefr",
    0xBA "KC_WFAV" "Web\nFav", 0xBB "KC_MFFD" "FFwd", 0xBC "KC_MRWD" "Rwd",
    0xBD "KC_BRIU" "Bri+", 0xBE "KC_BRID" "Bri-", 0xBF "KC_CPNL" "Ctrl\nPanel",
    0xC0 "KC_ASST" "Assist", 0xC1 "KC_MCTL" "Mission\nCtrl", 0xC2 "KC_LPAD" "Launch\npad",
    0xCD "KC_MS_U" "Souris\n↑", 0xCE "KC_MS_D" "Souris\n↓", 0xCF "KC_MS_L" "Souris\n←",
    0xD0 "KC_MS_R" "Souris\n→", 0xD1 "KC_BTN1" "Clic\n1", 0xD2 "KC_BTN2" "Clic\n2",
    0xD3 "KC_BTN3" "Clic\n3", 0xD4 "KC_BTN4" "Clic\n4", 0xD5 "KC_BTN5" "Clic\n5",
    0xD6 "KC_BTN6" "Clic\n6", 0xD7 "KC_BTN7" "Clic\n7", 0xD8 "KC_BTN8" "Clic\n8",
    0xD9 "KC_WH_U" "Molette\n↑", 0xDA "KC_WH_D" "Molette\n↓", 0xDB "KC_WH_L" "Molette\n←",
    0xDC "KC_WH_R" "Molette\n→", 0xDD "KC_ACL0" "Accel\n0", 0xDE "KC_ACL1" "Accel\n1",
    0xDF "KC_ACL2" "Accel\n2",
    0xE0 "KC_LCTL" "Ctrl", 0xE1 "KC_LSFT" "Shift", 0xE2 "KC_LALT" "Alt", 0xE3 "KC_LGUI" "Win",
    0xE4 "KC_RCTL" "RCtrl", 0xE5 "KC_RSFT" "RShift", 0xE6 "KC_RALT" "RAlt", 0xE7 "KC_RGUI" "RWin",
];

/// Keycodes spéciaux (hors plage basique) à nom fixe.
pub const SPECIAL: &[Named] = named![
    0x7000 "CL_SWAP" "Swap\nCtrl/Caps", 0x7001 "CL_NORM" "Norm\nCtrl/Caps",
    0x7005 "LAG_SWP" "Swap\nLAlt/Win", 0x7006 "LAG_NRM" "Norm\nLAlt/Win",
    0x7009 "GU_ON" "Win\nOn", 0x700A "GU_OFF" "Win\nOff", 0x700B "GU_TOGG" "Win\nTogg",
    0x700C "GE_SWAP" "Swap\nEsc/`", 0x700D "GE_NORM" "Norm\nEsc/`",
    0x7011 "NK_ON" "NKRO\nOn", 0x7012 "NK_OFF" "NKRO\nOff", 0x7013 "NK_TOGG" "NKRO\nTogg",
    0x7014 "AG_SWAP" "Swap\nAlt/Win", 0x7015 "AG_NORM" "Norm\nAlt/Win", 0x7016 "AG_TOGG" "Togg\nAlt/Win",
    0x7800 "BL_ON" "Rétro\nOn", 0x7801 "BL_OFF" "Rétro\nOff", 0x7802 "BL_TOGG" "Rétro\nTogg",
    0x7803 "BL_DOWN" "Rétro\n-", 0x7804 "BL_UP" "Rétro\n+", 0x7805 "BL_STEP" "Rétro\nCycle",
    0x7806 "BL_BRTG" "Rétro\nRespir.",
    0x7820 "RGB_TOG" "RGB\nTogg", 0x7821 "RGB_MOD" "RGB\nMode+", 0x7822 "RGB_RMOD" "RGB\nMode-",
    0x7823 "RGB_HUI" "Teinte\n+", 0x7824 "RGB_HUD" "Teinte\n-", 0x7825 "RGB_SAI" "Sat\n+",
    0x7826 "RGB_SAD" "Sat\n-", 0x7827 "RGB_VAI" "Lum\n+", 0x7828 "RGB_VAD" "Lum\n-",
    0x7829 "RGB_SPI" "Vit\n+", 0x782A "RGB_SPD" "Vit\n-",
    0x7C00 "QK_BOOT" "Boot", 0x7C01 "QK_RBT" "Reboot", 0x7C02 "DB_TOGG" "Debug", 0x7C03 "EE_CLR" "EE\nClear",
    0x7C16 "QK_GESC" "Esc\n`", 0x7C73 "CW_TOGG" "Caps\nWord",
];

/// Catégories du sélecteur de keycodes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Category {
    Basic,
    Numpad,
    Media,
    Mouse,
    Layers,
    Lighting,
    Special,
    Macros,
    Advanced,
}

impl Category {
    pub const ALL: [Category; 9] = [
        Category::Basic,
        Category::Numpad,
        Category::Media,
        Category::Mouse,
        Category::Layers,
        Category::Lighting,
        Category::Special,
        Category::Macros,
        Category::Advanced,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Category::Basic => "Basique",
            Category::Numpad => "Pavé num.",
            Category::Media => "Média & système",
            Category::Mouse => "Souris",
            Category::Layers => "Couches",
            Category::Lighting => "Rétroéclairage",
            Category::Special => "Spécial",
            Category::Macros => "Macros",
            Category::Advanced => "Avancé",
        }
    }
}

/// Keycodes proposés dans une catégorie (hors « Avancé », qui a son propre éditeur).
pub fn category_keycodes(cat: Category, layers: u8, macros: u8, custom: &[String]) -> Vec<u16> {
    let range = |a: u16, b: u16| (a..=b).collect::<Vec<_>>();
    match cat {
        Category::Basic => [
            vec![0x00, 0x01, 0x29],
            range(0x3A, 0x45),
            range(0x1E, 0x27),
            range(0x04, 0x1D),
            range(0x2D, 0x38).into_iter().filter(|&k| k != 0x32).collect(),
            vec![0x28, 0x2A, 0x2B, 0x2C, 0x39],
            range(0xE0, 0xE7),
            vec![0x49, 0x4A, 0x4B, 0x4C, 0x4D, 0x4E, 0x52, 0x50, 0x51, 0x4F],
            vec![0x46, 0x47, 0x48, 0x65],
        ]
        .concat(),
        Category::Numpad => [vec![0x53], range(0x54, 0x63), vec![0x67, 0x85]].concat(),
        Category::Media => [
            range(0xA8, 0xAE),
            vec![0xBB, 0xBC, 0xBD, 0xBE, 0xB0, 0xAF],
            range(0xB1, 0xBA),
            vec![0xBF, 0xC0, 0xC1, 0xC2, 0xA5, 0xA6, 0xA7],
        ]
        .concat(),
        Category::Mouse => range(0xCD, 0xDF),
        Category::Layers => {
            let mut v = Vec::new();
            for base in [0x5220u16, 0x5260, 0x5200, 0x5240, 0x5280, 0x52C0] {
                v.extend((0..layers as u16).map(|l| base + l));
            }
            v
        }
        Category::Lighting => range(0x7800, 0x7806),
        Category::Special => [
            vec![0x7C16, 0x7C73, 0x7013, 0x7014, 0x7015, 0x700C, 0x700D],
            (0..custom.len().max(1) as u16).map(|i| 0x7E00 + i).collect(),
            range(0x68, 0x73),
            range(0x74, 0x7E),
            range(0x87, 0x8B),
            range(0x90, 0x94),
            vec![0x32, 0x64],
        ]
        .concat(),
        Category::Macros => (0..macros as u16).map(|i| 0x7700 + i).collect(),
        Category::Advanced => Vec::new(),
    }
}

pub fn basic(code: u16) -> Option<&'static Named> {
    BASIC.iter().find(|k| k.code == code)
}

fn special(code: u16) -> Option<&'static Named> {
    SPECIAL.iter().find(|k| k.code == code)
}

/// Bits de modificateurs QMK (5 bits) : Ctrl, Shift, Alt, Gui, + bit « main droite ».
pub const MOD_CTRL: u8 = 0x01;
pub const MOD_SHIFT: u8 = 0x02;
pub const MOD_ALT: u8 = 0x04;
pub const MOD_GUI: u8 = 0x08;
pub const MOD_RIGHT: u8 = 0x10;

const MOD_FNS: [(u8, &str, &str); 4] = [
    (MOD_CTRL, "LCTL", "RCTL"),
    (MOD_SHIFT, "LSFT", "RSFT"),
    (MOD_ALT, "LALT", "RALT"),
    (MOD_GUI, "LGUI", "RGUI"),
];

fn mods_label(mods: u8) -> String {
    let side = if mods & MOD_RIGHT != 0 { "R" } else { "" };
    let mut parts = Vec::new();
    for (bit, name) in [(MOD_CTRL, "Ctrl"), (MOD_SHIFT, "Shift"), (MOD_ALT, "Alt"), (MOD_GUI, "Win")] {
        if mods & bit != 0 {
            parts.push(format!("{side}{name}"));
        }
    }
    parts.join("+")
}

/// Nom QMK des modificateurs, sous forme d'appels imbriqués autour de `inner`.
fn mods_wrap(mods: u8, inner: String) -> String {
    let right = mods & MOD_RIGHT != 0;
    MOD_FNS.iter().rev().fold(inner, |acc, (bit, l, r)| {
        if mods & bit != 0 {
            format!("{}({acc})", if right { r } else { l })
        } else {
            acc
        }
    })
}

fn mods_tap_name(mods: u8) -> Option<&'static str> {
    Some(match mods {
        0x01 => "LCTL_T",
        0x02 => "LSFT_T",
        0x04 => "LALT_T",
        0x08 => "LGUI_T",
        0x11 => "RCTL_T",
        0x12 => "RSFT_T",
        0x14 => "RALT_T",
        0x18 => "RGUI_T",
        _ => return None,
    })
}

fn basic_name(code: u8) -> String {
    basic(code as u16).map_or(format!("0x{code:02X}"), |k| k.name.to_string())
}

/// Libellé d'une touche basique sur une seule ligne (pour les combinaisons).
fn basic_short(code: u8) -> String {
    match basic(code as u16) {
        Some(k) => k.label.rsplit('\n').next().unwrap_or(k.label).to_string(),
        None => format!("{code:02X}"),
    }
}

/// Nom QMK d'un keycode, tel qu'il apparaît dans les sauvegardes.
pub fn name(code: u16) -> String {
    if let Some(k) = basic(code).or_else(|| special(code)) {
        return k.name.to_string();
    }
    let kc = (code & 0xFF) as u8;
    let mods = ((code >> 8) & 0x1F) as u8;
    match code {
        0x0100..=0x1FFF => mods_wrap(mods, basic_name(kc)),
        0x2000..=0x3FFF => match mods_tap_name(mods) {
            Some(f) => format!("{f}({})", basic_name(kc)),
            None => format!("MT(0x{mods:02X},{})", basic_name(kc)),
        },
        0x4000..=0x4FFF => format!("LT({},{})", (code >> 8) & 0xF, basic_name(kc)),
        0x5000..=0x51FF => format!("LM({},0x{:02X})", (code >> 5) & 0xF, code & 0x1F),
        0x5200..=0x521F => format!("TO({})", code & 0x1F),
        0x5220..=0x523F => format!("MO({})", code & 0x1F),
        0x5240..=0x525F => format!("DF({})", code & 0x1F),
        0x5260..=0x527F => format!("TG({})", code & 0x1F),
        0x5280..=0x529F => format!("OSL({})", code & 0x1F),
        0x52A0..=0x52BF => format!("OSM(0x{:02X})", code & 0x1F),
        0x52C0..=0x52DF => format!("TT({})", code & 0x1F),
        0x52E0..=0x52FF => format!("PDF({})", code & 0x1F),
        0x5700..=0x57FF => format!("TD({})", code & 0xFF),
        0x7700..=0x777F => format!("MACRO({})", code & 0x7F),
        0x7E00..=0x7E3F => format!("CUSTOM({})", code - 0x7E00),
        0x7E40..=0x7FFF => format!("USER({})", code - 0x7E40),
        _ => format!("0x{code:04X}"),
    }
}

/// Libellé court affiché sur la touche.
pub fn label(code: u16, custom: &[String]) -> String {
    if let Some(k) = basic(code).or_else(|| special(code)) {
        return k.label.to_string();
    }
    let kc = (code & 0xFF) as u8;
    let mods = ((code >> 8) & 0x1F) as u8;
    match code {
        0x0100..=0x1FFF => format!("{}+\n{}", mods_label(mods), basic_short(kc)),
        0x2000..=0x3FFF => format!("{}\n{}", basic_short(kc), mods_label(mods)),
        0x4000..=0x4FFF => format!("{}\nLT {}", basic_short(kc), (code >> 8) & 0xF),
        0x5000..=0x51FF => format!("LM {}\n{}", (code >> 5) & 0xF, mods_label((code & 0x1F) as u8)),
        0x5200..=0x521F => format!("TO {}", code & 0x1F),
        0x5220..=0x523F => format!("MO {}", code & 0x1F),
        0x5240..=0x525F => format!("DF {}", code & 0x1F),
        0x5260..=0x527F => format!("TG {}", code & 0x1F),
        0x5280..=0x529F => format!("OSL {}", code & 0x1F),
        0x52A0..=0x52BF => format!("OSM\n{}", mods_label((code & 0x1F) as u8)),
        0x52C0..=0x52DF => format!("TT {}", code & 0x1F),
        0x52E0..=0x52FF => format!("PDF {}", code & 0x1F),
        0x5700..=0x57FF => format!("TD {}", code & 0xFF),
        0x7700..=0x777F => format!("M{}", code & 0x7F),
        0x7E00..=0x7E3F => {
            let i = (code - 0x7E00) as usize;
            custom.get(i).cloned().unwrap_or_else(|| format!("KB {i}"))
        }
        0x7E40..=0x7FFF => format!("User\n{}", code - 0x7E40),
        _ => format!("{code:04X}"),
    }
}

/// Description lisible (infobulle).
pub fn describe(code: u16, custom: &[String]) -> String {
    let what = match code {
        0x0000 => "Aucune action".to_string(),
        0x0001 => "Transparent : reprend la touche de la couche inférieure".to_string(),
        0x0100..=0x1FFF => "Combinaison modificateurs + touche".to_string(),
        0x2000..=0x3FFF => "Mod-Tap : touche si tapée, modificateur si maintenue".to_string(),
        0x4000..=0x4FFF => "Layer-Tap : touche si tapée, couche si maintenue".to_string(),
        0x5200..=0x521F => "Active la couche (et désactive les autres)".to_string(),
        0x5220..=0x523F => "Couche active tant que la touche est maintenue".to_string(),
        0x5240..=0x525F => "Change la couche par défaut".to_string(),
        0x5260..=0x527F => "Bascule la couche".to_string(),
        0x5280..=0x529F => "Couche pour la prochaine touche seulement".to_string(),
        0x52C0..=0x52DF => "Maintenue : couche temporaire ; tapée plusieurs fois : bascule".to_string(),
        0x7700..=0x777F => "Macro (onglet Macros)".to_string(),
        0x7E00..=0x7E3F => match custom.get((code - 0x7E00) as usize) {
            Some(n) => format!("Keycode Lofree « {n} »"),
            None => "Keycode propre au firmware Lofree".to_string(),
        },
        0x7C00 => "Passe le clavier en mode bootloader".to_string(),
        _ => String::new(),
    };
    let head = format!("{}  (0x{code:04X})", name(code));
    if what.is_empty() { head } else { format!("{head}\n{what}") }
}

/// Analyse un nom de keycode (`KC_A`, `a`, `LCTL(KC_C)`, `MO(1)`, `0x7E0B`…).
pub fn parse(input: &str, custom: &[String]) -> Option<u16> {
    let s = input.trim();
    if s.is_empty() {
        return None;
    }
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        return u16::from_str_radix(hex, 16).ok();
    }
    let up = s.to_ascii_uppercase();
    if let Some(k) = BASIC.iter().chain(SPECIAL).find(|k| {
        k.name == up || k.name.strip_prefix("KC_") == Some(up.as_str())
    }) {
        return Some(k.code);
    }
    if let Some(i) = custom.iter().position(|c| c.eq_ignore_ascii_case(s)) {
        return Some(0x7E00 + i as u16);
    }
    if let Some(n) = up.strip_prefix('M').and_then(|n| n.parse::<u16>().ok()) {
        return (n < 128).then_some(0x7700 + n);
    }
    let (func, arg) = up.split_once('(')?;
    let arg = arg.strip_suffix(')')?;
    let num = |a: &str| -> Option<u16> {
        let a = a.trim();
        match a.strip_prefix("0X") {
            Some(h) => u16::from_str_radix(h, 16).ok(),
            None => a.parse().ok(),
        }
    };
    let basic_arg = |a: &str| parse(a, custom).filter(|k| *k <= 0xFF);
    let layer_fn = |base: u16, a: &str| num(a).filter(|n| *n < 32).map(|n| base + n);
    match func {
        "TO" => layer_fn(0x5200, arg),
        "MO" => layer_fn(0x5220, arg),
        "DF" => layer_fn(0x5240, arg),
        "TG" => layer_fn(0x5260, arg),
        "OSL" => layer_fn(0x5280, arg),
        "TT" => layer_fn(0x52C0, arg),
        "PDF" => layer_fn(0x52E0, arg),
        "OSM" => num(arg).filter(|m| *m < 32).map(|m| 0x52A0 + m),
        "TD" => num(arg).filter(|n| *n < 256).map(|n| 0x5700 + n),
        "MACRO" => num(arg).filter(|n| *n < 128).map(|n| 0x7700 + n),
        "CUSTOM" => num(arg).filter(|n| *n < 64).map(|n| 0x7E00 + n),
        "USER" => num(arg).filter(|n| *n < 448).map(|n| 0x7E40 + n),
        "LT" => {
            let (l, k) = arg.split_once(',')?;
            let l = num(l).filter(|l| *l < 16)?;
            Some(0x4000 | l << 8 | basic_arg(k)?)
        }
        "LM" => {
            let (l, m) = arg.split_once(',')?;
            let l = num(l).filter(|l| *l < 16)?;
            Some(0x5000 | l << 5 | num(m).filter(|m| *m < 32)?)
        }
        "MT" => {
            let (m, k) = arg.split_once(',')?;
            let m = num(m).filter(|m| *m < 32)?;
            Some(0x2000 | m << 8 | basic_arg(k)?)
        }
        _ => {
            if let Some(f) = func.strip_suffix("_T") {
                let m = mod_bits(f)?;
                return Some(0x2000 | (m as u16) << 8 | basic_arg(arg)?);
            }
            // Modificateurs imbriqués : LCTL(LSFT(KC_A)).
            let m = mod_bits(func)?;
            let inner = parse(arg, custom)?;
            if inner > 0x1FFF {
                return None;
            }
            let inner_mods = ((inner >> 8) & 0x1F) as u8;
            if inner_mods != 0 && (inner_mods & MOD_RIGHT) != (m & MOD_RIGHT) {
                return None; // QMK ne peut pas mélanger main gauche et main droite
            }
            Some(((inner_mods | m) as u16) << 8 | (inner & 0xFF))
        }
    }
}

fn mod_bits(f: &str) -> Option<u8> {
    Some(match f {
        "LCTL" | "C" => MOD_CTRL,
        "LSFT" | "S" => MOD_SHIFT,
        "LALT" | "A" | "LOPT" => MOD_ALT,
        "LGUI" | "G" | "LCMD" | "LWIN" => MOD_GUI,
        "RCTL" => MOD_RIGHT | MOD_CTRL,
        "RSFT" => MOD_RIGHT | MOD_SHIFT,
        "RALT" | "ROPT" | "ALGR" => MOD_RIGHT | MOD_ALT,
        "RGUI" | "RCMD" | "RWIN" => MOD_RIGHT | MOD_GUI,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        let custom = vec!["TRIGGER".to_string()];
        for code in [
            0x0029u16, 0x00BE, 0x0106, 0x0306, 0x1206, 0x2104, 0x4104, 0x5221, 0x5262, 0x7700,
            0x7803, 0x7E0B, 0x7C16, 0x52C2,
        ] {
            assert_eq!(parse(&name(code), &custom), Some(code), "{}", name(code));
        }
        assert_eq!(parse("trigger", &custom), Some(0x7E00));
        assert_eq!(parse("esc", &custom), Some(0x29));
        assert_eq!(parse("C(S(KC_A))", &custom), Some(0x0304));
    }
}
