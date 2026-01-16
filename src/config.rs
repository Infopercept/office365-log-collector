use std::collections::HashMap;
use std::ffi::OsString;
use std::fs::File;
use std::io::{BufReader, LineWriter, Read, Write};
use std::path::Path;
use chrono::{DateTime, NaiveDateTime, Utc};
use serde_derive::Deserialize;
use crate::data_structures::ArbitraryJson;


#[derive(Deserialize, Clone, Debug)]
pub struct Config {
    pub enabled: Option<bool>,
    pub interval: Option<String>,  // e.g., "5m", "1h", "30s"
    pub curl_max_size: Option<String>,  // e.g., "1M", "500K", "2G"
    pub only_future_events: Option<bool>,
    #[serde(rename = "workingDir")]
    pub working_dir: Option<String>,  // Directory for state files and known_blobs
    pub log: Option<LogSubConfig>,
    #[serde(default)]
    pub tenants: Vec<TenantConfig>,  // Default to empty vec for backward compatibility
    #[serde(default)]
    pub subscriptions: Vec<String>,  // Default to empty vec, Dynamic content types
    pub collect: Option<CollectSubConfig>,  // Now optional, using new structure
    pub output: OutputSubConfig
}
impl Config {

    pub fn new(path: String) -> Self {

        let open_file = File::open(path)
            .unwrap_or_else(|e| panic!("Config path could not be opened: {}", e.to_string()));
        let reader = BufReader::new(open_file);
        let config: Config = serde_yaml::from_reader(reader)
            .unwrap_or_else(|e| panic!("Config could not be parsed: {}", e.to_string()));
        config
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.unwrap_or(true)
    }

    pub fn get_interval_seconds(&self) -> u64 {
        if let Some(interval_str) = &self.interval {
            Self::parse_interval(interval_str)
        } else if let Some(ref collect) = self.collect {
            // Fallback to legacy hoursToCollect if present
            collect.hours_to_collect.unwrap_or(24) as u64 * 3600
        } else {
            300  // Default 5 minutes
        }
    }

    pub fn get_max_size_bytes(&self) -> Option<usize> {
        if let Some(size_str) = &self.curl_max_size {
            Some(Self::parse_size(size_str))
        } else {
            None
        }
    }

    fn parse_interval(s: &str) -> u64 {
        let s = s.trim();
        if s.ends_with('s') {
            s[..s.len()-1].parse().unwrap_or(300)
        } else if s.ends_with('m') {
            s[..s.len()-1].parse::<u64>().unwrap_or(5) * 60
        } else if s.ends_with('h') {
            s[..s.len()-1].parse::<u64>().unwrap_or(1) * 3600
        } else if s.ends_with('d') {
            s[..s.len()-1].parse::<u64>().unwrap_or(1) * 86400
        } else {
            s.parse().unwrap_or(300)  // Assume seconds if no unit
        }
    }

    fn parse_size(s: &str) -> usize {
        let s = s.trim().to_uppercase();
        if s.ends_with('K') {
            s[..s.len()-1].parse::<usize>().unwrap_or(1024) * 1024
        } else if s.ends_with('M') {
            s[..s.len()-1].parse::<usize>().unwrap_or(1) * 1024 * 1024
        } else if s.ends_with('G') {
            s[..s.len()-1].parse::<usize>().unwrap_or(1) * 1024 * 1024 * 1024
        } else {
            s.parse().unwrap_or(1024 * 1024)  // Assume bytes if no unit
        }
    }

    pub fn get_subscriptions(&self) -> Vec<String> {
        if !self.subscriptions.is_empty() {
            self.subscriptions.clone()
        } else if let Some(ref collect) = self.collect {
            // Fallback to legacy content_types
            collect.content_types.get_content_type_strings()
        } else {
            vec![]
        }
    }

    pub fn get_working_dir(&self) -> String {
        // Check top-level workingDir first, then fall back to collect.workingDir
        if let Some(ref dir) = self.working_dir {
            dir.clone()
        } else if let Some(ref collect) = self.collect {
            collect.working_dir.clone().unwrap_or_else(|| "./".to_string())
        } else {
            "./".to_string()
        }
    }

    pub fn get_needed_runs(&self) -> HashMap<String, Vec<(String, String)>> {
        self.get_needed_runs_from(None)
    }

    /// Get needed runs with optional start time override (for only_future_events)
    /// If start_from is provided, use it as the start time instead of NOW - hours_to_collect
    pub fn get_needed_runs_from(&self, start_from: Option<DateTime<Utc>>) -> HashMap<String, Vec<(String, String)>> {
        let mut runs: HashMap<String, Vec<(String, String)>> = HashMap::new();
        let end_time = chrono::Utc::now();

        let start_time_base = if let Some(from) = start_from {
            // Use provided start time (from state's last_log_time)
            from
        } else {
            // Fall back to hours_to_collect calculation
            let hours_to_collect = if let Some(ref collect) = self.collect {
                collect.hours_to_collect.unwrap_or(24)
            } else {
                24
            };

            if hours_to_collect > 168 {
                panic!("Hours to collect cannot be more than 168 due to Office API limits");
            }

            end_time - chrono::Duration::try_hours(hours_to_collect).unwrap()
        };

        let subscriptions = self.get_subscriptions();
        for content_type in subscriptions {
            runs.insert(content_type.clone(), vec!());
            let mut start_time = start_time_base;

            // Split into 24-hour chunks if needed (API limit)
            while end_time - start_time > chrono::Duration::try_hours(24).unwrap() {
                let split_end_time = start_time + chrono::Duration::try_hours(24).unwrap();
                let formatted_start_time = start_time.format("%Y-%m-%dT%H:%M:%SZ").to_string();
                let formatted_end_time = split_end_time.format("%Y-%m-%dT%H:%M:%SZ").to_string();
                runs.get_mut(&content_type).unwrap().push((formatted_start_time, formatted_end_time));
                start_time = split_end_time;
            }
            let formatted_start_time = start_time.format("%Y-%m-%dT%H:%M:%SZ").to_string();
            let formatted_end_time = end_time.format("%Y-%m-%dT%H:%M:%SZ").to_string();
            runs.get_mut(&content_type).unwrap().push((formatted_start_time, formatted_end_time));
        }
        runs
    }

    pub fn load_known_blobs(&self) -> HashMap<String, String> {
        let working_dir = self.get_working_dir();
        let file_name = Path::new("known_blobs");
        let mut path = Path::new(&working_dir).join(file_name);
        self.load_known_content(path.as_mut_os_string())
    }

    pub fn save_known_blobs(&mut self, known_blobs: &HashMap<String, String>) {
        let working_dir = self.get_working_dir();
        let mut known_blobs_path = Path::new(&working_dir).join(Path::new("known_blobs"));
        self.save_known_content(known_blobs, &known_blobs_path.as_mut_os_string())
    }

    fn load_known_content(&self, path: &OsString) -> HashMap<String, String> {

        let mut known_content = HashMap::new();
        if !Path::new(path).exists() {
            return known_content
        }

        // Load file
        let mut known_content_file = File::open(path).unwrap();
        let mut known_content_string = String::new();
        known_content_file.read_to_string(&mut known_content_string).unwrap();
        for line in known_content_string.lines() {
            if line.trim().is_empty() {
                continue
            }
            // Skip load expired content
            let now = Utc::now();
            if let Some((id, creation_time)) = line.split_once(',') {
                let is_valid = if let Ok(i) =
                    NaiveDateTime::parse_from_str(creation_time, "%Y-%m-%dT%H:%M:%S.%fZ") {
                    let time_utc = DateTime::<Utc>::from_naive_utc_and_offset(i, Utc);
                    now < time_utc  // Content is valid if current time is BEFORE expiration
                } else {
                    false  // Invalid timestamp = don't load
                };
                if is_valid {
                    known_content.insert(id.trim().to_string(), creation_time.trim().to_string());
                }
            }
        }
        known_content
    }

    fn save_known_content(&mut self, known_content: &HashMap<String, String>, path: &OsString) {

        let known_content_file = File::create(path).unwrap();
        let mut writer = LineWriter::new(known_content_file);

        for (id, creation_time) in known_content.iter() {
            writer.write_all(format!("{},{}\n", id, creation_time).as_bytes()).unwrap();
        }
        writer.flush().unwrap();
    }

}

#[derive(Deserialize, Clone, Debug)]
pub struct TenantConfig {
    pub tenant_id: String,
    pub client_id: String,
    pub client_secret: Option<String>,
    pub client_secret_path: Option<String>,
    pub api_type: Option<String>,  // commercial, gcc, gcc-high
}

impl TenantConfig {
    pub fn get_endpoints(&self) -> (String, String) {
        let api_type = self.api_type.as_deref().unwrap_or("commercial");
        match api_type {
            "commercial" => (
                "https://login.microsoftonline.com".to_string(),
                "https://manage.office.com".to_string()
            ),
            "gcc" => (
                "https://login.microsoftonline.com".to_string(),
                "https://manage-gcc.office.com".to_string()
            ),
            "gcc-high" => (
                "https://login.microsoftonline.us".to_string(),
                "https://manage.office365.us".to_string()
            ),
            _ => panic!("Invalid api_type: {}. Must be 'commercial', 'gcc', or 'gcc-high'", api_type)
        }
    }

    pub fn get_secret(&self) -> Result<String, String> {
        if let Some(secret) = &self.client_secret {
            return Ok(secret.clone());
        }

        if let Some(secret_path) = &self.client_secret_path {
            match std::fs::read_to_string(secret_path) {
                Ok(content) => Ok(content.trim().to_string()),
                Err(e) => Err(format!("Failed to read secret from {}: {}", secret_path, e))
            }
        } else {
            Err("Either client_secret or client_secret_path must be provided".to_string())
        }
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct LogSubConfig {
    pub path: String,
    pub debug: bool,
}

#[derive(Deserialize, Clone, Debug)]
pub struct CollectSubConfig {
    #[serde(rename = "workingDir")]
    pub working_dir: Option<String>,
    #[serde(rename = "cacheSize")]
    pub cache_size: Option<usize>,
    #[serde(rename = "contentTypes")]
    pub content_types: ContentTypesSubConfig,
    #[serde(rename = "maxThreads")]
    pub max_threads: Option<usize>,
    #[serde(rename = "globalTimeout")]
    pub global_timeout: Option<usize>,
    pub retries: Option<usize>,
    #[serde(rename = "hoursToCollect")]
    pub hours_to_collect: Option<i64>,
    #[serde(rename = "skipKnownLogs")]
    pub skip_known_logs: Option<bool>,
    pub filter: Option<FilterSubConfig>,
    pub duplicate: Option<usize>,
}
#[derive(Deserialize, Copy, Clone, Debug)]
pub struct ContentTypesSubConfig {
    #[serde(rename = "Audit.General")]
    pub general: Option<bool>,
    #[serde(rename = "Audit.AzureActiveDirectory")]
    pub azure_active_directory: Option<bool>,
    #[serde(rename = "Audit.Exchange")]
    pub exchange: Option<bool>,
    #[serde(rename = "Audit.SharePoint")]
    pub share_point: Option<bool>,
    #[serde(rename = "DLP.All")]
    pub dlp: Option<bool>,
}
impl ContentTypesSubConfig {
    pub fn get_content_type_strings(&self) -> Vec<String> {
        let mut results = Vec::new();
        if self.general.unwrap_or(false) {
            results.push("Audit.General".to_string())
        }
        if self.azure_active_directory.unwrap_or(false) {
            results.push("Audit.AzureActiveDirectory".to_string())
        }
        if self.exchange.unwrap_or(false) {
            results.push("Audit.Exchange".to_string())
        }
        if self.share_point.unwrap_or(false) {
            results.push("Audit.SharePoint".to_string())
        }
        if self.dlp.unwrap_or(false) {
            results.push("DLP.All".to_string())
        }
        results
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct FilterSubConfig {
    #[serde(rename = "Audit.General")]
    pub general: Option<ArbitraryJson>,
    #[serde(rename = "Audit.AzureActiveDirectory")]
    pub azure_active_directory: Option<ArbitraryJson>,
    #[serde(rename = "Audit.Exchange")]
    pub exchange: Option<ArbitraryJson>,
    #[serde(rename = "Audit.SharePoint")]
    pub share_point: Option<ArbitraryJson>,
    #[serde(rename = "DLP.All")]
    pub dlp: Option<ArbitraryJson>,
}
impl FilterSubConfig {
    pub fn get_filters(&self) -> HashMap<String, ArbitraryJson> {

        let mut results = HashMap::new();
        if let Some(filter) = self.general.as_ref() {
            results.insert("Audit.General".to_string(), filter.clone());
        }
        if let Some(filter) = self.azure_active_directory.as_ref() {
            results.insert("Audit.AzureActiveDirectory".to_string(), filter.clone());
        }
        if let Some(filter) = self.share_point.as_ref() {
            results.insert("Audit.SharePoint".to_string(), filter.clone());
        }
        if let Some(filter) = self.exchange.as_ref() {
            results.insert("Audit.Exchange".to_string(), filter.clone());
        }
        if let Some(filter) = self.dlp.as_ref() {
            results.insert("DLP.All".to_string(), filter.clone());
        }
        results
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct OutputSubConfig {
    pub file: Option<FileOutputSubConfig>,
    pub graylog: Option<GraylogOutputSubConfig>,
    pub fluentd: Option<FluentdOutputSubConfig>,
    #[serde(rename = "azureLogAnalytics")]
    pub oms: Option<OmsOutputSubConfig>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct FileOutputSubConfig {
    pub path: String,
    #[serde(rename = "separateByContentType")]
    pub separate_by_content_type: Option<bool>,
    pub separator: Option<String>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct GraylogOutputSubConfig {
    pub address: String,
    pub port: u16,
}

#[derive(Deserialize, Clone, Debug)]
pub struct FluentdOutputSubConfig {
    #[serde(rename = "tenantName")]
    pub tenant_name: String,
    pub address: String,
    pub port: u16,
}

#[derive(Deserialize, Clone, Debug)]
pub struct OmsOutputSubConfig {
    #[serde(rename = "workspaceId")]
    pub workspace_id: String,
}
