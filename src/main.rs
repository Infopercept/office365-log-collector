use std::sync::Arc;
use clap::Parser;
use chrono::{DateTime, Utc};
use crate::collector::Collector;
use crate::config::Config;
use crate::state::StateManager;
use log::{error, info, LevelFilter};
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
                info!("Sleeping for {} seconds until next collection...", interval_seconds);
                tokio::time::sleep(tokio::time::Duration::from_secs(interval_seconds)).await;
            }
        } else {
            info!("Starting Office365 collector in single-run mode");
            run_collection_for_all_tenants(args, config).await;
        }
    }
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

/// Get start time from state if only_future_events is enabled
/// Returns Some(time) to start collection from that time
/// Returns None to use default (last 24 hours)
fn get_start_time_from_state(config: &Config, tenant_id: &str) -> Option<DateTime<Utc>> {
    // Only use state-based start time if only_future_events is enabled
    if !config.only_future_events.unwrap_or(false) {
        return None;
    }

    let working_dir = config.get_working_dir();
    let state_manager = StateManager::new(&working_dir);

    // Check any subscription's state to get last_log_time
    let subscriptions = config.get_subscriptions();
    if let Some(first_sub) = subscriptions.first() {
        if let Some(state) = state_manager.load_state(tenant_id, first_sub) {
            info!("Using last_log_time {} as start time for tenant {} (only_future_events=true)",
                state.last_log_time, tenant_id);
            return Some(state.last_log_time);
        } else {
            // First run - initialize state and start collection from 1 second ago
            // (API requires startTime < endTime, so we use NOW - 1 second)
            let now = Utc::now();
            let start_time = now - chrono::Duration::try_seconds(1).unwrap();
            info!("First run for tenant {} with only_future_events=true: starting from {} (1 sec ago)",
                tenant_id, start_time);

            // Initialize state for all subscriptions
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

// Interactive mode logging disabled - not needed for production daemon mode
/*
fn init_interactive_logging(config: &Config, log_tx: UnboundedSender<(String, Level)>) {
    let level = if let Some(log_config) = &config.log {
        if log_config.debug { LevelFilter::Debug } else { LevelFilter::Info }
    } else {
        LevelFilter::Info
    };
    log::set_max_level(level);
    log::set_boxed_logger(InteractiveLogger::new(log_tx)).unwrap();
}

pub struct  InteractiveLogger {
    log_tx: UnboundedSender<(String, Level)>,
}
impl InteractiveLogger {
    pub fn new(log_tx: UnboundedSender<(String, Level)>) -> Box<Self> {
        Box::new(InteractiveLogger { log_tx })
    }
}
impl Log for InteractiveLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Info
    }
    fn log(&self, record: &Record) {
        let date = chrono::Utc::now().to_string();
        let msg = format!("[{}] {}:{} -- {}",
                 date,
                 record.level(),
                 record.target(),
                 record.args());
        self.log_tx.send((msg, record.level())).unwrap()
    }
    fn flush(&self) {}
}
*/

