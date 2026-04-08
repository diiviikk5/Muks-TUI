use anyhow::{Context, Result};
use muks_common::AppPaths;
use serde::{Deserialize, Serialize};
use std::fs;

use crate::config::{self, AppConfig};
use crate::snapshot::{self, SnapshotManifest};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusReport {
    pub profile_name: String,
    pub preset: String,
    pub wallpaper: String,
    pub config_path: String,
    pub snapshot_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionReport {
    pub message: String,
    pub changed_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeApplyReport {
    pub accent: String,
    pub surface: String,
    pub snapshot_id: Option<String>,
}

pub struct MuksState {
    pub paths: AppPaths,
}

#[derive(Debug, Clone, Copy)]
pub struct ScenePreset {
    pub id: &'static str,
    pub name: &'static str,
    pub theme_preset: &'static str,
    pub wallpaper: &'static str,
    pub wallpaper_source_type: &'static str,
    pub workspace_name: &'static str,
    pub rainmeter_profile: &'static str,
    pub yasb_profile: &'static str,
    pub windhawk_profile: &'static str,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct PresetFile {
    profile: Option<PresetProfile>,
    wallpaper: Option<PresetWallpaper>,
    theme: Option<PresetTheme>,
    rainmeter: Option<PresetToolProfile>,
    yasb: Option<PresetToolProfile>,
    komorebi: Option<PresetToolProfile>,
    windhawk: Option<PresetToolProfile>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct PresetProfile {
    name: Option<String>,
    workspace_name: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct PresetWallpaper {
    current: Option<String>,
    source_type: Option<String>,
    fit_mode: Option<String>,
    auto_extract_palette: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct PresetTheme {
    accent_mode: Option<String>,
    reduced_motion: Option<bool>,
    accent_override: Option<String>,
    accent_soft_override: Option<String>,
    background_override: Option<String>,
    surface_override: Option<String>,
    text_override: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct PresetToolProfile {
    profile: Option<String>,
    enabled: Option<bool>,
    managed_path: Option<String>,
}

impl MuksState {
    pub fn new() -> Result<Self> {
        let paths = AppPaths::new()?;
        paths.ensure()?;
        let state = Self { paths };
        state.ensure_bootstrap_files()?;
        Ok(state)
    }

    pub fn config(&self) -> Result<AppConfig> {
        config::load_or_create(&self.paths.config_file)
    }

    pub fn save_config(&self, config: &AppConfig) -> Result<()> {
        config::save(&self.paths.config_file, config)
    }

    pub fn status_report(&self) -> Result<StatusReport> {
        let config = self.config()?;
        let snapshots = snapshot::list_snapshots(&self.paths.snapshots)?;
        Ok(StatusReport {
            profile_name: config.profile.name,
            preset: config.theme.preset,
            wallpaper: config.wallpaper.current,
            config_path: self.paths.config_file.display().to_string(),
            snapshot_count: snapshots.len(),
        })
    }

    pub fn set_wallpaper(&self, source: &str) -> Result<AppConfig> {
        let mut config = self.config()?;
        config.wallpaper.current = source.to_string();
        config.wallpaper.source_type =
            if source.starts_with("http://") || source.starts_with("https://") {
                "url".to_string()
            } else if std::path::PathBuf::from(source).exists() {
                "file".to_string()
            } else {
                "preset".to_string()
            };
        self.save_config(&config)?;
        Ok(config)
    }

    pub fn set_theme_preset(&self, preset: &str) -> Result<AppConfig> {
        let mut config = self.config()?;
        config.profile.preset = preset.to_string();
        config.theme.preset = preset.to_string();
        config.theme.accent_override = None;
        config.theme.accent_soft_override = None;
        config.theme.background_override = None;
        config.theme.surface_override = None;
        config.theme.text_override = None;
        if let Some(preset_file) = self.load_preset_file(preset)? {
            apply_preset_file(&mut config, preset_file);
        }
        self.save_config(&config)?;
        Ok(config)
    }

    pub fn set_windhawk_profile(&self, profile: &str) -> Result<AppConfig> {
        let mut config = self.config()?;
        config.windhawk.profile = profile.to_string();
        self.save_config(&config)?;
        Ok(config)
    }

    pub fn apply_scene(&self, scene_id: &str) -> Result<AppConfig> {
        let scene = scene_preset(scene_id)?;
        let mut config = self.config()?;
        config.profile.name = scene.name.to_string();
        config.profile.preset = scene.theme_preset.to_string();
        config.profile.workspace_name = scene.workspace_name.to_string();

        config.wallpaper.current = scene.wallpaper.to_string();
        config.wallpaper.source_type = scene.wallpaper_source_type.to_string();
        config.wallpaper.fit_mode = "fill".to_string();
        config.wallpaper.auto_extract_palette = true;

        config.theme.preset = scene.theme_preset.to_string();
        config.theme.accent_override = None;
        config.theme.accent_soft_override = None;
        config.theme.background_override = None;
        config.theme.surface_override = None;
        config.theme.text_override = None;

        config.rainmeter.profile = scene.rainmeter_profile.to_string();
        config.yasb.profile = scene.yasb_profile.to_string();
        config.windhawk.profile = scene.windhawk_profile.to_string();

        if let Some(preset_file) = self.load_preset_file(scene.theme_preset)? {
            apply_preset_file(&mut config, preset_file);
        }
        config.rainmeter.profile = scene.rainmeter_profile.to_string();
        config.yasb.profile = scene.yasb_profile.to_string();
        config.windhawk.profile = scene.windhawk_profile.to_string();
        config.wallpaper.current = scene.wallpaper.to_string();
        config.wallpaper.source_type = scene.wallpaper_source_type.to_string();
        config.profile.name = scene.name.to_string();
        config.profile.workspace_name = scene.workspace_name.to_string();

        self.save_config(&config)?;
        Ok(config)
    }

    pub fn configure_adapter(
        &self,
        name: &str,
        enabled: Option<bool>,
        profile: Option<String>,
        managed_path: Option<String>,
        clear_managed_path: bool,
    ) -> Result<AppConfig> {
        let mut config = self.config()?;
        let tool = adapter_config_mut(&mut config, name)?;

        if let Some(enabled) = enabled {
            tool.enabled = enabled;
        }
        if let Some(profile) = profile {
            tool.profile = profile;
        }
        if let Some(path) = managed_path {
            tool.managed_path = Some(path);
        } else if clear_managed_path {
            tool.managed_path = None;
        }

        self.save_config(&config)?;
        Ok(config)
    }

    pub fn create_backup(&self, label: &str) -> Result<SnapshotManifest> {
        snapshot::create_snapshot(
            &self.paths.snapshots,
            &self.paths.config_file,
            &self.paths.generated,
            label,
        )
    }

    pub fn restore_backup(&self, snapshot_id: &str) -> Result<SnapshotManifest> {
        snapshot::restore_snapshot(
            snapshot_id,
            &self.paths.snapshots,
            &self.paths.config_file,
            &self.paths.generated,
        )
    }

    pub fn list_snapshots(&self) -> Result<Vec<SnapshotManifest>> {
        snapshot::list_snapshots(&self.paths.snapshots)
    }

    fn ensure_bootstrap_files(&self) -> Result<()> {
        let defaults = [
            (
                "graphite.toml",
                "[profile]\nname = \"Graphite\"\nworkspace_name = \"main\"\n\n[wallpaper]\ncurrent = \"graphite\"\nsource_type = \"preset\"\nfit_mode = \"fill\"\nauto_extract_palette = true\n\n[theme]\naccent_override = \"#8ab4ff\"\naccent_soft_override = \"#6f8fd0\"\nbackground_override = \"#11131a\"\nsurface_override = \"#1a1f2a\"\ntext_override = \"#f1f5ff\"\n\n[rainmeter]\nprofile = \"aurora\"\n\n[yasb]\nprofile = \"aurora\"\n",
            ),
            (
                "forest.toml",
                "[profile]\nname = \"Forest Glass\"\nworkspace_name = \"focus\"\n\n[wallpaper]\ncurrent = \"forest\"\nsource_type = \"preset\"\nfit_mode = \"fill\"\nauto_extract_palette = true\n\n[theme]\naccent_override = \"#7ecf9a\"\naccent_soft_override = \"#5fa97b\"\nbackground_override = \"#101914\"\nsurface_override = \"#17251d\"\ntext_override = \"#ecfff2\"\n\n[rainmeter]\nprofile = \"zen\"\n\n[yasb]\nprofile = \"zen\"\n",
            ),
            (
                "rose.toml",
                "[profile]\nname = \"Rose Dusk\"\nworkspace_name = \"creative\"\n\n[wallpaper]\ncurrent = \"rose\"\nsource_type = \"preset\"\nfit_mode = \"fill\"\nauto_extract_palette = true\n\n[theme]\naccent_override = \"#f1a7bd\"\naccent_soft_override = \"#c88498\"\nbackground_override = \"#1a1117\"\nsurface_override = \"#281a23\"\ntext_override = \"#ffeef4\"\n\n[rainmeter]\nprofile = \"aurora\"\n\n[yasb]\nprofile = \"aurora\"\n",
            ),
            (
                "cyber.toml",
                "[profile]\nname = \"Cyber Night\"\nworkspace_name = \"build\"\n\n[wallpaper]\ncurrent = \"cyber\"\nsource_type = \"preset\"\nfit_mode = \"fill\"\nauto_extract_palette = true\n\n[theme]\naccent_override = \"#54f5ff\"\naccent_soft_override = \"#33c6cf\"\nbackground_override = \"#0b0f1a\"\nsurface_override = \"#121a2b\"\ntext_override = \"#e8f7ff\"\n\n[rainmeter]\nprofile = \"hyper\"\n\n[yasb]\nprofile = \"hyper\"\n",
            ),
            (
                "nebula.toml",
                "[profile]\nname = \"Nebula\"\nworkspace_name = \"main\"\n\n[wallpaper]\ncurrent = \"nebula\"\nsource_type = \"preset\"\nfit_mode = \"fill\"\nauto_extract_palette = true\n\n[theme]\naccent_override = \"#9f8dff\"\naccent_soft_override = \"#7465d4\"\nbackground_override = \"#121027\"\nsurface_override = \"#1c1840\"\ntext_override = \"#f2efff\"\n\n[rainmeter]\nprofile = \"orbit\"\n\n[yasb]\nprofile = \"orbit\"\n",
            ),
        ];

        for (file, content) in defaults {
            let path = self.paths.presets.join(file);
            fs::write(&path, content)
                .with_context(|| format!("failed to write preset {}", path.display()))?;
        }
        Ok(())
    }

    fn load_preset_file(&self, preset: &str) -> Result<Option<PresetFile>> {
        let file_name = format!("{}.toml", preset.to_ascii_lowercase());
        let path = self.paths.presets.join(file_name);
        if !path.exists() {
            return Ok(None);
        }

        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let parsed: PresetFile =
            toml::from_str(&raw).with_context(|| format!("failed to parse {}", path.display()))?;
        Ok(Some(parsed))
    }
}

fn apply_preset_file(config: &mut AppConfig, preset: PresetFile) {
    if let Some(profile) = preset.profile {
        if let Some(name) = profile.name {
            config.profile.name = name;
        }
        if let Some(workspace_name) = profile.workspace_name {
            config.profile.workspace_name = workspace_name;
        }
    }

    if let Some(wallpaper) = preset.wallpaper {
        if let Some(current) = wallpaper.current {
            config.wallpaper.current = current;
        }
        if let Some(source_type) = wallpaper.source_type {
            config.wallpaper.source_type = source_type;
        }
        if let Some(fit_mode) = wallpaper.fit_mode {
            config.wallpaper.fit_mode = fit_mode;
        }
        if let Some(auto_extract) = wallpaper.auto_extract_palette {
            config.wallpaper.auto_extract_palette = auto_extract;
        }
    }

    if let Some(theme) = preset.theme {
        if let Some(accent_mode) = theme.accent_mode {
            config.theme.accent_mode = accent_mode;
        }
        if let Some(reduced_motion) = theme.reduced_motion {
            config.theme.reduced_motion = reduced_motion;
        }
        config.theme.accent_override = theme
            .accent_override
            .or(config.theme.accent_override.take());
        config.theme.accent_soft_override = theme
            .accent_soft_override
            .or(config.theme.accent_soft_override.take());
        config.theme.background_override = theme
            .background_override
            .or(config.theme.background_override.take());
        config.theme.surface_override = theme
            .surface_override
            .or(config.theme.surface_override.take());
        config.theme.text_override = theme.text_override.or(config.theme.text_override.take());
    }

    apply_tool_preset(&mut config.rainmeter, preset.rainmeter);
    apply_tool_preset(&mut config.yasb, preset.yasb);
    apply_tool_preset(&mut config.komorebi, preset.komorebi);
    apply_tool_preset(&mut config.windhawk, preset.windhawk);
}

fn apply_tool_preset(tool: &mut crate::config::ToolConfig, preset: Option<PresetToolProfile>) {
    let Some(preset) = preset else {
        return;
    };
    if let Some(profile) = preset.profile {
        tool.profile = profile;
    }
    if let Some(enabled) = preset.enabled {
        tool.enabled = enabled;
    }
    if let Some(path) = preset.managed_path {
        tool.managed_path = Some(path);
    }
}

fn adapter_config_mut<'a>(
    config: &'a mut AppConfig,
    name: &str,
) -> Result<&'a mut crate::config::ToolConfig> {
    match name.to_ascii_lowercase().as_str() {
        "rainmeter" => Ok(&mut config.rainmeter),
        "yasb" => Ok(&mut config.yasb),
        "komorebi" => Ok(&mut config.komorebi),
        "windhawk" => Ok(&mut config.windhawk),
        "lively" => {
            anyhow::bail!("lively is controlled through [wallpaper], not adapter tool config")
        }
        _ => anyhow::bail!("unknown adapter `{}`", name),
    }
}

pub fn built_in_scenes() -> &'static [ScenePreset] {
    &[
        ScenePreset {
            id: "atelier",
            name: "Atelier Rose",
            theme_preset: "rose",
            wallpaper: "rose",
            wallpaper_source_type: "preset",
            workspace_name: "creative",
            rainmeter_profile: "aurora",
            yasb_profile: "aurora",
            windhawk_profile: "curated",
        },
        ScenePreset {
            id: "greenroom",
            name: "Greenroom Focus",
            theme_preset: "forest",
            wallpaper: "forest",
            wallpaper_source_type: "preset",
            workspace_name: "focus",
            rainmeter_profile: "zen",
            yasb_profile: "zen",
            windhawk_profile: "curated",
        },
        ScenePreset {
            id: "hyperbeam",
            name: "Hyperbeam",
            theme_preset: "cyber",
            wallpaper: "cyber",
            wallpaper_source_type: "preset",
            workspace_name: "build",
            rainmeter_profile: "hyper",
            yasb_profile: "hyper",
            windhawk_profile: "curated",
        },
        ScenePreset {
            id: "deepfield",
            name: "Deepfield Orbit",
            theme_preset: "nebula",
            wallpaper: "nebula",
            wallpaper_source_type: "preset",
            workspace_name: "main",
            rainmeter_profile: "orbit",
            yasb_profile: "orbit",
            windhawk_profile: "curated",
        },
    ]
}

fn scene_preset(scene_id: &str) -> Result<ScenePreset> {
    built_in_scenes()
        .iter()
        .find(|scene| scene.id.eq_ignore_ascii_case(scene_id))
        .copied()
        .ok_or_else(|| anyhow::anyhow!("unknown scene `{}`", scene_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_theme_overrides_are_applied() {
        let mut config = AppConfig::default();
        let preset = PresetFile {
            theme: Some(PresetTheme {
                accent_override: Some("#101010".to_string()),
                text_override: Some("#ffffff".to_string()),
                ..PresetTheme::default()
            }),
            ..PresetFile::default()
        };

        apply_preset_file(&mut config, preset);
        assert_eq!(config.theme.accent_override.as_deref(), Some("#101010"));
        assert_eq!(config.theme.text_override.as_deref(), Some("#ffffff"));
    }

    #[test]
    fn preset_tool_profile_updates_tool_settings() {
        let mut config = AppConfig::default();
        let preset = PresetFile {
            komorebi: Some(PresetToolProfile {
                profile: Some("work".to_string()),
                enabled: Some(false),
                managed_path: Some("C:/custom/komorebi.json".to_string()),
            }),
            ..PresetFile::default()
        };

        apply_preset_file(&mut config, preset);
        assert_eq!(config.komorebi.profile, "work");
        assert!(!config.komorebi.enabled);
        assert_eq!(
            config.komorebi.managed_path.as_deref(),
            Some("C:/custom/komorebi.json")
        );
    }
}
