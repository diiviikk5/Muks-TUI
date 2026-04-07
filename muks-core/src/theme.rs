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
    let palette = preset_palette(&config.theme.preset);

    let mut accent = config
        .theme
        .accent_override
        .clone()
        .unwrap_or_else(|| palette.accent.clone());
    let mut accent_soft = config
        .theme
        .accent_soft_override
        .clone()
        .unwrap_or_else(|| palette.accent_soft.clone());
    let mut background = config
        .theme
        .background_override
        .clone()
        .unwrap_or_else(|| palette.background.clone());
    let mut surface = config
        .theme
        .surface_override
        .clone()
        .unwrap_or_else(|| palette.surface.clone());
    let text = config
        .theme
        .text_override
        .clone()
        .unwrap_or_else(|| palette.text.clone());

    if config.wallpaper.auto_extract_palette {
        let tint = wallpaper_tint(&config.wallpaper.current);
        accent = blend_hex(&accent, &tint, 0.35);
        accent_soft = blend_hex(&accent_soft, &tint, 0.2);
        background = blend_hex(&background, &tint, 0.08);
        surface = blend_hex(&surface, &tint, 0.12);
    }

    ThemeTokens {
        profile_name: config.profile.name.clone(),
        wallpaper_source: config.wallpaper.current.clone(),
        accent: normalize_hex_or(&accent, &palette.accent),
        accent_soft: normalize_hex_or(&accent_soft, &palette.accent_soft),
        background: normalize_hex_or(&background, &palette.background),
        surface: normalize_hex_or(&surface, &palette.surface),
        text: normalize_hex_or(&text, &palette.text),
    }
}

struct Palette {
    accent: String,
    accent_soft: String,
    background: String,
    surface: String,
    text: String,
}

fn preset_palette(preset: &str) -> Palette {
    match preset.to_ascii_lowercase().as_str() {
        "graphite" | "graphite-flare" => Palette {
            accent: "#8ab4ff".to_string(),
            accent_soft: "#6f8fd0".to_string(),
            background: "#11131a".to_string(),
            surface: "#1a1f2a".to_string(),
            text: "#f1f5ff".to_string(),
        },
        "forest" | "forest-glass" => Palette {
            accent: "#7ecf9a".to_string(),
            accent_soft: "#5fa97b".to_string(),
            background: "#101914".to_string(),
            surface: "#17251d".to_string(),
            text: "#ecfff2".to_string(),
        },
        "rose" | "rose-dusk" => Palette {
            accent: "#f1a7bd".to_string(),
            accent_soft: "#c88498".to_string(),
            background: "#1a1117".to_string(),
            surface: "#281a23".to_string(),
            text: "#ffeef4".to_string(),
        },
        "cyber" | "cyber-night" => Palette {
            accent: "#54f5ff".to_string(),
            accent_soft: "#33c6cf".to_string(),
            background: "#0b0f1a".to_string(),
            surface: "#121a2b".to_string(),
            text: "#e8f7ff".to_string(),
        },
        "nebula" | "nix-mist" => Palette {
            accent: "#9f8dff".to_string(),
            accent_soft: "#7465d4".to_string(),
            background: "#121027".to_string(),
            surface: "#1c1840".to_string(),
            text: "#f2efff".to_string(),
        },
        _ => {
            let hash = Sha256::digest(preset.as_bytes());
            Palette {
                accent: format!("#{:02x}{:02x}{:02x}", hash[0], hash[1], hash[2]),
                accent_soft: format!("#{:02x}{:02x}{:02x}", hash[3], hash[4], hash[5]),
                background: "#12141c".to_string(),
                surface: "#1b1f2a".to_string(),
                text: "#f3f6ff".to_string(),
            }
        }
    }
}

fn wallpaper_tint(source: &str) -> String {
    let hash = Sha256::digest(source.as_bytes());
    format!("#{:02x}{:02x}{:02x}", hash[0], hash[1], hash[2])
}

fn blend_hex(base: &str, tint: &str, amount: f32) -> String {
    let (br, bg, bb) = parse_hex_color(base).unwrap_or((122, 140, 190));
    let (tr, tg, tb) = parse_hex_color(tint).unwrap_or((80, 100, 170));

    let clamp = |value: f32| -> u8 { value.clamp(0.0, 255.0).round() as u8 };
    let r = clamp((br as f32) * (1.0 - amount) + (tr as f32) * amount);
    let g = clamp((bg as f32) * (1.0 - amount) + (tg as f32) * amount);
    let b = clamp((bb as f32) * (1.0 - amount) + (tb as f32) * amount);
    format!("#{:02x}{:02x}{:02x}", r, g, b)
}

fn normalize_hex_or(value: &str, fallback: &str) -> String {
    if parse_hex_color(value).is_some() {
        value.to_ascii_lowercase()
    } else {
        fallback.to_string()
    }
}

fn parse_hex_color(value: &str) -> Option<(u8, u8, u8)> {
    let color = value.strip_prefix('#')?;
    if color.len() != 6 {
        return None;
    }

    let r = u8::from_str_radix(&color[0..2], 16).ok()?;
    let g = u8::from_str_radix(&color[2..4], 16).ok()?;
    let b = u8::from_str_radix(&color[4..6], 16).ok()?;
    Some((r, g, b))
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

    #[test]
    fn honors_theme_overrides() {
        let mut config = AppConfig::default();
        config.wallpaper.auto_extract_palette = false;
        config.theme.accent_override = Some("#102030".to_string());
        config.theme.accent_soft_override = Some("#304050".to_string());
        config.theme.background_override = Some("#111111".to_string());
        config.theme.surface_override = Some("#222222".to_string());
        config.theme.text_override = Some("#ffffff".to_string());

        let tokens = derive_tokens(&config);
        assert_eq!(tokens.accent, "#102030");
        assert_eq!(tokens.accent_soft, "#304050");
        assert_eq!(tokens.background, "#111111");
        assert_eq!(tokens.surface, "#222222");
        assert_eq!(tokens.text, "#ffffff");
    }
}
