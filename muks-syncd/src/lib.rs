use anyhow::Result;
use muks_adapters::{AdapterRegistry, ApplyRequest};
use muks_core::{MuksState, theme::derive_tokens};
use serde::{Deserialize, Serialize};
use std::thread;
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchReport {
    pub iterations: usize,
    pub triggered_apply: bool,
    pub apply_count: usize,
}

pub struct SyncDaemon {
    state: MuksState,
    adapters: AdapterRegistry,
}

impl SyncDaemon {
    pub fn new() -> Result<Self> {
        Ok(Self {
            state: MuksState::new()?,
            adapters: AdapterRegistry::new(),
        })
    }

    pub fn watch(&self, iterations: usize, interval: Duration) -> Result<WatchReport> {
        let config = self.state.config()?;
        if !config.sync.watch_enabled {
            return Ok(WatchReport {
                iterations: 0,
                triggered_apply: false,
                apply_count: 0,
            });
        }
        let debounce_window = Duration::from_millis(config.sync.debounce_ms.max(100));
        let mut last_seen_signature = watch_signature(&self.state, &config)?;
        let mut pending_since: Option<SystemTime> = None;
        let mut triggered_apply = false;
        let mut apply_count = 0usize;

        for _ in 0..iterations {
            thread::sleep(interval);
            let config = self.state.config()?;
            let current_signature = watch_signature(&self.state, &config)?;

            if current_signature > last_seen_signature {
                last_seen_signature = current_signature;
                pending_since = Some(SystemTime::now());
            }

            let should_apply = pending_since
                .and_then(|since| SystemTime::now().duration_since(since).ok())
                .map(|elapsed| elapsed >= debounce_window)
                .unwrap_or(false);

            if should_apply && config.sync.live_apply {
                let request = ApplyRequest {
                    tokens: derive_tokens(&config),
                    config,
                    paths: self.state.paths.clone(),
                    best_effort: true,
                };
                self.adapters.apply_all(&request)?;
                triggered_apply = true;
                apply_count += 1;
                pending_since = None;
            } else if should_apply {
                pending_since = None;
            }
        }

        Ok(WatchReport {
            iterations,
            triggered_apply,
            apply_count,
        })
    }
}

fn watch_signature(state: &MuksState, config: &muks_core::AppConfig) -> Result<SystemTime> {
    let mut latest = file_mtime(&state.paths.config_file)?;
    let wallpaper_path = std::path::PathBuf::from(&config.wallpaper.current);

    if config.wallpaper.source_type == "file" && wallpaper_path.exists() {
        latest = latest.max(file_mtime(&wallpaper_path)?);
    }

    Ok(latest)
}

fn file_mtime(path: &std::path::Path) -> Result<SystemTime> {
    Ok(std::fs::metadata(path)?
        .modified()
        .unwrap_or(SystemTime::UNIX_EPOCH))
}
