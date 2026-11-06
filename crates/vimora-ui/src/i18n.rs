//! Interface language: Uzbek (source text), English and Russian.
//!
//! Source strings are written in Uzbek and double as lookup keys (gettext style). Each app installs
//! one table of `(uz, en, ru)` rows at start-up; [`tr`] returns the text for the current language
//! and falls back to the Uzbek source when a row is missing, so a forgotten string never breaks.
//!
//! When one Uzbek word means two things in other languages, prefix a context: `tr("unlock|Ochish")`
//! shows "Ochish" in Uzbek and has its own row ("Unlock") next to plain "Ochish" ("Open").
//!
//! Strings with values use named placeholders, filled by [`tf!`](crate::tf):
//! `tf!("{n} ta kompyuter", n = count)`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Lang {
    #[default]
    #[serde(rename = "uz")]
    Uz,
    #[serde(rename = "en")]
    En,
    #[serde(rename = "ru")]
    Ru,
}

impl Lang {
    pub const ALL: [Lang; 3] = [Lang::Uz, Lang::En, Lang::Ru];

    /// The language's own name (shown in the picker regardless of the current language).
    pub fn native_name(self) -> &'static str {
        match self {
            Lang::Uz => "O'zbekcha",
            Lang::En => "English",
            Lang::Ru => "Русский",
        }
    }

    /// Best guess from the OS locale (`LANG`, `LC_ALL`, …); Uzbek otherwise.
    pub fn detect() -> Lang {
        let env = ["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"]
            .iter()
            .find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty()))
            .unwrap_or_default()
            .to_lowercase();
        if env.starts_with("ru") {
            Lang::Ru
        } else if env.starts_with("en") {
            Lang::En
        } else {
            Lang::Uz
        }
    }
}

/// One translation row: Uzbek source, English, Russian.
pub type Row = (&'static str, &'static str, &'static str);

static LANG: AtomicU8 = AtomicU8::new(0);
static TABLE: OnceLock<HashMap<&'static str, (&'static str, &'static str)>> = OnceLock::new();

/// Installs the app's translation table (once, at start-up).
pub fn install(rows: &'static [Row]) {
    let _ = TABLE.set(rows.iter().map(|(uz, en, ru)| (*uz, (*en, *ru))).collect());
}

pub fn set_lang(l: Lang) {
    LANG.store(l as u8, Ordering::Relaxed);
}

pub fn lang() -> Lang {
    match LANG.load(Ordering::Relaxed) {
        1 => Lang::En,
        2 => Lang::Ru,
        _ => Lang::Uz,
    }
}

/// The text of `uz` in the current language.
pub fn tr(uz: &'static str) -> &'static str {
    let l = lang();
    if l == Lang::Uz {
        return strip_context(uz);
    }
    match TABLE.get().and_then(|t| t.get(uz)) {
        Some((en, ru)) => match l {
            Lang::En => en,
            _ => ru,
        },
        None => {
            missing(uz);
            strip_context(uz)
        }
    }
}

/// "unlock|Ochish" → "Ochish" (a lowercase ASCII word before '|' is a disambiguating context).
fn strip_context(s: &'static str) -> &'static str {
    match s.split_once('|') {
        Some((ctx, text)) if !ctx.is_empty() && ctx.bytes().all(|b| b.is_ascii_lowercase()) => text,
        _ => s,
    }
}

/// Debug aid: `VIMORA_I18N_MISSING=1` prints each untranslated string once.
fn missing(uz: &'static str) {
    use std::sync::Mutex;
    static SEEN: OnceLock<Mutex<std::collections::HashSet<&'static str>>> = OnceLock::new();
    if std::env::var_os("VIMORA_I18N_MISSING").is_none() {
        return;
    }
    let seen = SEEN.get_or_init(Default::default);
    if seen.lock().unwrap().insert(uz) {
        eprintln!("i18n: missing {uz:?}");
    }
}

/// Translated text with named placeholders: `tf!("{n} ta kompyuter", n = count)`.
#[macro_export]
macro_rules! tf {
    ($s:literal $(, $k:ident = $v:expr)* $(,)?) => {{
        #[allow(unused_mut)]
        let mut out = $crate::i18n::tr($s).to_string();
        $( out = out.replace(concat!("{", stringify!($k), "}"), &($v).to_string()); )*
        out
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    static ROWS: &[Row] = &[
        ("{n} ta kompyuter", "{n} computers", "{n} компьютеров"),
        ("unlock|Ochish", "Unlock", "Разблокировать"),
    ];

    #[test]
    fn lookup_and_fallback() {
        install(ROWS);
        set_lang(Lang::En);
        assert_eq!(crate::tf!("{n} ta kompyuter", n = 3), "3 computers");
        assert_eq!(tr("yo'q"), "yo'q");
        assert_eq!(tr("unlock|Ochish"), "Unlock");
        set_lang(Lang::Uz);
        assert_eq!(tr("unlock|Ochish"), "Ochish");
        assert_eq!(crate::tf!("{n} ta kompyuter", n = 3), "3 ta kompyuter");
    }
}
