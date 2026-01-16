use async_trait::async_trait;
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use chrono::Utc;
use futures::{stream, StreamExt};
use hmac::{Hmac, Mac};
use log::{error, info, warn};
use sha2::Sha256;
use crate::config::Config;
use crate::data_structures::Caches;
use crate::interfaces::interface::Interface;

pub struct OmsInterface {
    config: Config,
    key: String
}

impl OmsInterface {

    pub fn new(config: Config, key: String) -> Self {

        OmsInterface {
            config,
            key,
        }
    }
}

impl OmsInterface {
    fn build_signature(&self, date: String, content_length: usize, method: String,
                       content_type: String, resource: String) -> String {

        let x_headers = format!("x-ms-date:{}", date);
        let string_to_hash = format!("{}\n{}\n{}\n{}\n{}",
                                     method, content_length, content_type,
                                     x_headers, resource);
        let bytes_to_hash = string_to_hash.as_bytes();
        let decoded_key = BASE64_STANDARD.decode(self.key.clone()).unwrap();
        type HmacSha = Hmac<Sha256>;
        let mut encoded_hash = HmacSha::new_from_slice(&decoded_key).unwrap();
        encoded_hash.update(bytes_to_hash);
        let result = encoded_hash.finalize();
        let code_bytes = result.into_bytes();
        let b = BASE64_STANDARD.encode(code_bytes);
        let authorization = format!("SharedKey {}:{}",
                                    self.config.output.oms.as_ref().unwrap().workspace_id,
                                    b);
      authorization

    }
}

/// MEMORY FIX: Process logs in chunks to avoid creating all tasks upfront.
/// Previously, all logs were collected into a Vec before streaming, causing
/// memory spikes when flushing large caches (500k logs = ~500MB in task allocations).
const OMS_CHUNK_SIZE: usize = 1000;

#[async_trait]
impl Interface for OmsInterface {

    async fn send_logs(&mut self, logs: Caches) {
        let client = reqwest::Client::new();

        let resource = "/api/logs";
        let uri = format!("https://{}.ods.opinsights.azure.com{}?api-version=2016-04-01",
                          self.config.output.oms.as_ref().unwrap().workspace_id, resource);

        info!("Sending logs to OMS interface (chunked streaming).");

        // Process logs in chunks to avoid memory spikes
        for (content_type, content_logs) in logs.get_all_types() {
            if content_logs.is_empty() {
                continue;
            }

            info!("Sending {} {} logs to OMS in chunks of {}", content_logs.len(), content_type, OMS_CHUNK_SIZE);

            // Process in chunks
            for chunk in content_logs.chunks(OMS_CHUNK_SIZE) {
                let mut chunk_requests = Vec::with_capacity(chunk.len());

                for log in chunk.iter() {
                    let table_name = content_type.replace('.', "_");
                    let body = match serde_json::to_string(log) {
                        Ok(b) => b,
                        Err(e) => {
                            warn!("Failed to serialize log: {}", e);
                            continue;
                        }
                    };
                    let content_length = body.len();

                    let time_value = if let Some(i) = log.get("CreationTime") {
                        i.as_str().unwrap_or_default().to_string()
                    } else {
                        warn!("Expected CreationTime field, skipping log");
                        continue;
                    };

                    chunk_requests.push((body, table_name, time_value, content_length));
                }

                // Stream this chunk with bounded concurrency
                let calls = stream::iter(chunk_requests)
                    .map(|(body, table_name, time_value, content_length)| {
                        let client = client.clone();
                        let uri = uri.clone();
                        let method = "POST".to_string();
                        let content_type_header = "application/json".to_string();
                        let rfc1123date = Utc::now().format("%a, %d %b %Y %H:%M:%S GMT").to_string();
                        let signature = self.build_signature(
                            rfc1123date.clone(),
                            content_length,
                            method,
                            content_type_header,
                            resource.to_string()
                        );

                        async move {
                            let result = client
                                .post(&uri)
                                .header("content-type", "application/json")
                                .header("content-length", content_length)
                                .header("Authorization", signature)
                                .header("Log-Type", table_name)
                                .header("x-ms-date", rfc1123date)
                                .header("time-generated-field", time_value)
                                .body(body)
                                .send()
                                .await;

                            match result {
                                Ok(response) => {
                                    if !response.status().is_success() {
                                        match response.text().await {
                                            Ok(text) => error!("Error response after sending log to OMS: {}", text),
                                            Err(e) => error!("Error response after sending log to OMS, could not parse: {}", e),
                                        }
                                    }
                                },
                                Err(e) => {
                                    error!("Error sending log to OMS: {}", e);
                                }
                            }
                        }
                    })
                    .buffer_unordered(10);

                calls.for_each(|_| async {}).await;
            }
        }

        info!("Finished sending logs to OMS");
    }
}
