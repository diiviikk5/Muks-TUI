use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotManifest {
    pub id: String,
    pub label: String,
    pub created_at: String,
    pub config_file: String,
    pub generated_dir: String,
}

pub fn create_snapshot(
    snapshots_dir: &Path,
    config_file: &Path,
    generated_dir: &Path,
    label: &str,
) -> Result<SnapshotManifest> {
    let id = format!(
        "{}-{}",
        label.replace(' ', "-").to_lowercase(),
        uuid::Uuid::new_v4().simple()
    );
    let dir = snapshots_dir.join(&id);
    let generated_copy = dir.join("generated");
    fs::create_dir_all(&generated_copy)
        .with_context(|| format!("failed to create {}", generated_copy.display()))?;

    let config_copy = dir.join("config.toml");
    if config_file.exists() {
        fs::copy(config_file, &config_copy).with_context(|| {
            format!(
                "failed to copy config from {} to {}",
                config_file.display(),
                config_copy.display()
            )
        })?;
    }

    copy_dir_recursive(generated_dir, &generated_copy)?;

    let manifest = SnapshotManifest {
        id: id.clone(),
        label: label.to_string(),
        created_at: epoch_now_string(),
        config_file: config_copy.to_string_lossy().to_string(),
        generated_dir: generated_copy.to_string_lossy().to_string(),
    };
    let manifest_path = dir.join("manifest.json");
    fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest)?)
        .with_context(|| format!("failed to write {}", manifest_path.display()))?;

    Ok(manifest)
}

pub fn list_snapshots(snapshots_dir: &Path) -> Result<Vec<SnapshotManifest>> {
    if !snapshots_dir.exists() {
        return Ok(Vec::new());
    }

    let mut snapshots = Vec::new();
    for entry in fs::read_dir(snapshots_dir)
        .with_context(|| format!("failed to read {}", snapshots_dir.display()))?
    {
        let entry = entry?;
        let manifest_path = entry.path().join("manifest.json");
        if !manifest_path.exists() {
            continue;
        }
        let raw = fs::read_to_string(&manifest_path)
            .with_context(|| format!("failed to read {}", manifest_path.display()))?;
        let manifest: SnapshotManifest = serde_json::from_str(&raw)
            .with_context(|| format!("failed to parse {}", manifest_path.display()))?;
        snapshots.push(manifest);
    }
    snapshots.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(snapshots)
}

pub fn restore_snapshot(
    snapshot_id: &str,
    snapshots_dir: &Path,
    config_file: &Path,
    generated_dir: &Path,
) -> Result<SnapshotManifest> {
    let manifest_path = snapshots_dir.join(snapshot_id).join("manifest.json");
    let raw = fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    let manifest: SnapshotManifest =
        serde_json::from_str(&raw).context("failed to parse snapshot manifest")?;

    fs::copy(&manifest.config_file, config_file).with_context(|| {
        format!(
            "failed to restore config from {} to {}",
            manifest.config_file,
            config_file.display()
        )
    })?;

    if generated_dir.exists() {
        fs::remove_dir_all(generated_dir)
            .with_context(|| format!("failed to remove {}", generated_dir.display()))?;
    }
    fs::create_dir_all(generated_dir)
        .with_context(|| format!("failed to create {}", generated_dir.display()))?;
    copy_dir_recursive(Path::new(&manifest.generated_dir), generated_dir)?;
    Ok(manifest)
}

fn copy_dir_recursive(from: &Path, to: &Path) -> Result<()> {
    if !from.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(from).with_context(|| format!("failed to read {}", from.display()))? {
        let entry = entry?;
        let source = entry.path();
        let target = to.join(entry.file_name());
        if source.is_dir() {
            fs::create_dir_all(&target)
                .with_context(|| format!("failed to create {}", target.display()))?;
            copy_dir_recursive(&source, &target)?;
        } else {
            fs::copy(&source, &target).with_context(|| {
                format!(
                    "failed to copy {} to {}",
                    source.display(),
                    target.display()
                )
            })?;
        }
    }
    Ok(())
}

fn epoch_now_string() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}
