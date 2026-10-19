//! Macros VIA : conversion entre le tampon du clavier et une syntaxe texte.
//!
//! Syntaxe (celle de l'éditeur de VIA) :
//! - texte ASCII tapé tel quel ;
//! - `{KC_A}` tape une touche, `{KC_LCTL,KC_C}` un raccourci (appui dans l'ordre,
//!   relâche dans l'ordre inverse) ;
//! - `{+KC_LSFT}` appuie, `{-KC_LSFT}` relâche ;
//! - `{250}` attend 250 ms ;
//! - `\{` et `\\` donnent une accolade et une barre oblique inverse.

use anyhow::{bail, Context, Result};

use crate::keycodes;

const PREFIX: u8 = 0x01;
const TAP: u8 = 0x01;
const DOWN: u8 = 0x02;
const UP: u8 = 0x03;
const DELAY: u8 = 0x04;
const DELAY_END: u8 = b'|';
const MAX_DELAY_MS: u32 = 9999; // le firmware lit au plus 4 chiffres

#[derive(Clone, Copy, PartialEq, Debug)]
enum Action {
    Char(u8),
    Tap(u8),
    Down(u8),
    Up(u8),
    Delay(u32),
}

/// Découpe le tampon en `count` macros, converties en texte.
pub fn decode_buffer(buf: &[u8], count: usize) -> Vec<String> {
    let mut out = Vec::with_capacity(count);
    let mut rest = buf;
    for _ in 0..count {
        let (actions, used) = parse_bytes(rest);
        out.push(to_text(&actions));
        rest = rest.get(used..).unwrap_or(&[]);
    }
    out
}

/// Encode toutes les macros, chacune terminée par un octet nul.
pub fn encode_buffer(texts: &[String]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    for (i, text) in texts.iter().enumerate() {
        out.extend(encode(text).with_context(|| format!("macro M{i}"))?);
        out.push(0);
    }
    Ok(out)
}

/// Lit une macro jusqu'à son octet nul ; renvoie aussi le nombre d'octets consommés.
fn parse_bytes(buf: &[u8]) -> (Vec<Action>, usize) {
    let mut actions = Vec::new();
    let mut i = 0;
    while i < buf.len() {
        let b = buf[i];
        i += 1;
        match b {
            0 => return (actions, i),
            PREFIX => {
                let Some(&kind) = buf.get(i) else { break };
                i += 1;
                match kind {
                    TAP | DOWN | UP => {
                        let Some(&kc) = buf.get(i) else { break };
                        i += 1;
                        actions.push(match kind {
                            TAP => Action::Tap(kc),
                            DOWN => Action::Down(kc),
                            _ => Action::Up(kc),
                        });
                    }
                    DELAY => {
                        let mut ms = 0u32;
                        while let Some(&d) = buf.get(i) {
                            i += 1;
                            if d == DELAY_END || !d.is_ascii_digit() {
                                break;
                            }
                            ms = ms * 10 + (d - b'0') as u32;
                        }
                        actions.push(Action::Delay(ms));
                    }
                    _ => {} // extension inconnue : ignorée
                }
            }
            c => actions.push(Action::Char(c)),
        }
    }
    (actions, buf.len())
}

fn kc_name(kc: u8) -> String {
    keycodes::name(kc as u16)
}

fn to_text(actions: &[Action]) -> String {
    let mut s = String::new();
    let mut i = 0;
    while i < actions.len() {
        match actions[i] {
            Action::Char(c) => match c {
                b'{' => s.push_str("\\{"),
                b'\\' => s.push_str("\\\\"),
                b'\n' | b'\t' | 0x20..=0x7E => s.push(c as char),
                _ => s.push_str(&format!("\\x{c:02X}")),
            },
            Action::Tap(kc) => s.push_str(&format!("{{{}}}", kc_name(kc))),
            Action::Delay(ms) => s.push_str(&format!("{{{ms}}}")),
            Action::Down(_) => {
                // Raccourci : appuis successifs suivis des relâchements en ordre inverse.
                let downs: Vec<u8> = actions[i..]
                    .iter()
                    .map_while(|a| match a {
                        Action::Down(k) => Some(*k),
                        _ => None,
                    })
                    .collect();
                let n = downs.len();
                let ups_match = (0..n).all(|j| actions.get(i + n + j) == Some(&Action::Up(downs[n - 1 - j])));
                if ups_match {
                    let names: Vec<String> = downs.iter().map(|k| kc_name(*k)).collect();
                    s.push_str(&format!("{{{}}}", names.join(",")));
                    i += 2 * n;
                    continue;
                }
                if let Action::Down(k) = actions[i] {
                    s.push_str(&format!("{{+{}}}", kc_name(k)));
                }
            }
            Action::Up(k) => s.push_str(&format!("{{-{}}}", kc_name(k))),
        }
        i += 1;
    }
    s
}

fn macro_key(name: &str) -> Result<u8> {
    match keycodes::parse(name, &[]) {
        Some(k @ 0x04..=0xFF) => Ok(k as u8),
        Some(_) => bail!("« {name} » n'est pas une touche simple : impossible dans une macro"),
        None => bail!("touche inconnue « {name} »"),
    }
}

/// Encode une macro (sans l'octet nul final).
pub fn encode(text: &str) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.peek() {
                Some('{') | Some('\\') => out.push(chars.next().unwrap() as u8),
                Some('x') => {
                    chars.next();
                    let hex: String = chars.by_ref().take(2).collect();
                    let b = u8::from_str_radix(&hex, 16)
                        .with_context(|| format!("séquence \\x{hex} invalide"))?;
                    if b == 0 || b == PREFIX {
                        bail!("octet \\x{hex} réservé");
                    }
                    out.push(b);
                }
                _ => out.push(b'\\'),
            },
            '{' => {
                let mut inner = String::new();
                loop {
                    match chars.next() {
                        Some('}') => break,
                        Some(ch) => inner.push(ch),
                        None => bail!("accolade « {{ » non fermée"),
                    }
                }
                let inner = inner.trim();
                if inner.is_empty() {
                    bail!("« {{}} » vide");
                }
                if inner.chars().all(|ch| ch.is_ascii_digit()) {
                    let ms: u32 = inner.parse()?;
                    if ms > MAX_DELAY_MS {
                        bail!("délai {ms} ms trop long (maximum {MAX_DELAY_MS})");
                    }
                    out.extend([PREFIX, DELAY]);
                    out.extend(ms.to_string().bytes());
                    out.push(DELAY_END);
                } else if let Some(k) = inner.strip_prefix('+') {
                    out.extend([PREFIX, DOWN, macro_key(k)?]);
                } else if let Some(k) = inner.strip_prefix('-') {
                    out.extend([PREFIX, UP, macro_key(k)?]);
                } else {
                    let keys = inner.split(',').map(macro_key).collect::<Result<Vec<_>>>()?;
                    if let [k] = keys[..] {
                        out.extend([PREFIX, TAP, k]);
                    } else {
                        for k in &keys {
                            out.extend([PREFIX, DOWN, *k]);
                        }
                        for k in keys.iter().rev() {
                            out.extend([PREFIX, UP, *k]);
                        }
                    }
                }
            }
            '\n' | '\t' | ' '..='~' => out.push(c as u8),
            '\r' => {}
            _ => bail!(
                "caractère « {c} » non ASCII : une macro tape les touches d'un clavier US, \
                 utilisez {{KC_…}} pour les autres caractères"
            ),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let texts = vec![
            "Bonjour {KC_ENT}".to_string(),
            "{KC_LCTL,KC_LSFT,KC_ESC}{250}{+KC_LSFT}ab{-KC_LSFT}".to_string(),
            "a\\{b\\\\c".to_string(),
            String::new(),
        ];
        let buf = encode_buffer(&texts).unwrap();
        assert_eq!(decode_buffer(&buf, texts.len()), texts);
    }

    #[test]
    fn rejects_non_ascii() {
        assert!(encode("été").is_err());
        assert!(encode("{MO(1)}").is_err());
        assert!(encode("{10000}").is_err());
    }
}
