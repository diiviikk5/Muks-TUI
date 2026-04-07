use anyhow::{Context, Result, anyhow};
use muks_common::{
    AdapterCapability, AdapterHealth, AdapterMetadata, AdapterReloadMode, AdapterSafetyLevel,
    AdapterStatus, AppPaths, ToolName, command_exists, read_command_output,
};
use muks_core::{AppConfig, theme::ThemeTokens};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;

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
    fn backup(&self, paths: &AppPaths) -> Result<()>;
    fn rollback(&self, paths: &AppPaths) -> Result<()>;
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
        for adapter in &self.adapters {
            if let Err(error) = adapter.backup(&request.paths) {
                if !request.best_effort {
                    return Err(error).context(format!("backup failed for {}", adapter.tool()));
                }
            }

            match adapter.apply(request) {
                Ok(artifact) => generated.push(artifact),
                Err(error) if request.best_effort => {
                    tracing::warn!("best effort apply failed for {}: {}", adapter.tool(), error);
                }
                Err(error) => {
                    return Err(error).context(format!("apply failed for {}", adapter.tool()));
                }
            }
        }
        Ok(generated)
    }

    pub fn render_all(&self, request: &ApplyRequest) -> Result<Vec<GeneratedArtifact>> {
        self.adapters
            .iter()
            .map(|adapter| adapter.render(request))
            .collect()
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
        detect_from_candidates(
            self.metadata(paths),
            vec![
                env_path("%LOCALAPPDATA%\\Programs\\Lively Wallpaper\\Lively.exe"),
                env_path("%PROGRAMFILES%\\Lively Wallpaper\\Lively.exe"),
            ],
            None,
        )
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
        artifact
            .notes
            .push("Synced to managed lively payload target.".to_string());
        Ok(artifact)
    }
    fn backup(&self, _paths: &AppPaths) -> Result<()> {
        Ok(())
    }
    fn rollback(&self, _paths: &AppPaths) -> Result<()> {
        Ok(())
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
        detect_from_candidates(
            self.metadata(paths),
            vec![
                env_path("%PROGRAMFILES%\\Rainmeter\\Rainmeter.exe"),
                env_path("%PROGRAMFILES(X86)%\\Rainmeter\\Rainmeter.exe"),
            ],
            None,
        )
    }
    fn install(&self, _paths: &AppPaths) -> Result<String> {
        Ok("Install Rainmeter from rainmeter.net or the official documentation link.".to_string())
    }
    fn render(&self, request: &ApplyRequest) -> Result<GeneratedArtifact> {
        let dir = request.paths.generated_adapter_dir("rainmeter");
        fs::create_dir_all(&dir)?;
        let file = dir.join("muks-theme.ini");
        let content = format!(
            "[Variables]\nAccent={}\nAccentSoft={}\nBackground={}\nText={}\nWallpaper={}\n",
            request.tokens.accent,
            request.tokens.accent_soft,
            request.tokens.background,
            request.tokens.text,
            request.config.wallpaper.current
        );
        fs::write(&file, content)?;
        Ok(artifact("rainmeter", vec![file]))
    }
    fn apply(&self, request: &ApplyRequest) -> Result<GeneratedArtifact> {
        let mut artifact = self.render(request)?;
        let source = PathBuf::from(&artifact.files[0]);
        let target = resolve_rainmeter_target(request);
        copy_with_parent(&source, &target)?;
        artifact.live_targets = vec![target.display().to_string()];
        if let Some(executable) = locate_rainmeter_exe() {
            let _ = std::process::Command::new(executable)
                .arg("!RefreshApp")
                .status();
            artifact
                .notes
                .push("Attempted Rainmeter refresh hook.".to_string());
        }
        Ok(artifact)
    }
    fn backup(&self, _paths: &AppPaths) -> Result<()> {
        Ok(())
    }
    fn rollback(&self, _paths: &AppPaths) -> Result<()> {
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
        detect_from_candidates(
            self.metadata(paths),
            vec![
                env_path("%PROGRAMFILES%\\YASB\\yasb.exe"),
                env_path("%LOCALAPPDATA%\\Programs\\YASB\\yasb.exe"),
            ],
            None,
        )
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
    fn backup(&self, _paths: &AppPaths) -> Result<()> {
        Ok(())
    }
    fn rollback(&self, _paths: &AppPaths) -> Result<()> {
        Ok(())
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
        let mut status = detect_from_candidates(
            self.metadata(paths),
            vec![
                env_path("%USERPROFILE%\\komorebi\\komorebi.exe"),
                env_path("%PROGRAMFILES%\\komorebi\\komorebi.exe"),
            ],
            command_version,
        );
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
    fn backup(&self, _paths: &AppPaths) -> Result<()> {
        Ok(())
    }
    fn rollback(&self, _paths: &AppPaths) -> Result<()> {
        Ok(())
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
        detect_from_candidates(
            self.metadata(paths),
            vec![
                env_path("%PROGRAMFILES%\\Windhawk\\windhawk.exe"),
                env_path("%LOCALAPPDATA%\\Programs\\Windhawk\\windhawk.exe"),
            ],
            None,
        )
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
    fn backup(&self, _paths: &AppPaths) -> Result<()> {
        Ok(())
    }
    fn rollback(&self, _paths: &AppPaths) -> Result<()> {
        Ok(())
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

fn home_dir() -> PathBuf {
    env::var("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("C:\\"))
}

fn locate_rainmeter_exe() -> Option<PathBuf> {
    [
        env_path("%PROGRAMFILES%\\Rainmeter\\Rainmeter.exe"),
        env_path("%PROGRAMFILES(X86)%\\Rainmeter\\Rainmeter.exe"),
    ]
    .into_iter()
    .find(|candidate| candidate.exists())
}

fn resolve_lively_target(request: &ApplyRequest) -> PathBuf {
    request
        .paths
        .live_adapter_dir("lively")
        .join("wallpaper.json")
}

fn resolve_rainmeter_target(request: &ApplyRequest) -> PathBuf {
    if let Some(path) = &request.config.rainmeter.managed_path {
        return PathBuf::from(path).join("muks-theme.ini");
    }

    let default = home_dir()
        .join("Documents")
        .join("Rainmeter")
        .join("Skins")
        .join("Muks")
        .join("@Resources")
        .join("muks-theme.ini");

    if default
        .parent()
        .map(|parent| parent.exists())
        .unwrap_or(false)
    {
        default
    } else {
        request
            .paths
            .live_adapter_dir("rainmeter")
            .join("muks-theme.ini")
    }
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

fn detect_from_candidates(
    metadata: AdapterMetadata,
    candidates: Vec<PathBuf>,
    version: Option<String>,
) -> AdapterStatus {
    let installed = candidates.iter().any(|path| path.exists());
    let details = if installed {
        "Detected through common install paths.".to_string()
    } else {
        "Not detected in common install paths.".to_string()
    };
    AdapterStatus {
        tool: metadata.tool,
        installed,
        version,
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
