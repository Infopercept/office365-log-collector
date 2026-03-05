use std::sync::Arc;
use clap::Parser;
use chrono::{DateTime, Utc};
use crate::collector::Collector;
use crate::config::{Config, MAX_LOOKBACK_HOURS};
use crate::state::StateManager;
use log::{error, info, warn, LevelFilter};
use tokio::sync::Mutex;
use crate::data_structures::RunState;
// Interactive mode is disabled - not updated for multi-tenant
// use crate::interactive_mode::interactive;

mod collector;
mod api_connection;
mod data_structures;
mod config;
mod interfaces;
// Interactive mode disabled
// mod interactive_mode;
mod state;
mod recordtype_filter;
mod known_blobs_cache;

// Use jemalloc as the global allocator. Unlike glibc malloc, jemalloc actively
// returns freed pages to the OS, preventing the RSS ratchet effect where memory
// grows monotonically to the container limit and triggers OOMKill.
#[cfg(not(target_env = "msvc"))]
use tikv_jemallocator::Jemalloc;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

// Configure jemalloc to immediately return freed pages to the OS.
// dirty_decay_ms:0  — purge dirty pages instantly (no 10s default hold)
// muzzy_decay_ms:0  — purge muzzy pages instantly (no 10s default hold)
// This prevents RSS from growing monotonically in long-running daemon mode.
#[cfg(not(target_env = "msvc"))]
#[allow(non_upper_case_globals)]
#[export_name = "_rjem_malloc_conf"]
pub static malloc_conf: &[u8] = b"dirty_decay_ms:0,muzzy_decay_ms:0\0";

#[tokio::main]
async fn main() {

    let args = data_structures::CliArgs::parse();
    let config = Config::new(args.config.clone());

    if args.interactive {
        error!("Interactive mode is not supported in the multi-tenant version.");
        error!("Interactive mode has not been updated for multi-tenant architecture and will fail.");
        error!("Please use daemon mode instead: run without --interactive flag.");
        error!("See KNOWN-ISSUES.md for details.");
        std::process::exit(1);
    } else {
        init_non_interactive_logging(&config);

        // Check if collector is enabled
        if !config.is_enabled() {
            info!("Office365 collector is disabled in config. Exiting.");
            return;
        }

        // Daemon mode support
        let interval_seconds = config.get_interval_seconds();
        let daemon_mode = config.interval.is_some();

        if daemon_mode {
            info!("Starting Office365 collector in daemon mode with interval: {}s", interval_seconds);
            loop {
                run_collection_for_all_tenants(args.clone(), config.clone()).await;

                // Force jemalloc to return freed pages to the OS between cycles.
                // Without this, jemalloc retains pages in dirty page lists, causing
                // RSS to grow monotonically even when Rust has dropped all allocations.
                #[cfg(not(target_env = "msvc"))]
                log_jemalloc_stats();

                info!("Sleeping for {} seconds until next collection...", interval_seconds);
                tokio::time::sleep(tokio::time::Duration::from_secs(interval_seconds)).await;
            }
        } else {
            info!("Starting Office365 collector in single-run mode");
            run_collection_for_all_tenants(args, config).await;
        }
    }
}

/// Log jemalloc memory stats between daemon cycles.
/// Actual page purging is handled by dirty_decay_ms:0 / muzzy_decay_ms:0
/// set via _rjem_malloc_conf at init time.
#[cfg(not(target_env = "msvc"))]
fn log_jemalloc_stats() {
    use tikv_jemalloc_ctl::{epoch, stats};

    if let Err(e) = epoch::advance() {
        warn!("jemalloc epoch::advance failed: {}", e);
        return;
    }

    let allocated = stats::allocated::read().unwrap_or(0);
    let resident = stats::resident::read().unwrap_or(0);
    let active = stats::active::read().unwrap_or(0);

    info!(
        "jemalloc stats: allocated={:.1}MB, active={:.1}MB, resident={:.1}MB",
        allocated as f64 / 1048576.0,
        active as f64 / 1048576.0,
        resident as f64 / 1048576.0,
    );
}

async fn run_collection_for_all_tenants(args: data_structures::CliArgs, config: Config) {
    if config.tenants.is_empty() {
        error!("No tenants configured. Please add at least one tenant to the config.");
        return;
    }

    info!("Running collection for {} tenant(s)", config.tenants.len());

    // Run collectors for all tenants concurrently
    let mut handles = vec![];

    for tenant in config.tenants.clone() {
        let args_clone = args.clone();
        let config_clone = config.clone();
        let tenant_clone = tenant.clone();

        let handle = tokio::spawn(async move {
            // Determine start time based on only_future_events and state
            let start_from = get_start_time_from_state(&config_clone, &tenant_clone.tenant_id);

            let state = RunState::default();
            let wrapped_state = Arc::new(Mutex::new(state));
            let runs = config_clone.get_needed_runs_from(start_from);

            match Collector::new(args_clone, config_clone, tenant_clone.clone(), runs, wrapped_state.clone(), None).await {
                Ok(mut collector) => {
                    info!("Started collector for tenant: {}", tenant_clone.tenant_id);
                    collector.monitor().await;
                    info!("Completed collection for tenant: {}", tenant_clone.tenant_id);
                },
                Err(e) => {
                    error!("Could not start collector for tenant {}: {}", tenant_clone.tenant_id, e);
                }
            }
        });

        handles.push(handle);
    }

    // Wait for all tenant collectors to complete
    for handle in handles {
        if let Err(e) = handle.await {
            error!("Tenant collector task failed: {}", e);
        }
    }

    info!("All tenant collections completed");
}

fn get_start_time_from_state(config: &Config, tenant_id: &str) -> Option<DateTime<Utc>> {
    if !config.only_future_events.unwrap_or(false) {
        return None;
    }

    let working_dir = config.get_working_dir();
    let state_manager = StateManager::new(&working_dir);

    let subscriptions = config.get_subscriptions();
    if let Some(first_sub) = subscriptions.first() {
        if let Some(state) = state_manager.load_state(tenant_id, first_sub) {
            let now = Utc::now();
            let hours_since_last_run = (now - state.last_log_time).num_hours();

            if hours_since_last_run > MAX_LOOKBACK_HOURS {
                warn!(
                    "State for tenant {} is stale: last_log_time is {} ({} hours ago). \
                     Microsoft only retains audit logs for 7 days (~168 hours). \
                     Collection will be capped to {} hours lookback.",
                    tenant_id, state.last_log_time, hours_since_last_run, MAX_LOOKBACK_HOURS
                );
            } else {
                info!("Using last_log_time {} as start time for tenant {} (only_future_events=true, {} hours ago)",
                    state.last_log_time, tenant_id, hours_since_last_run);
            }

            return Some(state.last_log_time);
        } else {
            let now = Utc::now();
            let start_time = now - chrono::Duration::try_seconds(1).unwrap();
            info!("First run for tenant {} with only_future_events=true: starting from {} (1 sec ago)",
                tenant_id, start_time);

            for subscription in &subscriptions {
                let state = crate::state::TenantSubscriptionState {
                    last_log_time: start_time,
                    last_run: now,
                    first_run: true,
                };
                if let Err(e) = state_manager.save_state(tenant_id, subscription, &state) {
                    error!("Failed to initialize state for {}/{}: {}", tenant_id, subscription, e);
                } else {
                    info!("Initialized state for {}/{}", tenant_id, subscription);
                }
            }

            return Some(start_time);
        }
    }

    None
}

fn init_non_interactive_logging(config: &Config) {

    let (path, level) = if let Some(log_config) = &config.log {
        let level = if log_config.debug { LevelFilter::Debug } else { LevelFilter::Info };
        (log_config.path.clone(), level)
    } else {
        ("".to_string(), LevelFilter::Info)
    };

    if !path.is_empty() {
        simple_logging::log_to_file(path, level).unwrap();
    } else {
        simple_logging::log_to_stderr(level);
    }
}
