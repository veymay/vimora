//! Ready-made block categories ("O'yinlar", "AI chatbotlar", …) for sites and programs, plus the
//! teacher's imported lists (each imported file becomes a category of its own).
//!
//! A category blocked as a whole blocks every item in it (also items a later version adds);
//! otherwise only the items picked inside it are blocked.

use serde::{Deserialize, Serialize};
use vimora_ui::icons;

/// What the config keeps for one category.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct BlockCategory {
    /// Built-in key ("games", …) or "import:<n>" for an imported list.
    pub id: String,
    /// Imported lists only: shown name (the file name).
    pub name: String,
    /// Imported lists only: the items.
    pub items: Vec<String>,
    /// The whole category is blocked.
    pub all: bool,
    /// Items blocked one by one (when `all` is off).
    pub picked: Vec<String>,
}

impl BlockCategory {
    pub fn imported(&self) -> bool {
        self.id.starts_with("import:")
    }

    pub fn is_on(&self, item: &str) -> bool {
        self.all || self.picked.iter().any(|p| p == item)
    }

    /// Items this category blocks.
    pub fn blocked(&self) -> Vec<String> {
        self.items
            .iter()
            .filter(|i| self.is_on(i))
            .cloned()
            .collect()
    }

    /// Switches one item; keeps `all` in step with the items.
    pub fn set_item(&mut self, item: &str, on: bool) {
        if self.all {
            self.all = false;
            self.picked = self.items.clone();
        }
        self.picked.retain(|p| p != item);
        if on {
            self.picked.push(item.to_string());
        }
        if !self.items.is_empty() && self.items.iter().all(|i| self.picked.contains(i)) {
            self.all = true;
            self.picked.clear();
        }
    }

    pub fn set_all(&mut self, on: bool) {
        self.all = on;
        self.picked.clear();
    }
}

pub struct Def {
    pub id: &'static str,
    /// Uzbek name (the `tr` key).
    pub name: &'static str,
    pub icon: &'static str,
    pub items: &'static [&'static str],
}

pub const SITES: &[Def] = &[
    Def {
        id: "social",
        name: "Ijtimoiy tarmoqlar",
        icon: icons::USERS_ROUND,
        items: &[
            "instagram.com",
            "tiktok.com",
            "facebook.com",
            "vk.com",
            "ok.ru",
            "x.com",
            "twitter.com",
            "threads.net",
            "snapchat.com",
            "pinterest.com",
            "reddit.com",
            "likee.video",
        ],
    },
    Def {
        id: "messengers",
        name: "Messenjerlar",
        icon: icons::MESSAGES,
        items: &[
            "web.telegram.org",
            "telegram.org",
            "web.whatsapp.com",
            "discord.com",
            "messenger.com",
            "viber.com",
        ],
    },
    Def {
        id: "video",
        name: "Video va kino",
        icon: icons::CLAPPERBOARD,
        items: &[
            "youtube.com",
            "twitch.tv",
            "kick.com",
            "rutube.ru",
            "kinopoisk.ru",
            "netflix.com",
            "ivi.ru",
            "okko.tv",
            "vimeo.com",
            "dailymotion.com",
        ],
    },
    Def {
        id: "games",
        name: "O'yinlar",
        icon: icons::GAMEPAD,
        items: &[
            "roblox.com",
            "steampowered.com",
            "steamcommunity.com",
            "epicgames.com",
            "minecraft.net",
            "battle.net",
            "ea.com",
            "poki.com",
            "crazygames.com",
            "miniclip.com",
            "y8.com",
            "friv.com",
            "agar.io",
        ],
    },
    Def {
        id: "ai",
        name: "AI chatbotlar",
        icon: icons::BOT,
        items: &[
            "chatgpt.com",
            "openai.com",
            "claude.ai",
            "gemini.google.com",
            "copilot.microsoft.com",
            "perplexity.ai",
            "deepseek.com",
            "grok.com",
            "meta.ai",
            "chat.mistral.ai",
            "character.ai",
            "poe.com",
            "you.com",
        ],
    },
    Def {
        id: "gambling",
        name: "Qimor va bukmekerlar",
        icon: icons::DICES,
        items: &[
            "1xbet.com",
            "melbet.com",
            "mostbet.com",
            "linebet.com",
            "betwinner.com",
            "22bet.com",
            "pin-up.com",
            "parimatch.com",
            "fonbet.ru",
            "bet365.com",
            "stake.com",
            "pokerstars.com",
            "888casino.com",
        ],
    },
    Def {
        id: "adult",
        name: "Kattalar uchun (18+)",
        icon: icons::EYE_OFF,
        items: &[
            "pornhub.com",
            "xvideos.com",
            "xnxx.com",
            "xhamster.com",
            "redtube.com",
            "youporn.com",
            "onlyfans.com",
            "chaturbate.com",
        ],
    },
    Def {
        id: "torrent",
        name: "Torrent va pirat saytlar",
        icon: icons::DOWNLOAD_DRIVE,
        items: &[
            "thepiratebay.org",
            "rutracker.org",
            "rutor.info",
            "1337x.to",
            "nnmclub.to",
            "kinozal.tv",
            "yts.mx",
        ],
    },
    Def {
        id: "vpn",
        name: "VPN va proksi",
        icon: icons::MASK,
        items: &[
            "nordvpn.com",
            "expressvpn.com",
            "protonvpn.com",
            "surfshark.com",
            "windscribe.com",
            "psiphon.ca",
            "hide.me",
            "hidemy.name",
            "torproject.org",
            "croxyproxy.com",
            "proxysite.com",
        ],
    },
    Def {
        id: "music",
        name: "Musiqa",
        icon: icons::MUSIC,
        items: &[
            "spotify.com",
            "music.yandex.ru",
            "music.youtube.com",
            "soundcloud.com",
            "deezer.com",
        ],
    },
    Def {
        id: "shops",
        name: "Onlayn do'konlar",
        icon: icons::CART,
        items: &[
            "uzum.uz",
            "olx.uz",
            "aliexpress.com",
            "temu.com",
            "wildberries.ru",
            "ozon.ru",
            "amazon.com",
        ],
    },
];

pub const APPS: &[Def] = &[
    Def {
        id: "games",
        name: "O'yinlar",
        icon: icons::GAMEPAD,
        items: &[
            "steam",
            "epicgameslauncher",
            "robloxplayerbeta",
            "minecraft",
            "javaw",
            "battle.net",
            "riotclientservices",
            "leagueclient",
            "eadesktop",
            "cs2",
            "dota2",
            "gta5",
        ],
    },
    Def {
        id: "messengers",
        name: "Messenjerlar",
        icon: icons::MESSAGES,
        items: &[
            "telegram", "discord", "whatsapp", "viber", "skype", "signal",
        ],
    },
    Def {
        id: "ai",
        name: "AI ilovalar",
        icon: icons::BOT,
        items: &["chatgpt", "claude", "perplexity"],
    },
    Def {
        id: "torrent",
        name: "Torrent dasturlari",
        icon: icons::DOWNLOAD_DRIVE,
        items: &[
            "qbittorrent",
            "utorrent",
            "bittorrent",
            "transmission-qt",
            "transmission-gtk",
            "deluge",
            "mediaget",
        ],
    },
    Def {
        id: "vpn",
        name: "VPN va proksi",
        icon: icons::MASK,
        items: &[
            "nordvpn",
            "expressvpn",
            "protonvpn",
            "windscribe",
            "psiphon3",
            "openvpn-gui",
            "hotspotshield",
            "v2rayn",
            "outline",
        ],
    },
    Def {
        id: "browsers",
        name: "Boshqa brauzerlar",
        icon: icons::APP_WINDOW,
        items: &["opera", "brave", "vivaldi", "tor"],
    },
];

/// A category as the drawer shows it: config state with its items filled in.
pub fn merged(defs: &[Def], saved: &[BlockCategory]) -> Vec<BlockCategory> {
    let mut out: Vec<BlockCategory> = defs
        .iter()
        .map(|d| {
            let s = saved.iter().find(|c| c.id == d.id);
            BlockCategory {
                id: d.id.into(),
                name: String::new(),
                items: d.items.iter().map(|s| s.to_string()).collect(),
                all: s.is_some_and(|c| c.all),
                picked: s.map(|c| c.picked.clone()).unwrap_or_default(),
            }
        })
        .collect();
    out.extend(saved.iter().filter(|c| c.imported()).cloned());
    out
}

/// What goes back into the config: built-in categories keep only their state.
pub fn to_config(cats: &[BlockCategory]) -> Vec<BlockCategory> {
    cats.iter()
        .filter(|c| c.imported() || c.all || !c.picked.is_empty())
        .map(|c| {
            let mut c = c.clone();
            if !c.imported() {
                c.items.clear();
                c.name.clear();
            }
            c
        })
        .collect()
}

/// Everything the categories block (no duplicates).
pub fn blocked(cats: &[BlockCategory]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for c in cats {
        for i in c.blocked() {
            if !out.iter().any(|o| o.eq_ignore_ascii_case(&i)) {
                out.push(i);
            }
        }
    }
    out
}

/// Built-in definition of a category (none for imported lists).
pub fn def<'a>(defs: &'a [Def], id: &str) -> Option<&'a Def> {
    defs.iter().find(|d| d.id == id)
}

/// A new imported category from a file's text: one item per line / comma.
pub fn import(cats: &[BlockCategory], name: &str, text: &str) -> Option<BlockCategory> {
    let mut items: Vec<String> = Vec::new();
    for raw in crate::config::split_list(text) {
        let t = raw.trim_start_matches('#').trim().to_string();
        if t.is_empty() || t.len() > 200 {
            continue;
        }
        if !items.iter().any(|x| x.eq_ignore_ascii_case(&t)) {
            items.push(t);
        }
    }
    if items.is_empty() {
        return None;
    }
    let n = (0..)
        .find(|n| !cats.iter().any(|c| c.id == format!("import:{n}")))
        .unwrap_or(0);
    Some(BlockCategory {
        id: format!("import:{n}"),
        name: name.to_string(),
        items,
        all: true,
        picked: vec![],
    })
}

/// Merges the manual list (lines, '#' = off) with what the categories block.
pub fn effective(manual: &str, cats: &[BlockCategory]) -> Vec<String> {
    let mut out = crate::config::active_list(manual);
    for i in blocked(cats) {
        if !out.iter().any(|o| o.eq_ignore_ascii_case(&i)) {
            out.push(i);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_and_single_items() {
        let mut cats = merged(SITES, &[]);
        let ai = cats.iter_mut().find(|c| c.id == "ai").unwrap();
        ai.set_all(true);
        assert_eq!(ai.blocked().len(), ai.items.len());
        ai.set_item("claude.ai", false);
        assert!(!ai.all);
        assert_eq!(ai.blocked().len(), ai.items.len() - 1);
        ai.set_item("claude.ai", true);
        assert!(ai.all);
        let saved = to_config(&cats);
        assert_eq!(saved.len(), 1);
        assert!(saved[0].items.is_empty());
        let back = merged(SITES, &saved);
        assert!(back.iter().find(|c| c.id == "ai").unwrap().all);
    }

    #[test]
    fn imports_are_kept() {
        let cats = merged(SITES, &[]);
        let c = import(&cats, "respublika.txt", "a.uz\nb.uz, a.uz\n#c.uz").unwrap();
        assert_eq!(c.items, vec!["a.uz", "b.uz", "c.uz"]);
        let saved = to_config(&[c]);
        let back = merged(SITES, &saved);
        assert_eq!(back.last().unwrap().name, "respublika.txt");
        assert_eq!(
            effective("x.uz\n#y.uz", &back),
            vec!["x.uz", "a.uz", "b.uz", "c.uz"]
        );
    }

    #[test]
    fn unique_ids() {
        for defs in [SITES, APPS] {
            for (i, d) in defs.iter().enumerate() {
                assert!(defs[..i].iter().all(|o| o.id != d.id));
            }
        }
    }
}
