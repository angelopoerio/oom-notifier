use std::process;
use std::time::Duration;

use elasticsearch::{http::transport::Transport, Elasticsearch, IndexParts};
use kafka::producer::{Producer, Record, RequiredAcks};
use syslog::{Facility, Formatter3164};

pub fn syslog_notifier(message: &String, proto: String, server: String) -> Result<String, String> {
    let formatter = Formatter3164 {
        facility: Facility::LOG_USER,
        hostname: None,
        process: "oom-notifier".to_string(),
        pid: process::id() as i32,
    };

    match proto.as_str() {
        "unix" => match syslog::unix(formatter) {
            Err(e) => Err(e.to_string()),
            Ok(mut writer) => match writer.err(message) {
                Err(e) => Err(e.to_string()),
                Ok(_) => Ok("".to_string()),
            },
        },
        "tcp" => match syslog::tcp(formatter, server) {
            Err(e) => Err(e.to_string()),
            Ok(mut writer) => match writer.err(message) {
                Err(e) => Err(e.to_string()),
                Ok(_) => Ok("".to_string()),
            },
        },
        "udp" => match syslog::udp(formatter, "127.0.0.1:4444", &server) {
            Err(e) => Err(e.to_string()),
            Ok(mut writer) => match writer.err(message) {
                Err(e) => Err(e.to_string()),
                Ok(_) => Ok("".to_string()),
            },
        },
        _ => Err("invalid configuration for protocol passed to the syslog_notifier".to_string()),
    }
}

pub async fn elasticsearch_notifier(
    message: &serde_json::Value,
    index: String,
    server: String,
) -> Result<String, String> {
    match Transport::single_node(&server.to_string()) {
        Err(e) => return Err(e.to_string()),
        Ok(transport) => match Elasticsearch::new(transport)
            .index(IndexParts::Index(&index.to_string()))
            .body(message)
            .send()
            .await
        {
            Err(e) => return Err(e.to_string()),
            Ok(response) => return Ok(response.status_code().to_string()),
        },
    }
}

pub fn kafka_notifier(
    message: &String,
    topic: String,
    brokers: Vec<String>,
) -> Result<String, String> {
    match Producer::from_hosts(brokers)
        .with_ack_timeout(Duration::from_secs(1)) /* TODO: make this parametric */
        .with_required_acks(RequiredAcks::One) /* same here */
        .create()
    {
        Err(e) => {
            return Err(format!(
                "Could not instantiate the kafka producer: {}",
                e.to_string()
            ))
        }
        Ok(mut producer) => match producer.send(&Record::from_value(&topic, message.as_bytes())) {
            Err(e) => {
                return Err(format!(
                    "Error while producing the event to kafka: {}",
                    e.to_string()
                ))
            }
            Ok(_) => return Ok("".to_string()),
        },
    }
}
