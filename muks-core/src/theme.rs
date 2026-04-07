use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::config::AppConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeTokens {
    pub profile_name: String,
    pub wallpaper_source: String,
    pub accent: String,
    pub accent_soft: String,
    pub background: String,
    pub surface: String,
    pub text: String,
}

pub fn derive_tokens(config: &AppConfig) -> ThemeTokens {
    let seed = format!(
        "{}:{}:{}",
        config.profile.preset, config.profile.name, config.wallpaper.current
    );
    let hash = Sha256::digest(seed.as_bytes());

    ThemeTokens {
        profile_name: config.profile.name.clone(),
        wallpaper_source: config.wallpaper.current.clone(),
        accent: format!("#{:02x}{:02x}{:02x}", hash[0], hash[1], hash[2]),
        accent_soft: format!("#{:02x}{:02x}{:02x}", hash[3], hash[4], hash[5]),
        background: format!("#{:02x}{:02x}{:02x}", hash[6], hash[7], hash[8]),
        surface: format!("#{:02x}{:02x}{:02x}", hash[9], hash[10], hash[11]),
        text: "#f5f7fb".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;

    #[test]
    fn derives_stable_tokens_for_same_input() {
        let config = AppConfig::default();
        let first = derive_tokens(&config);
        let second = derive_tokens(&config);
        assert_eq!(first.accent, second.accent);
        assert_eq!(first.surface, second.surface);
    }
}
