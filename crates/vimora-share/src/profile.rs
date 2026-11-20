//! Receiver profiles: what is known about a kind of display, as data (not as `if`s in the code).
//!
//! A profile says how sure it is (`confidence`) and whether it was tested (`tested`); nothing is
//! ever marked verified without a date. Unknown receivers get the generic profile of their
//! protocol, which is conservative.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    Miracast,
    Cast,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    /// A guess to start from.
    Assumed,
    /// Seen on a real device.
    Observed,
    /// Tested end to end on real devices (needs a date).
    Verified,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tested {
    /// "untested", "partial" or "verified".
    pub status: String,
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default)]
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub vendor: String,
    /// Substrings of the model name this profile applies to (empty = generic for the protocol).
    #[serde(default)]
    pub model_contains: Vec<String>,
    pub protocol: Protocol,
    pub codecs: Vec<String>,
    pub max_width: u32,
    pub max_height: u32,
    pub max_fps: u32,
    /// Known behaviours and workarounds, by name.
    #[serde(default)]
    pub quirks: Vec<String>,
    pub confidence: Confidence,
    pub tested: Tested,
}

#[derive(Debug, Clone)]
pub struct ProfileSet {
    profiles: Vec<Profile>,
}

const BUILTIN: &str = include_str!("../profiles.json");

impl ProfileSet {
    pub fn builtin() -> Self {
        Self::from_json(BUILTIN).expect("built-in profiles are valid")
    }

    pub fn from_json(text: &str) -> anyhow::Result<Self> {
        let profiles: Vec<Profile> = serde_json::from_str(text)?;
        for p in &profiles {
            if p.confidence == Confidence::Verified && p.tested.date.is_none() {
                anyhow::bail!("profile {} is verified without a test date", p.id);
            }
        }
        Ok(Self { profiles })
    }

    /// The best profile for a receiver: a specific one (model match) else the protocol's generic.
    pub fn find(&self, protocol: Protocol, model: &str) -> &Profile {
        let model = model.to_lowercase();
        self.profiles
            .iter()
            .filter(|p| p.protocol == protocol && !p.model_contains.is_empty())
            .find(|p| {
                p.model_contains
                    .iter()
                    .any(|m| model.contains(&m.to_lowercase()))
            })
            .or_else(|| {
                self.profiles
                    .iter()
                    .find(|p| p.protocol == protocol && p.model_contains.is_empty())
            })
            .expect("a generic profile exists for every protocol")
    }

    pub fn all(&self) -> &[Profile] {
        &self.profiles
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_real_chromecast_ultra_gets_its_observed_profile() {
        let set = ProfileSet::builtin();
        let p = set.find(Protocol::Cast, "Chromecast Ultra");
        assert_eq!(p.id, "google-chromecast-ultra-fw1.36");
        assert_eq!(p.confidence, Confidence::Observed);
        assert!(p.quirks.iter().any(|q| q == "x509_v1_certificate"));
        assert!(p
            .quirks
            .iter()
            .any(|q| q == "ignores_default_media_receiver"));
    }

    #[test]
    fn unknown_receivers_get_the_conservative_generic_profile() {
        let set = ProfileSet::builtin();
        for proto in [Protocol::Cast, Protocol::Miracast] {
            let p = set.find(proto, "Some Unheard-of Board 9000");
            assert!(p.model_contains.is_empty());
            assert_eq!(p.confidence, Confidence::Assumed);
            assert_eq!(p.tested.status, "untested");
            assert!(p.max_height <= 1080 && p.max_fps <= 30);
        }
    }

    #[test]
    fn nothing_is_verified_without_a_date() {
        let bad = r#"[{"id":"x","vendor":"v","protocol":"cast","codecs":["h264"],
            "max_width":1280,"max_height":720,"max_fps":30,"confidence":"verified",
            "tested":{"status":"verified"}}]"#;
        assert!(ProfileSet::from_json(bad).is_err());
        // And none of the built-in ones claim it.
        assert!(ProfileSet::builtin()
            .all()
            .iter()
            .all(|p| p.confidence != Confidence::Verified));
    }

    #[test]
    fn matching_ignores_case() {
        let set = ProfileSet::builtin();
        assert_eq!(
            set.find(Protocol::Cast, "CHROMECAST ULTRA").id,
            "google-chromecast-ultra-fw1.36"
        );
    }
}
