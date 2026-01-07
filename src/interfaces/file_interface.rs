use std::collections::HashMap;
use std::path::Path;
use std::fs::OpenOptions;
use std::io::Write;
use async_trait::async_trait;
use chrono::Utc;
use crate::config::Config;
use crate::data_structures::{ArbitraryJson, Caches};
use crate::interfaces::interface::Interface;

/// Interface that sends found logs to JSON file(s) - one JSON object per line (JSONL format)
pub struct FileInterface {
    config: Config,
    paths: HashMap<String, String>,
    postfix: String,
}

impl FileInterface {
    pub fn new(config: Config) -> Self {

        let postfix = Utc::now().format("%Y%m%d%H%M%S").to_string();
        let mut interface = FileInterface {
            config,
            paths: HashMap::new(),
            postfix: postfix.clone()
        };
        if interface.separate_by_content_type() {
            interface.create_content_type_paths();
        }
        interface
    }

    /// Based on the desired CSV path, create a path for each content type. Used
    /// when SeparateByContentType is true.
    fn create_content_type_paths(&mut self) {
        let path = Path::new(&self.config.output.file
            .as_ref()
            .unwrap()
            .path);
        let dir = path.parent();
        let stem = path
            .file_stem().unwrap()
            .to_str().unwrap()
            .to_string();

        // Get subscriptions from config (new format) or fallback to legacy content_types
        let content_strings = if !self.config.subscriptions.is_empty() {
            self.config.subscriptions.clone()
        } else {
            self.config.collect.as_ref()
                .map(|c| c.content_types.get_content_type_strings())
                .unwrap_or_else(Vec::new)
        };
        for content_type in content_strings {
            // Simple filename without timestamp - user handles rotation
            let filename = format!("{}.json",
                               content_type.as_str().replace('.', ""));
            let file = if let Some(parent) = dir {
                let parent_str = parent.to_str().unwrap();
                if parent_str.is_empty() {
                    filename
                } else {
                    format!("{}/{}", parent_str, filename)
                }
            } else {
                filename
            };
            self.paths.insert(content_type, file);
        }
    }

    /// Convenience method to get config property.
    fn separate_by_content_type(&self) -> bool {
        self.config.output.file.as_ref().unwrap().separate_by_content_type.unwrap_or(false)
    }

    /// Save the logs of all content types in a single JSON file (JSONL format - one JSON per line)
    fn send_logs_unified(&self, mut cache: Caches) {
        let all_logs = cache.get_all();
        let path = &self.config.output.file.as_ref().unwrap().path;

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap_or_else(|e| panic!("Error in file interface: Could not write to path '{}': {}", path, e));

        for logs in all_logs.iter() {
            for log in logs.iter() {
                let json_str = serde_json::to_string(log).unwrap();
                writeln!(file, "{}", json_str).unwrap();
            }
        }
        file.flush().unwrap();
    }

    /// Save the logs of each content type to a separate JSON file (JSONL format)
    fn send_logs_separated(&self, mut cache: Caches) {
        for (content_type, logs) in cache.get_all_types() {
            if logs.is_empty() {
                continue
            }
            let path = self.paths.get(&content_type).unwrap();
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .unwrap_or_else(|e| panic!("Error in file interface: Could not write to path '{}': {}", path, e));

            for log in logs {
                let json_str = serde_json::to_string(log).unwrap();
                writeln!(file, "{}", json_str).unwrap();
            }
            file.flush().unwrap();
        }
    }
}

#[async_trait]
impl Interface for FileInterface {
    async fn send_logs(&mut self, logs: Caches) {
        if !self.separate_by_content_type() {
            self.send_logs_unified(logs);
        } else {
            self.send_logs_separated(logs);
        }
    }
}


/// Get all column names in a heterogeneous collection of logs.
pub fn get_all_columns(logs: &[ArbitraryJson]) -> Vec<String> {

    let mut columns: Vec<String> = Vec::new();
    for log in logs.iter() {
        for k in log.keys() {
            if !columns.contains(k) {
                columns.push(k.to_string());
            }
        }
    }
    columns
}

/// Due to heterogeneous logs not all logs have all columns. Fill missing columns of
/// a log with an empty string.
pub fn fill_log(log: &ArbitraryJson, columns: &Vec<String>) -> Vec<String> {
    let mut new_log= Vec::new();
    for col in columns {
        if !log.contains_key(col) {
            new_log.push("".to_string());
        } else {
            new_log.push(log.get(col).unwrap().to_string())
        }
    }
    new_log
}