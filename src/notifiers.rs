use std::process;

use syslog::{Facility, Formatter3164};

pub fn syslog_notifier(message: String, proto: String, server: String) -> Result<String, String> {
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
                Ok(s) => Ok("".to_string()),
            },
        },
        "tcp" => match syslog::tcp(formatter, server) {
            Err(e) => Err(e.to_string()),
            Ok(mut writer) => match writer.err(message) {
                Err(e) => Err(e.to_string()),
                Ok(s) => Ok("".to_string()),
            },
        },
        "udp" => match syslog::udp(formatter, "127.0.0.1:4444", &server) {
            Err(e) => Err(e.to_string()),
            Ok(mut writer) => match writer.err(message) {
                Err(e) => Err(e.to_string()),
                Ok(s) => Ok("".to_string()),
            },
        },
        _ => Err("invalid configuration for proto passed to the syslog_notifier".to_string()),
    }
}
