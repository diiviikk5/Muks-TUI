use anyhow::{Context, Result, anyhow};
use muks_common::{
    AdapterCapability, AdapterHealth, AdapterMetadata, AdapterReloadMode, AdapterSafetyLevel,
    AdapterStatus, AppPaths, ToolName, command_exists, read_command_output,
};
use muks_core::{AppConfig, theme::ThemeTokens};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct ApplyRequest {
    pub config: AppConfig,
    pub paths: AppPaths,
    pub tokens: ThemeTokens,
    pub best_effort: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedArtifact {
    pub adapter: String,
    pub files: Vec<String>,
    pub live_targets: Vec<String>,
    pub notes: Vec<String>,
}

pub trait Adapter: Send + Sync {
    fn tool(&self) -> ToolName;
    fn metadata(&self, paths: &AppPaths) -> AdapterMetadata;
    fn detect(&self, paths: &AppPaths) -> AdapterStatus;
    fn install(&self, paths: &AppPaths) -> Result<String>;
    fn render(&self, request: &ApplyRequest) -> Result<GeneratedArtifact>;
    fn apply(&self, request: &ApplyRequest) -> Result<GeneratedArtifact>;
    fn backup(&self, request: &ApplyRequest) -> Result<()>;
    fn rollback(&self, request: &ApplyRequest) -> Result<()>;
    fn doctor(&self, paths: &AppPaths) -> Result<String>;
}

pub struct AdapterRegistry {
    adapters: Vec<Box<dyn Adapter>>,
}

impl AdapterRegistry {
    pub fn new() -> Self {
        Self {
            adapters: vec![
                Box::new(LivelyAdapter),
                Box::new(RainmeterAdapter),
                Box::new(YasbAdapter),
                Box::new(KomorebiAdapter),
                Box::new(WindhawkAdapter),
            ],
        }
    }

    pub fn detect_all(&self, paths: &AppPaths) -> Result<Vec<AdapterStatus>> {
        Ok(self
            .adapters
            .iter()
            .map(|adapter| adapter.detect(paths))
            .collect())
    }

    pub fn apply_all(&self, request: &ApplyRequest) -> Result<Vec<GeneratedArtifact>> {
        let mut generated = Vec::new();
        let mut applied_indexes = Vec::new();
        for (index, adapter) in self.adapters.iter().enumerate() {
            if !adapter_enabled(adapter.tool(), &request.config) {
                continue;
            }

            if let Err(error) = adapter.backup(request) {
                if request.best_effort {
                    tracing::warn!(
                        "best effort backup failed for {}: {}",
                        adapter.tool(),
                        error
                    );
                    continue;
                }
                rollback_applied(&self.adapters, &applied_indexes, request);
                return Err(error).context(format!("backup failed for {}", adapter.tool()));
            }

            match adapter.apply(request) {
                Ok(artifact) => {
                    generated.push(artifact);
                    applied_indexes.push(index);
                }
                Err(error) if request.best_effort => {
                    tracing::warn!("best effort apply failed for {}: {}", adapter.tool(), error);
                }
                Err(error) => {
                    rollback_applied(&self.adapters, &applied_indexes, request);
                    return Err(error).context(format!("apply failed for {}", adapter.tool()));
                }
            }
        }
        Ok(generated)
    }

    pub fn render_all(&self, request: &ApplyRequest) -> Result<Vec<GeneratedArtifact>> {
        self.adapters
            .iter()
            .filter(|adapter| adapter_enabled(adapter.tool(), &request.config))
            .map(|adapter| adapter.render(request))
            .collect()
    }

    pub fn apply_one(&self, tool: &str, request: &ApplyRequest) -> Result<GeneratedArtifact> {
        let adapter = self
            .find(tool)
            .ok_or_else(|| anyhow!("adapter `{}` not found", tool))?;

        if !adapter_enabled(adapter.tool(), &request.config) {
            return Err(anyhow!("adapter `{}` is disabled in config", tool));
        }

        if let Err(error) = adapter.backup(request) {
            if request.best_effort {
                tracing::warn!(
                    "best effort backup failed for {}: {}",
                    adapter.tool(),
                    error
                );
            } else {
                return Err(error).context(format!("backup failed for {}", adapter.tool()));
            }
        }

        match adapter.apply(request) {
            Ok(artifact) => Ok(artifact),
            Err(error) if request.best_effort => Err(error).context(format!(
                "apply failed for {} (best-effort mode still returned error)",
                adapter.tool()
            )),
            Err(error) => {
                let _ = adapter.rollback(request);
                Err(error).context(format!("apply failed for {}", adapter.tool()))
            }
        }
    }

    pub fn list_metadata(&self, paths: &AppPaths) -> Vec<AdapterMetadata> {
        self.adapters
            .iter()
            .map(|adapter| adapter.metadata(paths))
            .collect()
    }

    pub fn adapter_status(&self, tool: &str, paths: &AppPaths) -> Result<AdapterStatus> {
        let adapter = self
            .find(tool)
            .ok_or_else(|| anyhow!("adapter `{}` not found", tool))?;
        Ok(adapter.detect(paths))
    }

    pub fn reinstall(&self, tool: &str, paths: &AppPaths) -> Result<String> {
        let adapter = self
            .find(tool)
            .ok_or_else(|| anyhow!("adapter `{}` not found", tool))?;
        adapter.install(paths)
    }

    pub fn doctor(&self, tool: &str, paths: &AppPaths) -> Result<String> {
        let adapter = self
            .find(tool)
            .ok_or_else(|| anyhow!("adapter `{}` not found", tool))?;
        adapter.doctor(paths)
    }

    fn find(&self, tool: &str) -> Option<&dyn Adapter> {
        self.adapters
            .iter()
            .find(|adapter| adapter.tool().as_str() == tool)
            .map(|adapter| adapter.as_ref())
    }
}

struct LivelyAdapter;
struct RainmeterAdapter;
struct YasbAdapter;
struct KomorebiAdapter;
struct WindhawkAdapter;

impl Adapter for LivelyAdapter {
    fn tool(&self) -> ToolName {
        ToolName::Lively
    }
    fn metadata(&self, paths: &AppPaths) -> AdapterMetadata {
        base_metadata(
            ToolName::Lively,
            "https://www.rocksdanister.com/lively/",
            vec![paths.generated_adapter_dir("lively").display().to_string()],
            vec![
                AdapterCapability::Install,
                AdapterCapability::Detect,
                AdapterCapability::WallpaperControl,
                AdapterCapability::ThemeTokens,
                AdapterCapability::BackupRollback,
            ],
            AdapterReloadMode::Command,
            AdapterSafetyLevel::Safe,
        )
    }
    fn detect(&self, paths: &AppPaths) -> AdapterStatus {
        let mut candidates = vec![
            env_path("%LOCALAPPDATA%\\Programs\\Lively Wallpaper\\Lively.exe"),
            env_path("%PROGRAMFILES%\\Lively Wallpaper\\Lively.exe"),
        ];
        candidates.extend(registry_install_candidates("Lively", "Lively.exe"));
        detect_from_candidates(self.metadata(paths), candidates, None)
    }
    fn install(&self, _paths: &AppPaths) -> Result<String> {
        Ok("Install Lively Wallpaper from the official site or GitHub releases.".to_string())
    }
    fn render(&self, request: &ApplyRequest) -> Result<GeneratedArtifact> {
        let dir = request.paths.generated_adapter_dir("lively");
        fs::create_dir_all(&dir)?;
        let file = dir.join("wallpaper.json");
        let content = serde_json::json!({"wallpaper": request.config.wallpaper.current, "source_type": request.config.wallpaper.source_type, "fit_mode": request.config.wallpaper.fit_mode, "accent": request.tokens.accent});
        fs::write(&file, serde_json::to_vec_pretty(&content)?)?;
        Ok(artifact("lively", vec![file]))
    }
    fn apply(&self, request: &ApplyRequest) -> Result<GeneratedArtifact> {
        let mut artifact = self.render(request)?;
        let source = PathBuf::from(&artifact.files[0]);
        let target = resolve_lively_target(request);
        copy_with_parent(&source, &target)?;
        artifact.live_targets = vec![target.display().to_string()];
        artifact.notes.push("Synced to managed lively payload target.".to_string());

        let resolved_wallpaper = resolve_lively_wallpaper_source(request)?;
        if let Some(lively_exe) = locate_lively_exe() {
            let mut command = Command::new(&lively_exe);
            command.arg("setwp").arg("--file").arg(&resolved_wallpaper);
            let status = command.status().with_context(|| {
                format!(
                    "failed to launch Lively CLI at {}",
                    lively_exe.display()
                )
            })?;
            if !status.success() {
                return Err(anyhow!(
                    "Lively rejected wallpaper apply for {} with exit {:?}",
                    resolved_wallpaper.display(),
                    status.code()
                ));
            }
            artifact
                .notes
                .push(format!("Applied wallpaper through Lively: {}", resolved_wallpaper.display()));
            artifact
                .live_targets
                .push(resolved_wallpaper.display().to_string());
        } else {
            artifact.notes.push(
                "Lively executable was not found; generated wallpaper source but could not apply it."
                    .to_string(),
            );
        }
        Ok(artifact)
    }
    fn backup(&self, request: &ApplyRequest) -> Result<()> {
        let target = resolve_lively_target(request);
        backup_target("lively", "wallpaper", &target, &request.paths)
    }
    fn rollback(&self, request: &ApplyRequest) -> Result<()> {
        let target = resolve_lively_target(request);
        restore_target("lively", "wallpaper", &target, &request.paths)
    }
    fn doctor(&self, paths: &AppPaths) -> Result<String> {
        let status = self.detect(paths);
        Ok(if status.installed {
            "Lively detected and ready for generated wallpaper payloads.".to_string()
        } else {
            "Lively not detected. Install from the official site or configure a managed path."
                .to_string()
        })
    }
}

impl Adapter for RainmeterAdapter {
    fn tool(&self) -> ToolName {
        ToolName::Rainmeter
    }
    fn metadata(&self, paths: &AppPaths) -> AdapterMetadata {
        base_metadata(
            ToolName::Rainmeter,
            "https://www.rainmeter.net/",
            vec![
                paths
                    .generated_adapter_dir("rainmeter")
                    .display()
                    .to_string(),
            ],
            vec![
                AdapterCapability::Install,
                AdapterCapability::Detect,
                AdapterCapability::WidgetConfig,
                AdapterCapability::ThemeTokens,
                AdapterCapability::BackupRollback,
                AdapterCapability::LiveReload,
            ],
            AdapterReloadMode::ProcessRestart,
            AdapterSafetyLevel::Safe,
        )
    }
    fn detect(&self, paths: &AppPaths) -> AdapterStatus {
        let mut candidates = vec![
            env_path("%PROGRAMFILES%\\Rainmeter\\Rainmeter.exe"),
            env_path("%PROGRAMFILES(X86)%\\Rainmeter\\Rainmeter.exe"),
        ];
        candidates.extend(registry_install_candidates("Rainmeter", "Rainmeter.exe"));
        detect_from_candidates(self.metadata(paths), candidates, None)
    }
    fn install(&self, _paths: &AppPaths) -> Result<String> {
        Ok("Install Rainmeter from rainmeter.net or the official documentation link.".to_string())
    }
    fn render(&self, request: &ApplyRequest) -> Result<GeneratedArtifact> {
        let dir = request.paths.generated_adapter_dir("rainmeter");
        fs::create_dir_all(&dir)?;
        let file = dir.join("Muks.ini");
        let resources = dir.join("@Resources").join("MuksVariables.inc");
        let content = format!(
            "[Rainmeter]\nUpdate=1000\nAccurateText=1\nDynamicWindowSize=1\nBackgroundMode=2\nSolidColor=0,0,0,1\n\n[Metadata]\nName=Muks Control Widget\nAuthor=Muks\nInformation=Managed widget generated by Muks\nVersion=1.0\nLicense=MIT\n\n[Variables]\n@include=#@#MuksVariables.inc\n\n[MeasureTime]\nMeasure=Time\nFormat=%H:%M\n\n[MeasureDate]\nMeasure=Time\nFormat=%A, %d %b\n\n[MeasureCpu]\nMeasure=CPU\nProcessor=0\n\n[MeasureRam]\nMeasure=PhysicalMemory\n\n[MeterCard]\nMeter=Shape\nShape=Rectangle 0,0,392,208,24 | Fill Color #SurfaceRgb#,235 | StrokeWidth 1 | Stroke Color #AccentRgb#,90\nAntiAlias=1\nDynamicVariables=1\n\n[MeterAccent]\nMeter=Shape\nX=24\nY=24\nShape=Ellipse 0,0,14,14 | Fill Color #AccentRgb#\nAntiAlias=1\nDynamicVariables=1\n\n[MeterTitle]\nMeter=String\nX=48\nY=18\nFontFace=Segoe UI Variable Display\nFontSize=11\nFontColor=#TextRgb#\nStringCase=Upper\nLetterSpacing=2\nText=MUKS LIVE CONTROL\nAntiAlias=1\nDynamicVariables=1\n\n[MeterClock]\nMeter=String\nMeasureName=MeasureTime\nX=24\nY=54\nFontFace=Segoe UI Variable Display\nFontWeight=700\nFontSize=34\nFontColor=#TextRgb#\nAntiAlias=1\nDynamicVariables=1\n\n[MeterDate]\nMeter=String\nMeasureName=MeasureDate\nX=24\nY=102\nFontFace=Segoe UI Variable Text\nFontSize=11\nFontColor=#TextSoftRgb#\nAntiAlias=1\nDynamicVariables=1\n\n[MeterPresetLabel]\nMeter=String\nX=24\nY=138\nFontFace=Segoe UI Variable Text\nFontSize=10\nFontColor=#TextSoftRgb#\nText=Preset\nAntiAlias=1\nDynamicVariables=1\n\n[MeterPreset]\nMeter=String\nX=24\nY=156\nFontFace=Segoe UI Variable Display\nFontWeight=700\nFontSize=16\nFontColor=#AccentRgb#\nText=#PresetName#\nAntiAlias=1\nDynamicVariables=1\n\n[MeterWallpaperLabel]\nMeter=String\nX=198\nY=138\nFontFace=Segoe UI Variable Text\nFontSize=10\nFontColor=#TextSoftRgb#\nText=Wallpaper\nAntiAlias=1\nDynamicVariables=1\n\n[MeterWallpaper]\nMeter=String\nX=198\nY=156\nW=168\nH=22\nFontFace=Segoe UI Variable Text\nFontSize=12\nFontColor=#TextRgb#\nClipString=2\nText=#WallpaperName#\nAntiAlias=1\nDynamicVariables=1\n\n[MeterCpuLabel]\nMeter=String\nX=198\nY=24\nFontFace=Segoe UI Variable Text\nFontSize=10\nFontColor=#TextSoftRgb#\nText=CPU\nAntiAlias=1\nDynamicVariables=1\n\n[MeterCpu]\nMeter=String\nMeasureName=MeasureCpu\nX=198\nY=42\nFontFace=Segoe UI Variable Display\nFontWeight=700\nFontSize=22\nFontColor=#TextRgb#\nNumOfDecimals=0\nPostfix=%\nAntiAlias=1\nDynamicVariables=1\n\n[MeterRamLabel]\nMeter=String\nX=282\nY=24\nFontFace=Segoe UI Variable Text\nFontSize=10\nFontColor=#TextSoftRgb#\nText=RAM\nAntiAlias=1\nDynamicVariables=1\n\n[MeterRam]\nMeter=String\nMeasureName=MeasureRam\nX=282\nY=42\nFontFace=Segoe UI Variable Display\nFontWeight=700\nFontSize=22\nFontColor=#TextRgb#\nPercentual=1\nNumOfDecimals=0\nPostfix=%\nAntiAlias=1\nDynamicVariables=1\n"
        );
        let variable_content = format!(
            "[Variables]\nAccentHex={}\nAccentSoftHex={}\nBackgroundHex={}\nSurfaceHex={}\nTextHex={}\nTextSoftHex={}\nAccentRgb={}\nAccentSoftRgb={}\nBackgroundRgb={}\nSurfaceRgb={}\nTextRgb={}\nTextSoftRgb={}\nPresetName={}\nWallpaperName={}\n",
            request.tokens.accent,
            request.tokens.accent_soft,
            request.tokens.background,
            request.tokens.surface,
            request.tokens.text,
            soften_hex(&request.tokens.text),
            hex_to_rgb_triplet(&request.tokens.accent),
            hex_to_rgb_triplet(&request.tokens.accent_soft),
            hex_to_rgb_triplet(&request.tokens.background),
            hex_to_rgb_triplet(&request.tokens.surface),
            hex_to_rgb_triplet(&request.tokens.text),
            hex_to_rgb_triplet(&soften_hex(&request.tokens.text)),
            request.config.theme.preset,
            request.config.wallpaper.current
        );
        if let Some(parent) = resources.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&file, content)?;
        fs::write(&resources, variable_content)?;
        Ok(artifact("rainmeter", vec![file, resources]))
    }
    fn apply(&self, request: &ApplyRequest) -> Result<GeneratedArtifact> {
        let mut artifact = self.render(request)?;
        let source_skin = PathBuf::from(&artifact.files[0]);
        let source_resources = PathBuf::from(&artifact.files[1]);
        let (target_skin, target_resources) = resolve_rainmeter_targets(request);
        copy_with_parent(&source_skin, &target_skin)?;
        copy_with_parent(&source_resources, &target_resources)?;
        artifact.live_targets = vec![
            target_skin.display().to_string(),
            target_resources.display().to_string(),
        ];
        let (runtime_skin, runtime_resources) = resolve_rainmeter_runtime_targets(request);
        if runtime_skin != target_skin || runtime_resources != target_resources {
            copy_with_parent(&source_skin, &runtime_skin)?;
            copy_with_parent(&source_resources, &runtime_resources)?;
            artifact.live_targets.push(runtime_skin.display().to_string());
            artifact
                .live_targets
                .push(runtime_resources.display().to_string());
            artifact.notes.push(format!(
                "Copied Rainmeter skin into active SkinPath at {}.",
                runtime_skin.display()
            ));
        }
        if let Some(executable) = locate_rainmeter_exe() {
            let _ = launch_gui_process(&executable);
            std::thread::sleep(Duration::from_millis(1200));
            let _ = Command::new(&executable)
                .args(["!ActivateConfig", "Muks", "Muks.ini"])
                .status();
            let _ = Command::new(executable).arg("!RefreshApp").status();
            artifact.notes.push(
                "Attempted Rainmeter activation and refresh for the managed Muks skin."
                    .to_string(),
            );
        }
        Ok(artifact)
    }
    fn backup(&self, request: &ApplyRequest) -> Result<()> {
        let (target_skin, target_resources) = resolve_rainmeter_targets(request);
        backup_target("rainmeter", "skin", &target_skin, &request.paths)?;
        backup_target("rainmeter", "variables", &target_resources, &request.paths)?;
        let (runtime_skin, runtime_resources) = resolve_rainmeter_runtime_targets(request);
        if runtime_skin != target_skin || runtime_resources != target_resources {
            backup_target("rainmeter", "runtime-skin", &runtime_skin, &request.paths)?;
            backup_target(
                "rainmeter",
                "runtime-variables",
                &runtime_resources,
                &request.paths,
            )?;
        }
        Ok(())
    }
    fn rollback(&self, request: &ApplyRequest) -> Result<()> {
        let (target_skin, target_resources) = resolve_rainmeter_targets(request);
        restore_target("rainmeter", "skin", &target_skin, &request.paths)?;
        restore_target("rainmeter", "variables", &target_resources, &request.paths)?;
        let (runtime_skin, runtime_resources) = resolve_rainmeter_runtime_targets(request);
        if runtime_skin != target_skin || runtime_resources != target_resources {
            restore_target("rainmeter", "runtime-skin", &runtime_skin, &request.paths)?;
            restore_target(
                "rainmeter",
                "runtime-variables",
                &runtime_resources,
                &request.paths,
            )?;
        }
        Ok(())
    }
    fn doctor(&self, paths: &AppPaths) -> Result<String> {
        let status = self.detect(paths);
        Ok(if status.installed {
            "Rainmeter detected. Generated themes can be copied into a managed skin path."
                .to_string()
        } else {
            "Rainmeter missing. Install the official release before applying widget skins."
                .to_string()
        })
    }
}

impl Adapter for YasbAdapter {
    fn tool(&self) -> ToolName {
        ToolName::Yasb
    }
    fn metadata(&self, paths: &AppPaths) -> AdapterMetadata {
        base_metadata(
            ToolName::Yasb,
            "https://yasb.dev/",
            vec![paths.generated_adapter_dir("yasb").display().to_string()],
            vec![
                AdapterCapability::Install,
                AdapterCapability::Detect,
                AdapterCapability::BarConfig,
                AdapterCapability::ThemeTokens,
                AdapterCapability::LiveReload,
                AdapterCapability::BackupRollback,
            ],
            AdapterReloadMode::ConfigTouch,
            AdapterSafetyLevel::Safe,
        )
    }
    fn detect(&self, paths: &AppPaths) -> AdapterStatus {
        let mut candidates = vec![
            env_path("%PROGRAMFILES%\\YASB\\yasb.exe"),
            env_path("%LOCALAPPDATA%\\Programs\\YASB\\yasb.exe"),
        ];
        candidates.extend(registry_install_candidates("YASB", "yasb.exe"));
        detect_from_candidates(self.metadata(paths), candidates, None)
    }
    fn install(&self, _paths: &AppPaths) -> Result<String> {
        Ok("Install YASB from the official installer or official GitHub releases.".to_string())
    }
    fn render(&self, request: &ApplyRequest) -> Result<GeneratedArtifact> {
        let dir = request.paths.generated_adapter_dir("yasb");
        fs::create_dir_all(&dir)?;
        let config_file = dir.join("config.yaml");
        let styles_file = dir.join("styles.css");
        let config = format!(
            "bars:\n  primary:\n    screens: [\"*\"]\n    widgets:\n      left: [\"komorebi-workspaces\", \"quick-launch\"]\n      center: [\"clock\"]\n      right: [\"media\", \"weather\", \"system\"]\n  theme:\n    preset: \"{}\"\n",
            request.config.theme.preset
        );
        let styles = format!(
            ":root {{\n  --muks-accent: {};\n  --muks-accent-soft: {};\n  --muks-surface: {};\n  --muks-text: {};\n}}\n",
            request.tokens.accent,
            request.tokens.accent_soft,
            request.tokens.surface,
            request.tokens.text
        );
        fs::write(&config_file, config)?;
        fs::write(&styles_file, styles)?;
        Ok(artifact("yasb", vec![config_file, styles_file]))
    }
    fn apply(&self, request: &ApplyRequest) -> Result<GeneratedArtifact> {
        let mut artifact = self.render(request)?;
        let source_config = PathBuf::from(&artifact.files[0]);
        let source_styles = PathBuf::from(&artifact.files[1]);
        let (target_config, target_styles) = resolve_yasb_targets(request);
        copy_with_parent(&source_config, &target_config)?;
        copy_with_parent(&source_styles, &target_styles)?;
        artifact.live_targets = vec![
            target_config.display().to_string(),
            target_styles.display().to_string(),
        ];
        if command_exists("yasb.exe") {
            let _ = std::process::Command::new("yasb").arg("--reload").status();
            artifact
                .notes
                .push("Attempted YASB reload hook.".to_string());
        }
        Ok(artifact)
    }
    fn backup(&self, request: &ApplyRequest) -> Result<()> {
        let (target_config, target_styles) = resolve_yasb_targets(request);
        backup_target("yasb", "config", &target_config, &request.paths)?;
        backup_target("yasb", "styles", &target_styles, &request.paths)
    }
    fn rollback(&self, request: &ApplyRequest) -> Result<()> {
        let (target_config, target_styles) = resolve_yasb_targets(request);
        restore_target("yasb", "config", &target_config, &request.paths)?;
        restore_target("yasb", "styles", &target_styles, &request.paths)
    }
    fn doctor(&self, paths: &AppPaths) -> Result<String> {
        let status = self.detect(paths);
        Ok(if status.installed {
            "YASB detected. Generated bar config and styles are ready.".to_string()
        } else {
            "YASB missing. Install the official build from yasb.dev or GitHub.".to_string()
        })
    }
}

impl Adapter for KomorebiAdapter {
    fn tool(&self) -> ToolName {
        ToolName::Komorebi
    }
    fn metadata(&self, paths: &AppPaths) -> AdapterMetadata {
        base_metadata(
            ToolName::Komorebi,
            "https://github.com/LGUG2Z/komorebi",
            vec![
                paths
                    .generated_adapter_dir("komorebi")
                    .display()
                    .to_string(),
            ],
            vec![
                AdapterCapability::Install,
                AdapterCapability::Detect,
                AdapterCapability::TilingControl,
                AdapterCapability::ThemeTokens,
                AdapterCapability::BackupRollback,
            ],
            AdapterReloadMode::Command,
            AdapterSafetyLevel::Safe,
        )
    }
    fn detect(&self, paths: &AppPaths) -> AdapterStatus {
        let command_version = read_command_output("komorebi", &["--version"]);
        let mut candidates = vec![
            env_path("%USERPROFILE%\\komorebi\\komorebi.exe"),
            env_path("%PROGRAMFILES%\\komorebi\\komorebi.exe"),
        ];
        candidates.extend(registry_install_candidates(
            "Komorebi|komorebi",
            "komorebi.exe",
        ));
        let mut status = detect_from_candidates(self.metadata(paths), candidates, command_version);
        if command_exists("komorebi.exe") {
            status.installed = true;
            status.health = AdapterHealth::Healthy;
            if status.version.is_none() {
                status.version = read_command_output("komorebi", &["--version"]);
            }
        }
        status
    }
    fn install(&self, _paths: &AppPaths) -> Result<String> {
        Ok(
            "Install Komorebi from the official GitHub releases or official package source."
                .to_string(),
        )
    }
    fn render(&self, request: &ApplyRequest) -> Result<GeneratedArtifact> {
        let dir = request.paths.generated_adapter_dir("komorebi");
        fs::create_dir_all(&dir)?;
        let file = dir.join("komorebi.json");
        let content = serde_json::json!({"theme": request.config.theme.preset, "border_accent": request.tokens.accent, "transparency": 0.92, "workspaces": ["1", "2", "3", "4", "5"]});
        fs::write(&file, serde_json::to_vec_pretty(&content)?)?;
        Ok(artifact("komorebi", vec![file]))
    }
    fn apply(&self, request: &ApplyRequest) -> Result<GeneratedArtifact> {
        let mut artifact = self.render(request)?;
        let source = PathBuf::from(&artifact.files[0]);
        let target = resolve_komorebi_target(request);
        copy_with_parent(&source, &target)?;
        artifact.live_targets = vec![target.display().to_string()];
        if command_exists("komorebic.exe") {
            let _ = std::process::Command::new("komorebic")
                .arg("reload-configuration")
                .status();
            artifact
                .notes
                .push("Attempted Komorebi reload hook.".to_string());
        }
        Ok(artifact)
    }
    fn backup(&self, request: &ApplyRequest) -> Result<()> {
        let target = resolve_komorebi_target(request);
        backup_target("komorebi", "config", &target, &request.paths)
    }
    fn rollback(&self, request: &ApplyRequest) -> Result<()> {
        let target = resolve_komorebi_target(request);
        restore_target("komorebi", "config", &target, &request.paths)
    }
    fn doctor(&self, paths: &AppPaths) -> Result<String> {
        let status = self.detect(paths);
        Ok(if status.installed {
            "Komorebi detected. CLI control path is available when the binary is on PATH."
                .to_string()
        } else {
            "Komorebi not detected. Install the official release and ensure the binary is reachable.".to_string()
        })
    }
}

impl Adapter for WindhawkAdapter {
    fn tool(&self) -> ToolName {
        ToolName::Windhawk
    }
    fn metadata(&self, paths: &AppPaths) -> AdapterMetadata {
        base_metadata(
            ToolName::Windhawk,
            "https://windhawk.org/",
            vec![
                paths
                    .generated_adapter_dir("windhawk")
                    .display()
                    .to_string(),
            ],
            vec![
                AdapterCapability::Install,
                AdapterCapability::Detect,
                AdapterCapability::ManagedMods,
                AdapterCapability::BackupRollback,
            ],
            AdapterReloadMode::Command,
            AdapterSafetyLevel::Curated,
        )
    }
    fn detect(&self, paths: &AppPaths) -> AdapterStatus {
        let mut candidates = vec![
            env_path("%PROGRAMFILES%\\Windhawk\\windhawk.exe"),
            env_path("%LOCALAPPDATA%\\Programs\\Windhawk\\windhawk.exe"),
        ];
        candidates.extend(registry_install_candidates("Windhawk", "windhawk.exe"));
        detect_from_candidates(self.metadata(paths), candidates, None)
    }
    fn install(&self, _paths: &AppPaths) -> Result<String> {
        Ok(
            "Install Windhawk from windhawk.org and manage only curated mods through Muks."
                .to_string(),
        )
    }
    fn render(&self, request: &ApplyRequest) -> Result<GeneratedArtifact> {
        let dir = request.paths.generated_adapter_dir("windhawk");
        fs::create_dir_all(&dir)?;
        let file = dir.join("managed-mods.toml");
        let content = format!(
            "[mods.taskbar_styler]\nenabled = true\naccent = \"{}\"\n\n[mods.notification_center_styler]\nenabled = true\nsurface = \"{}\"\n",
            request.tokens.accent, request.tokens.surface
        );
        fs::write(&file, content)?;
        Ok(artifact("windhawk", vec![file]))
    }
    fn apply(&self, request: &ApplyRequest) -> Result<GeneratedArtifact> {
        let mut artifact = self.render(request)?;
        let source = PathBuf::from(&artifact.files[0]);
        let target = resolve_windhawk_target(request);
        copy_with_parent(&source, &target)?;
        artifact.live_targets = vec![target.display().to_string()];
        artifact.notes.push(
            "Synced curated mod payload. Windhawk UI import may still be required.".to_string(),
        );
        Ok(artifact)
    }
    fn backup(&self, request: &ApplyRequest) -> Result<()> {
        let target = resolve_windhawk_target(request);
        backup_target("windhawk", "mods", &target, &request.paths)
    }
    fn rollback(&self, request: &ApplyRequest) -> Result<()> {
        let target = resolve_windhawk_target(request);
        restore_target("windhawk", "mods", &target, &request.paths)
    }
    fn doctor(&self, paths: &AppPaths) -> Result<String> {
        let status = self.detect(paths);
        Ok(if status.installed {
            "Windhawk detected. Muks will only manage curated allowlisted mod settings.".to_string()
        } else {
            "Windhawk not detected. Install from windhawk.org for curated mod management."
                .to_string()
        })
    }
}

fn copy_with_parent(source: &PathBuf, destination: &PathBuf) -> Result<()> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::copy(source, destination).with_context(|| {
        format!(
            "failed to copy {} to {}",
            source.display(),
            destination.display()
        )
    })?;
    Ok(())
}

fn launch_gui_process(executable: &Path) -> Result<()> {
    let escaped = executable.display().to_string().replace('\'', "''");
    let script = format!("Start-Process -FilePath '{}'", escaped);
    let status = Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .status()
        .with_context(|| format!("failed to launch {}", executable.display()))?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!(
            "failed to start GUI process {}",
            executable.display()
        ))
    }
}

fn home_dir() -> PathBuf {
    env::var("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("C:\\"))
}

fn locate_lively_exe() -> Option<PathBuf> {
    let mut candidates = vec![
        env_path("%LOCALAPPDATA%\\Programs\\Lively Wallpaper\\Lively.exe"),
        env_path("%PROGRAMFILES%\\Lively Wallpaper\\Lively.exe"),
    ];
    candidates.extend(registry_install_candidates("Lively", "Lively.exe"));
    candidates.into_iter().find(|candidate| candidate.exists())
}

fn locate_rainmeter_exe() -> Option<PathBuf> {
    let mut candidates = vec![
        env_path("%PROGRAMFILES%\\Rainmeter\\Rainmeter.exe"),
        env_path("%PROGRAMFILES(X86)%\\Rainmeter\\Rainmeter.exe"),
    ];
    candidates.extend(registry_install_candidates("Rainmeter", "Rainmeter.exe"));
    candidates.into_iter().find(|candidate| candidate.exists())
}

fn resolve_lively_target(request: &ApplyRequest) -> PathBuf {
    request
        .paths
        .live_adapter_dir("lively")
        .join("wallpaper.json")
}

fn resolve_rainmeter_targets(request: &ApplyRequest) -> (PathBuf, PathBuf) {
    if let Some(skin_path) = rainmeter_skin_root() {
        let root = skin_path.join("Muks");
        return (
            root.join("Muks.ini"),
            root.join("@Resources").join("MuksVariables.inc"),
        );
    }

    let managed_root = if let Some(path) = &request.config.rainmeter.managed_path {
        let candidate = PathBuf::from(path);
        if candidate
            .file_name()
            .map(|name| name.to_string_lossy().eq_ignore_ascii_case("@Resources"))
            .unwrap_or(false)
        {
            candidate
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or(candidate)
        } else {
            candidate
        }
    } else {
        let default = home_dir().join("Documents").join("Rainmeter").join("Skins").join("Muks");
        if default.parent().map(|parent| parent.exists()).unwrap_or(false) {
            default
        } else {
            request.paths.live_adapter_dir("rainmeter")
        }
    };

    (
        managed_root.join("Muks.ini"),
        managed_root.join("@Resources").join("MuksVariables.inc"),
    )
}

fn resolve_rainmeter_runtime_targets(request: &ApplyRequest) -> (PathBuf, PathBuf) {
    resolve_rainmeter_targets(request)
}

fn resolve_yasb_targets(request: &ApplyRequest) -> (PathBuf, PathBuf) {
    let base = if let Some(path) = &request.config.yasb.managed_path {
        PathBuf::from(path)
    } else {
        let default = home_dir().join(".config").join("yasb");
        if default.exists() {
            default
        } else {
            request.paths.live_adapter_dir("yasb")
        }
    };
    (base.join("config.yaml"), base.join("styles.css"))
}

fn resolve_komorebi_target(request: &ApplyRequest) -> PathBuf {
    if let Some(path) = &request.config.komorebi.managed_path {
        return PathBuf::from(path);
    }

    let default = home_dir().join("komorebi.json");
    if default
        .parent()
        .map(|parent| parent.exists())
        .unwrap_or(false)
    {
        default
    } else {
        request
            .paths
            .live_adapter_dir("komorebi")
            .join("komorebi.json")
    }
}

fn resolve_windhawk_target(request: &ApplyRequest) -> PathBuf {
    if let Some(path) = &request.config.windhawk.managed_path {
        return PathBuf::from(path).join("managed-mods.toml");
    }

    let appdata = env::var("APPDATA").unwrap_or_default();
    let default = PathBuf::from(appdata)
        .join("Windhawk")
        .join("Engine")
        .join("Mods")
        .join("Muks")
        .join("managed-mods.toml");

    if default
        .parent()
        .map(|parent| parent.exists())
        .unwrap_or(false)
    {
        default
    } else {
        request
            .paths
            .live_adapter_dir("windhawk")
            .join("managed-mods.toml")
    }
}

fn rainmeter_skin_root() -> Option<PathBuf> {
    let ini_path = env::var("APPDATA")
        .ok()
        .map(PathBuf::from)?
        .join("Rainmeter")
        .join("Rainmeter.ini");
    let contents = read_text_file(&ini_path).ok()?;
    for line in contents.lines() {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix("SkinPath=") {
            let path = PathBuf::from(value.trim());
            if path.exists() {
                return Some(path);
            }
        }
    }
    None
}

fn read_text_file(path: &Path) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let mut units = Vec::with_capacity((bytes.len().saturating_sub(2)) / 2);
        for chunk in bytes[2..].chunks_exact(2) {
            units.push(u16::from_le_bytes([chunk[0], chunk[1]]));
        }
        return String::from_utf16(&units)
            .map_err(|error| anyhow!("failed to decode UTF-16 file {}: {}", path.display(), error));
    }

    String::from_utf8(bytes)
        .map_err(|error| anyhow!("failed to decode UTF-8 file {}: {}", path.display(), error))
}

fn resolve_lively_wallpaper_source(request: &ApplyRequest) -> Result<PathBuf> {
    let source = request.config.wallpaper.current.trim();
    if source.eq_ignore_ascii_case("random") {
        return Ok(PathBuf::from("random"));
    }

    match request.config.wallpaper.source_type.as_str() {
        "file" => Ok(PathBuf::from(source)),
        "url" => {
            let extension = infer_remote_extension(source).unwrap_or("jpg");
            let destination = request
                .paths
                .cache
                .join("wallpapers")
                .join(format!("remote-wallpaper.{}", extension));
            download_wallpaper(source, &destination)?;
            Ok(destination)
        }
        _ => generate_lively_preset_wallpaper(request, source),
    }
}

fn generate_lively_preset_wallpaper(request: &ApplyRequest, preset_name: &str) -> Result<PathBuf> {
    let preset = sanitize_file_segment(preset_name);
    let base = request
        .paths
        .generated_adapter_dir("lively")
        .join("presets")
        .join(&preset);
    fs::create_dir_all(&base)
        .with_context(|| format!("failed to create {}", base.display()))?;

    let html = base.join("index.html");
    let contents = format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\" />\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" />\n<title>Muks {preset}</title>\n<style>\n:root {{\n  --accent: {accent};\n  --accent-soft: {accent_soft};\n  --background: {background};\n  --surface: {surface};\n  --text: {text};\n}}\n* {{ box-sizing: border-box; }}\nhtml, body {{ margin: 0; width: 100%; height: 100%; overflow: hidden; background: radial-gradient(circle at 18% 18%, color-mix(in srgb, var(--accent) 42%, transparent), transparent 32%), radial-gradient(circle at 82% 22%, color-mix(in srgb, var(--accent-soft) 38%, transparent), transparent 30%), linear-gradient(135deg, var(--background), color-mix(in srgb, var(--surface) 72%, black)); color: var(--text); font-family: 'Segoe UI Variable Display', 'Segoe UI', sans-serif; }}\nbody::before {{ content: ''; position: fixed; inset: -10%; background: linear-gradient(120deg, transparent 0%, rgba(255,255,255,0.05) 28%, transparent 55%); transform: rotate(-8deg); animation: sweep 18s linear infinite; }}\n.grid {{ position: fixed; inset: 0; background-image: linear-gradient(rgba(255,255,255,0.05) 1px, transparent 1px), linear-gradient(90deg, rgba(255,255,255,0.05) 1px, transparent 1px); background-size: 90px 90px; mask-image: radial-gradient(circle at center, black, transparent 80%); opacity: 0.2; }}\n.orb {{ position: absolute; border-radius: 999px; filter: blur(10px); opacity: 0.65; animation: float 16s ease-in-out infinite; }}\n.orb.one {{ width: 28vw; height: 28vw; left: 8vw; top: 16vh; background: color-mix(in srgb, var(--accent) 75%, white); }}\n.orb.two {{ width: 22vw; height: 22vw; right: 12vw; top: 10vh; background: color-mix(in srgb, var(--accent-soft) 70%, white); animation-delay: -5s; }}\n.orb.three {{ width: 18vw; height: 18vw; right: 20vw; bottom: 8vh; background: color-mix(in srgb, var(--surface) 82%, white); animation-delay: -9s; }}\n.panel {{ position: absolute; left: 5vw; bottom: 6vh; width: min(520px, 46vw); padding: 28px 32px; border-radius: 28px; background: linear-gradient(180deg, rgba(255,255,255,0.14), rgba(255,255,255,0.06)); border: 1px solid rgba(255,255,255,0.16); backdrop-filter: blur(20px) saturate(135%); box-shadow: 0 20px 80px rgba(0,0,0,0.38); }}\n.kicker {{ font-size: 14px; letter-spacing: 0.28em; text-transform: uppercase; opacity: 0.72; }}\n.title {{ margin: 14px 0 8px; font-size: clamp(42px, 6vw, 78px); font-weight: 700; line-height: 0.95; }}\n.subtitle {{ max-width: 28ch; font-size: clamp(14px, 1.4vw, 20px); line-height: 1.45; opacity: 0.82; }}\n.chips {{ display: flex; gap: 12px; margin-top: 24px; flex-wrap: wrap; }}\n.chip {{ padding: 10px 14px; border-radius: 999px; background: rgba(255,255,255,0.08); border: 1px solid rgba(255,255,255,0.1); font-size: 13px; }}\n@keyframes float {{ 0%, 100% {{ transform: translate3d(0,0,0) scale(1); }} 50% {{ transform: translate3d(0,-22px,0) scale(1.06); }} }}\n@keyframes sweep {{ from {{ transform: translateX(-22%) rotate(-8deg); }} to {{ transform: translateX(22%) rotate(-8deg); }} }}\n</style>\n</head>\n<body>\n<div class=\"grid\"></div>\n<div class=\"orb one\"></div>\n<div class=\"orb two\"></div>\n<div class=\"orb three\"></div>\n<section class=\"panel\">\n  <div class=\"kicker\">Muks Dynamic Wallpaper</div>\n  <div class=\"title\">{preset}</div>\n  <div class=\"subtitle\">Live palette-driven wallpaper generated by Muks so presets actually change something the moment you apply them.</div>\n  <div class=\"chips\">\n    <div class=\"chip\">accent {accent}</div>\n    <div class=\"chip\">surface {surface}</div>\n    <div class=\"chip\">profile {profile}</div>\n  </div>\n</section>\n</body>\n</html>\n",
        preset = preset_name,
        accent = request.tokens.accent,
        accent_soft = request.tokens.accent_soft,
        background = request.tokens.background,
        surface = request.tokens.surface,
        text = request.tokens.text,
        profile = request.config.profile.name
    );
    fs::write(&html, contents).with_context(|| format!("failed to write {}", html.display()))?;
    Ok(html)
}

fn download_wallpaper(url: &str, destination: &Path) -> Result<()> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let escaped_url = url.replace('\'', "''");
    let escaped_destination = destination.display().to_string().replace('\'', "''");
    let script = format!(
        "$ProgressPreference='SilentlyContinue'; Invoke-WebRequest -Uri '{url}' -OutFile '{destination}'",
        url = escaped_url,
        destination = escaped_destination
    );
    let status = Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .status()
        .context("failed to launch PowerShell for wallpaper download")?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("failed to download wallpaper from {}", url))
    }
}

fn infer_remote_extension(url: &str) -> Option<&'static str> {
    let lower = url.to_ascii_lowercase();
    [".mp4", ".webm", ".gif", ".png", ".jpg", ".jpeg", ".bmp"]
        .into_iter()
        .find_map(|suffix| lower.ends_with(suffix).then_some(suffix.trim_start_matches('.')))
}

fn sanitize_file_segment(value: &str) -> String {
    let mut result = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            result.push(ch.to_ascii_lowercase());
        } else if ch.is_whitespace() {
            result.push('-');
        }
    }
    if result.is_empty() {
        "preset".to_string()
    } else {
        result
    }
}

fn hex_to_rgb_triplet(hex: &str) -> String {
    parse_hex_triplet(hex)
        .map(|(r, g, b)| format!("{},{},{}", r, g, b))
        .unwrap_or_else(|| "120,140,180".to_string())
}

fn soften_hex(hex: &str) -> String {
    if let Some((r, g, b)) = parse_hex_triplet(hex) {
        let soften = |value: u8| -> u8 { ((value as f32) * 0.78).round() as u8 };
        format!("#{:02x}{:02x}{:02x}", soften(r), soften(g), soften(b))
    } else {
        "#b3b6c0".to_string()
    }
}

fn parse_hex_triplet(hex: &str) -> Option<(u8, u8, u8)> {
    let raw = hex.trim().trim_start_matches('#');
    if raw.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&raw[0..2], 16).ok()?;
    let g = u8::from_str_radix(&raw[2..4], 16).ok()?;
    let b = u8::from_str_radix(&raw[4..6], 16).ok()?;
    Some((r, g, b))
}

fn adapter_enabled(tool: ToolName, config: &AppConfig) -> bool {
    match tool {
        ToolName::Lively => true,
        ToolName::Rainmeter => config.rainmeter.enabled,
        ToolName::Yasb => config.yasb.enabled,
        ToolName::Komorebi => config.komorebi.enabled,
        ToolName::Windhawk => config.windhawk.enabled,
    }
}

fn rollback_applied(
    adapters: &[Box<dyn Adapter>],
    applied_indexes: &[usize],
    request: &ApplyRequest,
) {
    for index in applied_indexes.iter().rev() {
        if let Some(adapter) = adapters.get(*index) {
            if let Err(error) = adapter.rollback(request) {
                tracing::error!(
                    "rollback failed for {} while handling apply error: {}",
                    adapter.tool(),
                    error
                );
            }
        }
    }
}

fn backup_target(adapter: &str, key: &str, target: &PathBuf, paths: &AppPaths) -> Result<()> {
    let (backup_file, marker_file) = backup_entry_paths(adapter, key, paths);
    if let Some(parent) = backup_file.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    if target.exists() {
        copy_with_parent(target, &backup_file)?;
        if marker_file.exists() {
            fs::remove_file(&marker_file)
                .with_context(|| format!("failed to remove {}", marker_file.display()))?;
        }
    } else {
        fs::write(&marker_file, b"missing")
            .with_context(|| format!("failed to write {}", marker_file.display()))?;
        if backup_file.exists() {
            fs::remove_file(&backup_file)
                .with_context(|| format!("failed to remove {}", backup_file.display()))?;
        }
    }

    Ok(())
}

fn restore_target(adapter: &str, key: &str, target: &PathBuf, paths: &AppPaths) -> Result<()> {
    let (backup_file, marker_file) = backup_entry_paths(adapter, key, paths);

    if backup_file.exists() {
        copy_with_parent(&backup_file, target)?;
    } else if marker_file.exists() && target.exists() {
        fs::remove_file(target)
            .with_context(|| format!("failed to remove {}", target.display()))?;
    }

    Ok(())
}

fn backup_entry_paths(adapter: &str, key: &str, paths: &AppPaths) -> (PathBuf, PathBuf) {
    let base = paths.backup_adapter_dir(adapter);
    (
        base.join(format!("{}.bak", key)),
        base.join(format!("{}.missing", key)),
    )
}

fn env_path(template: &str) -> PathBuf {
    let mut value = template.to_string();
    for (key, replacement) in [
        (
            "%LOCALAPPDATA%",
            env::var("LOCALAPPDATA").unwrap_or_default(),
        ),
        (
            "%PROGRAMFILES%",
            env::var("ProgramFiles").unwrap_or_default(),
        ),
        (
            "%PROGRAMFILES(X86)%",
            env::var("ProgramFiles(x86)").unwrap_or_default(),
        ),
        ("%USERPROFILE%", env::var("USERPROFILE").unwrap_or_default()),
    ] {
        value = value.replace(key, &replacement);
    }
    PathBuf::from(value)
}

fn registry_install_candidates(display_name_pattern: &str, exe_name: &str) -> Vec<PathBuf> {
    let escaped_pattern = display_name_pattern.replace('\'', "''");
    let escaped_exe = exe_name.replace('\'', "''");
    let script = format!(
        "$roots=@('HKLM:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\*','HKLM:\\Software\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\*','HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\*'); \
         Get-ItemProperty -Path $roots -ErrorAction SilentlyContinue | \
         Where-Object {{ $_.DisplayName -and $_.DisplayName -match '{pattern}' }} | \
         ForEach-Object {{ \
           if ($_.DisplayIcon) {{ ($_.DisplayIcon -split ',')[0].Trim('\"') }}; \
           if ($_.InstallLocation) {{ Join-Path $_.InstallLocation '{exe}' }} \
         }}",
        pattern = escaped_pattern,
        exe = escaped_exe
    );

    let output = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .output();

    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    let raw = String::from_utf8_lossy(&output.stdout);
    let mut candidates = Vec::new();
    for line in raw.lines() {
        let cleaned = line.trim().trim_matches('"');
        if cleaned.is_empty() {
            continue;
        }
        let path = PathBuf::from(cleaned);
        if path.exists() {
            candidates.push(path);
        }
    }

    candidates.sort();
    candidates.dedup();
    candidates
}

fn detect_from_candidates(
    metadata: AdapterMetadata,
    candidates: Vec<PathBuf>,
    version: Option<String>,
) -> AdapterStatus {
    let installed = candidates.iter().any(|path| path.exists());
    let detected_version = if installed {
        version.or_else(|| {
            candidates
                .iter()
                .find(|path| path.exists())
                .and_then(read_windows_file_version)
        })
    } else {
        version
    };
    let details = if installed {
        "Detected through common install paths.".to_string()
    } else {
        "Not detected in common install paths.".to_string()
    };
    AdapterStatus {
        tool: metadata.tool,
        installed,
        version: detected_version,
        health: if installed {
            AdapterHealth::Healthy
        } else {
            AdapterHealth::Missing
        },
        details,
        metadata,
    }
}

fn base_metadata(
    tool: ToolName,
    install_source: &str,
    owned_paths: Vec<String>,
    capabilities: Vec<AdapterCapability>,
    reload_mode: AdapterReloadMode,
    safety_level: AdapterSafetyLevel,
) -> AdapterMetadata {
    AdapterMetadata {
        tool,
        display_name: tool.display_name().to_string(),
        install_source: install_source.to_string(),
        owned_paths,
        capabilities,
        reload_mode,
        verification_mode: "path-and-generated-file-check".to_string(),
        safety_level,
    }
}

fn artifact(adapter: &str, files: Vec<PathBuf>) -> GeneratedArtifact {
    GeneratedArtifact {
        adapter: adapter.to_string(),
        files: files
            .into_iter()
            .map(|file| file.display().to_string())
            .collect(),
        live_targets: Vec::new(),
        notes: Vec::new(),
    }
}

fn read_windows_file_version(path: &PathBuf) -> Option<String> {
    if !path.exists() {
        return None;
    }

    let escaped = path.display().to_string().replace('\'', "''");
    let script = format!(
        "$ErrorActionPreference='Stop'; (Get-Item -LiteralPath '{}').VersionInfo.ProductVersion",
        escaped
    );
    read_command_output("powershell", &["-NoProfile", "-Command", &script])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_enable_flags_follow_config() {
        let mut config = AppConfig::default();
        assert!(adapter_enabled(ToolName::Lively, &config));
        assert!(adapter_enabled(ToolName::Rainmeter, &config));
        assert!(adapter_enabled(ToolName::Yasb, &config));
        assert!(adapter_enabled(ToolName::Komorebi, &config));
        assert!(adapter_enabled(ToolName::Windhawk, &config));

        config.rainmeter.enabled = false;
        config.yasb.enabled = false;
        config.komorebi.enabled = false;
        config.windhawk.enabled = false;

        assert!(adapter_enabled(ToolName::Lively, &config));
        assert!(!adapter_enabled(ToolName::Rainmeter, &config));
        assert!(!adapter_enabled(ToolName::Yasb, &config));
        assert!(!adapter_enabled(ToolName::Komorebi, &config));
        assert!(!adapter_enabled(ToolName::Windhawk, &config));
    }
}
