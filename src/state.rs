// State Management for Office365 Collector
// Tracks last_log_time per tenant+subscription for precise resumption

use std::fs;
use std::path::PathBuf;
use chrono::{DateTime, Utc};
use serde_derive::{Deserialize, Serialize};
use log::{debug, error, info};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantSubscriptionState {
    pub last_log_time: DateTime<Utc>,
    pub last_run: DateTime<Utc>,
    pub first_run: bool,
}

impl TenantSubscriptionState {
    pub fn new() -> Self {
        Self {
            last_log_time: Utc::now(),
            last_run: Utc::now(),
            first_run: true,
        }
    }
}

pub struct StateManager {
    working_dir: PathBuf,
}

impl StateManager {
    pub fn new(working_dir: &str) -> Self {
        let dir = PathBuf::from(working_dir);

        // Create state directory if it doesn't exist
        if let Err(e) = fs::create_dir_all(&dir) {
            error!("Failed to create state directory {}: {}", dir.display(), e);
        }

        Self {
            working_dir: dir,
        }
    }

    /// Get state file path for a specific tenant+subscription
    fn get_state_file_path(&self, tenant_id: &str, subscription: &str) -> PathBuf {
        let filename = format!("office365-{}-{}.json",
            sanitize_filename(tenant_id),
            sanitize_filename(subscription)
        );
        self.working_dir.join(filename)
    }

    /// Load state for a tenant+subscription
    pub fn load_state(&self, tenant_id: &str, subscription: &str) -> Option<TenantSubscriptionState> {
        let path = self.get_state_file_path(tenant_id, subscription);

        if !path.exists() {
            debug!("No state file found for {}/{}", tenant_id, subscription);
            return None;
        }

        match fs::read_to_string(&path) {
            Ok(content) => {
                match serde_json::from_str::<TenantSubscriptionState>(&content) {
                    Ok(state) => {
                        debug!("Loaded state for {}/{}: last_log_time={}",
                            tenant_id, subscription, state.last_log_time);
                        Some(state)
                    }
                    Err(e) => {
                        error!("Failed to parse state file {}: {}", path.display(), e);
                        None
                    }
                }
            }
            Err(e) => {
                error!("Failed to read state file {}: {}", path.display(), e);
                None
            }
        }
    }

    /// Save state for a tenant+subscription
    pub fn save_state(&self, tenant_id: &str, subscription: &str, state: &TenantSubscriptionState) -> Result<(), String> {
        let path = self.get_state_file_path(tenant_id, subscription);

        match serde_json::to_string_pretty(state) {
            Ok(content) => {
                match fs::write(&path, content) {
                    Ok(_) => {
                        debug!("Saved state for {}/{}: last_log_time={}",
                            tenant_id, subscription, state.last_log_time);
                        Ok(())
                    }
                    Err(e) => {
                        error!("Failed to write state file {}: {}", path.display(), e);
                        Err(format!("Failed to write state: {}", e))
                    }
                }
            }
            Err(e) => {
                error!("Failed to serialize state: {}", e);
                Err(format!("Failed to serialize state: {}", e))
            }
        }
    }

    /// Initialize or update state for first run with only_future_events
    pub fn initialize_state(&self, tenant_id: &str, subscription: &str, only_future_events: bool) -> TenantSubscriptionState {
        // Try to load existing state
        if let Some(mut state) = self.load_state(tenant_id, subscription) {
            // State exists, mark as not first run
            state.first_run = false;
            return state;
        }

        // No state exists - this is first run
        let now = Utc::now();
        let state = TenantSubscriptionState {
            last_log_time: now,  // Set to NOW for only_future_events
            last_run: now,
            first_run: true,
        };

        // Save initial state
        if let Err(e) = self.save_state(tenant_id, subscription, &state) {
            error!("Failed to save initial state: {}", e);
        } else if only_future_events {
            info!("First run with only_future_events: Set bookmark to NOW for {}/{}",
                tenant_id, subscription);
        }

        state
    }

    /// Check if this is the first run for a tenant+subscription
    pub fn is_first_run(&self, tenant_id: &str, subscription: &str) -> bool {
        self.load_state(tenant_id, subscription).is_none()
    }
}

/// Sanitize filename to remove invalid characters
fn sanitize_filename(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_state_save_load() {
        let dir = tempdir().unwrap();
        let manager = StateManager::new(dir.path().to_str().unwrap());

        let state = TenantSubscriptionState::new();
        manager.save_state("tenant1", "Audit.General", &state).unwrap();

        let loaded = manager.load_state("tenant1", "Audit.General");
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().first_run, true);
    }

    #[test]
    fn test_first_run_detection() {
        let dir = tempdir().unwrap();
        let manager = StateManager::new(dir.path().to_str().unwrap());

        assert_eq!(manager.is_first_run("tenant1", "Audit.General"), true);

        let state = TenantSubscriptionState::new();
        manager.save_state("tenant1", "Audit.General", &state).unwrap();

        assert_eq!(manager.is_first_run("tenant1", "Audit.General"), false);
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("tenant/id:123"), "tenant_id_123");
        assert_eq!(sanitize_filename("normal-tenant-id"), "normal-tenant-id");
    }
}
