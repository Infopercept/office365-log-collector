use std::collections::HashMap;
use std::ops::Div;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use anyhow::Result;
use log::{warn, error, info};
use futures::SinkExt;
use futures::channel::mpsc::channel;
use futures::channel::mpsc::{Sender, Receiver};
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::Mutex;
use tokio::time::sleep;
use crate::data_structures;
use crate::api_connection;
use crate::api_connection::ApiConnection;
use crate::config::{Config, ContentTypesSubConfig};
use crate::data_structures::{ArbitraryJson, CliArgs, ContentToRetrieve, FileWriter, RunState};
use crate::state::StateManager;
use crate::known_blobs_cache::{KnownBlobsCache, SharedKnownBlobsCache};


/// # Office Audit Log Collector
///
/// MEMORY FIX: The collector no longer buffers response bodies in channels or caches.
/// Download tasks process responses inline: parse from bytes → filter → write to file.
/// The monitor loop only receives log counts and updates known_blobs for dedup.
///
/// TASK LIFECYCLE FIX: Spawned background tasks are tracked and aborted on cleanup.
/// Without this, the blob collector task hangs forever (self-referential channel)
/// and leaks ~42MB per cycle.
pub struct Collector {
    config: Config,
    tenant_id: String,
    result_rx: Receiver<(usize, ContentToRetrieve)>,
    stats_rx: Receiver<(usize, usize, usize, usize)>,
    kill_tx: tokio::sync::mpsc::Sender<bool>,
    known_blobs: SharedKnownBlobsCache,
    saved: usize,
    file_writer: Arc<FileWriter>,
    /// Handles to spawned background tasks. Must be aborted on cleanup to prevent leaks.
    task_handles: Vec<tokio::task::JoinHandle<()>>,
}

impl Collector {

    pub async fn new(args: CliArgs,
                     config: Config,
                     tenant: crate::config::TenantConfig,
                     runs: HashMap<String, Vec<(String, String)>>,
                     state: Arc<Mutex<RunState>>,
                     _interactive_sender: Option<UnboundedSender<Vec<String>>>
    ) -> Result<Collector> {

        info!("Initializing collector for tenant {}.", tenant.tenant_id);

        // Initialize collector threads
        let tenant_id = tenant.tenant_id.clone();
        let api = api_connection::get_api_connection(args.clone(), config.clone(), tenant).await?;
        api.subscribe_to_feeds().await?;

        // Load known blobs using memory-efficient LRU cache
        let working_dir = config.get_working_dir();
        let known_blobs_path = Path::new(&working_dir).join("known_blobs");
        let known_blobs_cache = KnownBlobsCache::load_from_file(&known_blobs_path);
        info!("Loaded {} known blobs into LRU cache", known_blobs_cache.len());
        let known_blobs = SharedKnownBlobsCache::from_cache(known_blobs_cache);

        // Get content types/subscriptions
        let content_types_config = if let Some(ref collect) = config.collect {
            collect.content_types
        } else {
            let subs = config.get_subscriptions();
            ContentTypesSubConfig {
                general: Some(subs.contains(&"Audit.General".to_string())),
                azure_active_directory: Some(subs.contains(&"Audit.AzureActiveDirectory".to_string())),
                exchange: Some(subs.contains(&"Audit.Exchange".to_string())),
                share_point: Some(subs.contains(&"Audit.SharePoint".to_string())),
                dlp: Some(subs.contains(&"DLP.All".to_string())),
            }
        };

        // Create the shared FileWriter for direct-to-disk writing
        let file_writer = if let Some(ref file_config) = config.output.file {
            if file_config.separate_by_content_type.unwrap_or(false) {
                let paths = FileWriter::build_separated_paths(
                    &file_config.path,
                    &config.get_subscriptions(),
                );
                Arc::new(FileWriter::new_separated(paths))
            } else {
                Arc::new(FileWriter::new_unified(&file_config.path))
            }
        } else {
            Arc::new(FileWriter::new_noop())
        };

        // Build filters for inline processing in download tasks
        let filters = if let Some(ref collect) = config.collect {
            if let Some(ref filter_config) = collect.filter {
                filter_config.get_filters()
            } else {
                HashMap::new()
            }
        } else {
            HashMap::new()
        };

        let (result_rx, stats_rx, kill_tx, task_handles) =
            get_available_content(api,
                                  content_types_config,
                                  runs.clone(),
                                  &config,
                                  known_blobs.clone(),
                                  state,
                                  file_writer.clone(),
                                  filters).await;

        let collector = Collector {
            config,
            tenant_id,
            result_rx,
            stats_rx,
            known_blobs,
            saved: 0,
            kill_tx,
            file_writer,
            task_handles,
        };
        Ok(collector)
    }

    /// Monitor all started content retrieval threads.
    /// MEMORY FIX: No longer processes JSON data — only receives log counts.
    pub async fn monitor(&mut self) {

        let start = Instant::now();
        const DEFAULT_TIMEOUT_MINUTES: usize = 30;

        loop {
            let timeout_minutes = if let Some(ref collect) = self.config.collect {
                collect.global_timeout.unwrap_or(DEFAULT_TIMEOUT_MINUTES)
            } else {
                DEFAULT_TIMEOUT_MINUTES
            };

            let elapsed_minutes = start.elapsed().as_secs().div(60) as usize;
            if timeout_minutes > 0 && elapsed_minutes >= timeout_minutes {
                warn!(
                    "Global timeout expired after {} minutes. Requesting collector stop.",
                    elapsed_minutes
                );
                let _ = self.kill_tx.send(true).await;
                sleep(Duration::from_secs(2)).await;
                break;
            }

            if self.check_stats().await {
                break
            }

            self.check_results().await;

            sleep(Duration::from_millis(10)).await;
        }
        self.check_all_results().await;
        self.end_run().await;
    }

    pub async fn end_run(&mut self) {
        // Flush all file writers to ensure all data is on disk
        self.file_writer.flush_all();

        // Save known blobs
        let working_dir = self.config.get_working_dir();
        let known_blobs_path = Path::new(&working_dir).join("known_blobs");
        if let Err(e) = self.known_blobs.save_to_file(&known_blobs_path).await {
            error!("Failed to save known blobs: {}", e);
        } else {
            info!("Saved {} known blobs to file", self.known_blobs.len().await);
        }

        // Update state with current time for only_future_events
        if self.config.only_future_events.unwrap_or(false) {
            let working_dir = self.config.get_working_dir();
            let state_manager = StateManager::new(&working_dir);
            let now = chrono::Utc::now();

            for subscription in self.config.get_subscriptions() {
                let state = crate::state::TenantSubscriptionState {
                    last_log_time: now,
                    last_run: now,
                    first_run: false,
                };
                if let Err(e) = state_manager.save_state(&self.tenant_id, &subscription, &state) {
                    error!("Failed to update state for {}/{}: {}", self.tenant_id, subscription, e);
                } else {
                    info!("Updated state for {}/{}: last_log_time={}", self.tenant_id, subscription, now);
                }
            }
        }

        // CRITICAL: Abort AND await background tasks to prevent memory leaks.
        // The blob collector task has a self-referential channel (blobs_tx/blobs_rx)
        // and will hang forever if not explicitly aborted. We must AWAIT each handle
        // after aborting to guarantee tokio fully drops the task state machine
        // (async future + all captured data). Without await, tokio may defer cleanup
        // and the ~42MB of task state per cycle accumulates indefinitely.
        for handle in self.task_handles.drain(..) {
            handle.abort();
            let _ = handle.await; // Wait for tokio to fully drop task state
        }
    }

    /// MEMORY FIX: Now receives (usize, ContentToRetrieve) — a count, not data.
    pub async fn check_results(&mut self) -> usize {
        if let Ok(Some((count, content))) = self.result_rx.try_next() {
            self.handle_content(count, content).await
        } else {
            0
        }
    }

    pub async fn check_all_results(&mut self) -> usize {
        let mut amount = 0;
        while let Ok(Some((count, content))) = self.result_rx.try_next() {
            amount += self.handle_content(count, content).await;
        }
        amount
    }

    /// MEMORY FIX: No JSON parsing here. Just update known_blobs for dedup and track count.
    async fn handle_content(&mut self, count: usize, content: ContentToRetrieve) -> usize {
        self.known_blobs.insert(content.content_id.clone(), &content.expiration).await;
        self.saved += count;
        count
    }

    pub async fn check_stats(&mut self) -> bool {
        if let Ok(Some((found,
                        successful,
                        retried,
                        failed))) = self.stats_rx.try_next() {

            // Flush file writer to ensure all data is on disk before reporting stats
            self.file_writer.flush_all();

            let output = self.get_output_string(
                found,
                successful,
                failed,
                retried,
                self.saved,
            );
            info!("{}", output);
            true
        } else {
            false
        }
    }

    fn get_output_string(&self, found: usize, successful: usize, failed: usize, retried: usize,
                         saved: usize) -> String {
        format!("\
Done!||
Blobs found: {}||
Blobs successful: {}||
Blobs failed: {}||
Blobs retried: {}||
Logs saved: {}",
            found, successful, failed, retried, saved
        )
    }

}


/// Initialize channels for inter-task communication.
///
/// MEMORY FIX: result channel now carries (usize, ContentToRetrieve) not (String, ContentToRetrieve).
/// FileWriter and filters are passed through to GetContentConfig for inline processing.
fn initialize_channels(
    api: ApiConnection, content_types: ContentTypesSubConfig,
    runs: HashMap<String, Vec<(String, String)>>, config: &Config,
    file_writer: Arc<FileWriter>,
    filters: HashMap<String, ArbitraryJson>)
    -> (data_structures::GetBlobConfig,
        data_structures::GetContentConfig,
        data_structures::MessageLoopConfig,
        Receiver<(String, String)>,
        Receiver<ContentToRetrieve>,
        Receiver<(usize, ContentToRetrieve)>,
        Receiver<(usize, usize, usize, usize)>,
        tokio::sync::mpsc::Sender<bool>) {

    let urls = api.create_base_urls(runs);

    let (status_tx, status_rx):
        (Sender<data_structures::StatusMessage>,
         Receiver<data_structures::StatusMessage>) = channel(2000);

    let (blobs_tx, blobs_rx):
        (Sender<(String, String)>,
         Receiver<(String, String)>) = channel(2000);

    let (blob_error_tx, blob_error_rx):
        (Sender<(String, String)>,
         Receiver<(String, String)>) = channel(2000);

    let (content_tx, content_rx):
        (Sender<ContentToRetrieve>,
         Receiver<ContentToRetrieve>) = channel(2000);

    let (content_error_tx, content_error_rx):
        (Sender<ContentToRetrieve>,
         Receiver<ContentToRetrieve>) = channel(2000);

    // MEMORY FIX: Channel now carries (count, metadata) not (full_response_body, metadata).
    // Capacity 500 is generous — each item is ~200 bytes (usize + ContentToRetrieve).
    let (result_tx, result_rx):
        (Sender<(usize, ContentToRetrieve)>,
         Receiver<(usize, ContentToRetrieve)>) = channel(500);

    let (stats_tx, stats_rx):
        (Sender<(usize, usize, usize, usize)>,
         Receiver<(usize, usize, usize, usize)>) = channel(100);

    let (kill_tx, kill_rx): (tokio::sync::mpsc::Sender<bool>,
                             tokio::sync::mpsc::Receiver<bool>) = tokio::sync::mpsc::channel(10);

    let max_threads = config.collect.as_ref()
        .and_then(|c| c.max_threads)
        .unwrap_or(10);
    let duplicate = config.collect.as_ref()
        .and_then(|c| c.duplicate)
        .unwrap_or(1);
    let retries = config.collect.as_ref()
        .and_then(|c| c.retries)
        .unwrap_or(3);

    let client = reqwest::Client::new();

    let blob_config = data_structures::GetBlobConfig {
        client: client.clone(),
        headers: api.headers.clone(),
        status_tx: status_tx.clone(), blobs_tx: blobs_tx.clone(),
        blob_error_tx: blob_error_tx.clone(), content_tx: content_tx.clone(),
        threads: max_threads,
        duplicate,
    };

    let content_config = data_structures::GetContentConfig {
        client: client.clone(),
        headers: api.headers.clone(),
        result_tx: result_tx.clone(),
        content_error_tx: content_error_tx.clone(),
        status_tx: status_tx.clone(),
        threads: max_threads,
        max_response_size: config.get_max_size_bytes(),
        file_writer,
        filters,
    };

    let message_loop_config = data_structures::MessageLoopConfig {
        content_tx: content_tx.clone(),
        blobs_tx: blobs_tx.clone(),
        stats_tx: stats_tx.clone(),
        urls,
        content_error_rx,
        status_rx,
        blob_error_rx,
        content_types,
        retries,
        kill_rx,
    };
    (blob_config, content_config, message_loop_config, blobs_rx, content_rx, result_rx,
            stats_rx, kill_tx)
}


/// Get all the available log content.
///
/// MEMORY FIX: Accepts FileWriter and filters to pass through to content download tasks.
/// TASK LIFECYCLE FIX: Returns task handles so they can be aborted on cleanup.
async fn get_available_content(api: ApiConnection,
                         content_types: ContentTypesSubConfig,
                         runs: HashMap<String, Vec<(String, String)>>,
                         config: &Config,
                         known_blobs: SharedKnownBlobsCache,
                         state: Arc<Mutex<RunState>>,
                         file_writer: Arc<FileWriter>,
                         filters: HashMap<String, ArbitraryJson>)
                         -> (Receiver<(usize, ContentToRetrieve)>,
                             Receiver<(usize, usize, usize, usize)>,
                             tokio::sync::mpsc::Sender<bool>,
                             Vec<tokio::task::JoinHandle<()>>) {

    let (blob_config,
        content_config,
        message_loop_config,
        blobs_rx,
        content_rx,
        result_rx,
        stats_rx,
        kill_tx) = initialize_channels(api, content_types, runs, config, file_writer, filters);

    let task_handles = spawn_blob_collector(blob_config,
                         content_config,
                         message_loop_config,
                         blobs_rx,
                         content_rx,
                         known_blobs,
                         state);

    (result_rx, stats_rx, kill_tx, task_handles)
}


/// Spawn async tasks for collectors on the existing Tokio runtime.
/// Returns JoinHandles so tasks can be aborted on cleanup (prevents 42MB/cycle leak).
fn spawn_blob_collector(
    blob_config: data_structures::GetBlobConfig,
    content_config: data_structures::GetContentConfig,
    message_loop_config: data_structures::MessageLoopConfig,
    blobs_rx: Receiver<(String, String)>,
    content_rx: Receiver<ContentToRetrieve>,
    known_blobs: SharedKnownBlobsCache,
    state: Arc<Mutex<RunState>>) -> Vec<tokio::task::JoinHandle<()>> {

    info!("Spawning collector tasks on shared runtime");

    let h1 = tokio::spawn(async move {
        api_connection::get_content_blobs_async(blob_config, blobs_rx, known_blobs).await;
    });

    let h2 = tokio::spawn(async move {
        api_connection::get_content_async(content_config, content_rx).await;
    });

    let h3 = tokio::spawn(async move {
        message_loop(message_loop_config, state).await;
    });

    vec![h1, h2, h3]
}


/// Message loop: track progress and terminate when all content is retrieved.
pub async fn message_loop(mut config: data_structures::MessageLoopConfig,
                          mut state: Arc<Mutex<RunState>>) {

    for (content_type, base_url) in config.urls.into_iter() {
        config.blobs_tx.clone().send((content_type, base_url)).await.unwrap();
        state.lock().await.awaiting_content_types += 1;
    }

    let mut rate_limit_backoff_started: Option<Instant> = None;

    const MAX_RETRY_ENTRIES: usize = 50_000;
    let mut retry_map: lru::LruCache<String, usize> =
        lru::LruCache::new(std::num::NonZeroUsize::new(MAX_RETRY_ENTRIES).unwrap());

    loop {

        if let Some(t) = rate_limit_backoff_started {
            if t.elapsed().as_secs() >= 30 {
                rate_limit_backoff_started = None;
                state.lock().await.rate_limited = false;
                info!("Release rate limit");
            }
        }

        if let Ok(msg) = config.kill_rx.try_recv() {
            if msg {
                info!("Stopping collector.");
                break
            }
        }

        if let Ok(Some(msg)) = config.status_rx.try_next() {
            match msg {
                data_structures::StatusMessage::FoundNewContentBlob => {
                    state.lock().await.awaiting_content_blobs +=1;
                    state.lock().await.stats.blobs_found += 1;
                },
                data_structures::StatusMessage::FinishedContentBlobs => {
                    let new_content_types = state.lock().await.awaiting_content_types.saturating_sub(1);
                    state.lock().await.awaiting_content_types = new_content_types;
                    if check_done(&mut state).await {
                        break
                    }
                },
                data_structures::StatusMessage::RetrievedContentBlob => {
                    state.lock().await.awaiting_content_blobs -= 1;
                    state.lock().await.stats.blobs_successful += 1;
                    if check_done(&mut state).await {
                        config.content_tx.close_channel();
                        break;
                    }
                },
                data_structures::StatusMessage::ErrorContentBlob => {
                    state.lock().await.awaiting_content_blobs -= 1;
                    state.lock().await.stats.blobs_error += 1;
                    if check_done(&mut state).await {
                        config.content_tx.close_channel();
                        break;
                    }
                }
                data_structures::StatusMessage::BeingThrottled => {
                    if rate_limit_backoff_started.is_none() {
                        warn!("Being rate limited, backing off 30 seconds.");
                        state.lock().await.rate_limited = true;
                        rate_limit_backoff_started = Some(Instant::now());
                    }
                }
            }
        }

        if let Ok(Some((content_type, url))) = config.blob_error_rx.try_next() {
            if let Some(retries_left) = retry_map.get_mut(&url) {
                if *retries_left == 0 {
                    error!("Gave up on blob {}", url);
                    retry_map.pop(&url);
                    state.lock().await.awaiting_content_types -= 1;
                    state.lock().await.stats.blobs_error += 1;
                    if check_done(&mut state).await {
                        break;
                    }
                } else {
                    if rate_limit_backoff_started.is_none() {
                        *retries_left -= 1;
                    }
                    let retries = *retries_left;
                    state.lock().await.stats.blobs_retried += 1;
                    warn!("Retry blob {} {}", retries, url);
                    config.blobs_tx.send((content_type, url)).await.unwrap();
                }
            } else {
                retry_map.put(url.clone(), config.retries - 1);
                state.lock().await.stats.blobs_retried += 1;
                warn!("Retry blob {} {}", config.retries - 1, url);
                config.blobs_tx.send((content_type, url)).await.unwrap();
            }
        };

        if let Ok(Some(content)) = config.content_error_rx.try_next() {
            state.lock().await.stats.blobs_retried += 1;
            if let Some(retries_left) = retry_map.get_mut(&content.url) {
                if *retries_left == 0 {
                    error!("Gave up on content {}", content.url);
                    retry_map.pop(&content.url);
                    state.lock().await.awaiting_content_blobs -= 1;
                    state.lock().await.stats.blobs_error += 1;
                    if check_done(&mut state).await {
                        config.content_tx.close_channel();
                        break;
                    }
                } else {
                    if rate_limit_backoff_started.is_none() {
                        *retries_left -= 1;
                    }
                    let retries = *retries_left;
                    warn!("Retry content {} {}", retries, content.url);
                    config.content_tx.send(content).await.unwrap();
                }
            } else {
                retry_map.put(content.url.to_string(), config.retries - 1);
                state.lock().await.stats.blobs_retried += 1;
                warn!("Retry content {} {}", config.retries - 1, content.url);
                config.content_tx.send(content).await.unwrap();
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }

    let stats = state.lock().await.stats.clone();
    sleep(Duration::from_secs(3)).await;
    config.stats_tx.send((
        stats.blobs_found,
        stats.blobs_successful,
        stats.blobs_retried,
        stats.blobs_error)).await.unwrap();
}

async fn check_done(state: &mut Arc<Mutex<RunState>>) -> bool {
    let types = state.lock().await.awaiting_content_types;
    let blobs = state.lock().await.awaiting_content_blobs;
    types == 0 && blobs == 0
}
