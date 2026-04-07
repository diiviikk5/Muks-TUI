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
        let mut last_modified = config_mtime(&self.state)?;
        let mut triggered_apply = false;

        for _ in 0..iterations {
            thread::sleep(interval);
            let current = config_mtime(&self.state)?;
            if current > last_modified {
                last_modified = current;
                let config = self.state.config()?;
                let request = ApplyRequest {
                    tokens: derive_tokens(&config),
                    config,
                    paths: self.state.paths.clone(),
                    best_effort: true,
                };
                self.adapters.apply_all(&request)?;
                triggered_apply = true;
            }
        }

        Ok(WatchReport {
            iterations,
            triggered_apply,
        })
    }
}

fn config_mtime(state: &MuksState) -> Result<SystemTime> {
    Ok(std::fs::metadata(&state.paths.config_file)?
        .modified()
        .unwrap_or(SystemTime::UNIX_EPOCH))
}
