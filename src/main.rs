use std::env;
use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time;

use clap::{App, Arg};
use env_logger::Env;
use lru::LruCache;
use rmesg::log_entries;
use rmesg::Backend;
use serde_json::json;
use signal_hook::flag;
use tokio::runtime::Runtime;

mod notifiers;

#[macro_use]
extern crate log;

fn is_string_numeric(str: String) -> bool {
    for c in str.chars() {
        if !c.is_numeric() {
            return false;
        }
    }
    return true;
}

fn get_uptime() -> Result<time::Duration, String> {
    match fs::read_to_string("/proc/uptime") {
        Err(e) => return Err(format!("Could not read /proc/uptime: {}", e)),
        Ok(content) => {
            let uptime = time::Duration::from_secs_f32(
                content
                    .split_whitespace()
                    .next()
                    .unwrap_or("0")
                    .parse::<f32>()
                    .unwrap_or(0.0),
            );

            return Ok(uptime);
        }
    }
}

fn get_pid_max() -> Result<usize, String> {
    match fs::read_to_string("/proc/sys/kernel/pid_max") {
        Err(e) => return Err(format!("Could not read /proc/sys/kernel/pid_max: {}", e)),
        Ok(content) => return Ok(content.trim().parse::<usize>().unwrap()), // this is guaranteed to be an integer
    }
}

fn get_hostname() -> String {
    match env::var("HOSTNAME") {
        Ok(val) => return val,
        Err(_) => match fs::read_to_string("/proc/sys/kernel/hostname") {
            Ok(host_name) => return host_name.trim().to_string(),
            Err(e) => {
                error!(
                    "Could not read /proc/sys/kernel/hostname to obtain the hostname: {}",
                    e
                );

                return "N/A".to_string();
            }
        },
    }
}

fn get_kernel_version() -> String {
    match fs::read_to_string("/proc/version") {
        Ok(kernel_version) => return kernel_version.trim().to_string(),
        Err(e) => {
            error!(
                "Could not read /proc/version to obtain the kernel version: {}",
                e
            );

            return "N/A".to_string();
        }
    }
}

fn build_oom_event(pid: i32, cmdline: String) -> serde_json::Value {
    let message = json!({ "cmdline": cmdline,
                    "pid": pid.to_string(),
                    "hostname": get_hostname(),
                    "kernel": get_kernel_version(),
                "time": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis().to_string()});

    return message;
}

fn main() {
    let mut sleep_time_b = time::Duration::from_millis(5000);
    let mut sleep_time_d = time::Duration::from_millis(10000);
    let mut pid_max = 0;

    match get_pid_max() {
        Ok(p_max) => pid_max = p_max,
        Err(e) => {
            error!("Could not determine the pid_max of the system: {}", e);
            std::process::exit(1)
        }
    }

    let processes = Arc::new(Mutex::new(LruCache::new(pid_max)));
    let procs_b = Arc::clone(&processes);
    let procs_d = Arc::clone(&processes);

    let env = Env::default().filter_or("LOGGING_LEVEL", "info");
    env_logger::init_from_env(env);

    let matches = App::new("oom-notifier")
        .version("0.1")
        .author("Angelo Poerio <angelo.poerio@gmail.com>")
        .about("Notify about oomed processes reporting full command line")
        .arg(
            Arg::new("process-refresh")
                .long("process-refresh")
                .alias("pr")
                .value_name("process_refresh")
                .about("Set the frequency to refresh the list of processes in milliseconds")
                .takes_value(true)
                .default_value("5000"),
        )
        .arg(
            Arg::new("kernel-log-refresh")
                .long("kernel-log-refresh")
                .alias("kr")
                .value_name("kernel_refresh")
                .about("Set the frequency to check for new Kernel log entries in milliseconds")
                .takes_value(true)
                .default_value("10000"),
        )
        .arg(
            Arg::new("syslog-proto")
                .long("syslog-proto")
                .alias("sp")
                .value_name("syslog_proto")
                .about("Set protocol to connect to the syslog-server. Options: unix/tcp/udp")
                .takes_value(true)
                .required(false)
        )
        .arg(
            Arg::new("syslog-server")
                .long("syslog-server")
                .alias("ss")
                .value_name("syslog_server")
                .about("Syslog server where to send the oom events. It must have the form hostname:port. If unix protocol is used this option is ignored")
                .takes_value(true)
                .required(false)
        )
        .arg(
            Arg::new("elasticsearch-server")
                .long("elasticsearch-server")
                .alias("es")
                .value_name("elasticsearch_server")
                .about("Elasticsearch server where to send the events. It must have the format http://hostname:port")
                .takes_value(true)
                .required(false)
        )
        .arg(
            Arg::new("elasticsearch-index")
                .long("elasticsearch-index")
                .alias("ei")
                .value_name("elasticsearch_index")
                .about("The name of the elasticsearch index where to index the oom events")
                .takes_value(true)
                .required(false)
        )
        .arg(
            Arg::new("kafka-brokers")
                .long("kafka-brokers")
                .alias("kb")
                .value_name("kafka_brokers")
                .about("Kafka cluster where to send the events. It must have the format broker1:port1,broker2:port2, ... , brokerN:portN")
                .takes_value(true)
                .required(false)
        )
        .arg(
            Arg::new("kafka-topic")
                .long("kafka-topic")
                .alias("kt")
                .value_name("kafka_topic")
                .about("The name of the kafka topic where to send the oom events")
                .takes_value(true)
                .required(false)
        )
        .arg(
            Arg::new("slack-webhook")
                .long("slack-webhook")
                .alias("slw")
                .value_name("slack_webhook")
                .about("Slack webhook where the post the notifications")
                .takes_value(true)
                .required(false)
        )
        .arg(
            Arg::new("slack-channel")
                .long("slack-channel")
                .alias("slc")
                .value_name("slack_channel")
                .about("The slack channel where to post the notifications")
                .takes_value(true)
                .required(false)
        )
        .get_matches();

    if let Some(p_r) = matches.value_of("process-refresh") {
        match p_r.parse::<u64>() {
            Ok(val) => sleep_time_b = time::Duration::from_millis(val),
            Err(e) => error!("Invalid value specified for the parameter process-refresh, fallback to the default one. Error : {}", e),
        }
    }

    if let Some(k_r) = matches.value_of("kernel-log-refresh") {
        match k_r.parse::<u64>() {
            Ok(val) => sleep_time_d = time::Duration::from_millis(val),
            Err(e) => error!("Invalid value specified for the parameter kernel-log-refresh, fallback to the default one. Error : {}", e),
        }
    }

    info!("pid_max of the system is {}", pid_max);

    let term_b = Arc::new(AtomicBool::new(false));
    flag::register(signal_hook::consts::SIGTERM, Arc::clone(&term_b))
        .expect("Could not install the SIGTERM handler for the process-refresher thread");
    flag::register(signal_hook::consts::SIGINT, Arc::clone(&term_b))
        .expect("Could not install the SIGINT handler for the process-refresher thread");

    let term_d = Arc::new(AtomicBool::new(false));
    flag::register(signal_hook::consts::SIGTERM, Arc::clone(&term_d))
        .expect("Could not install the SIGTERM handler for the kernel-log-refresher thread");
    flag::register(signal_hook::consts::SIGINT, Arc::clone(&term_d))
        .expect("Could not install the SIGINT handler for the kernel-log-refresher thread");

    let procs_browser = thread::spawn(move || {
        while !term_b.load(Ordering::Relaxed) {
            {
                match procs_b.lock() {
                    Ok(mut procs) => match procfs::process::all_processes() {
                        Ok(procs_list) => {
                            for proc in procs_list {
                                let cmdline = match proc.cmdline() {
                                    Ok(cmdline) => cmdline.join(" "),
                                    Err(error) => error.to_string(),
                                };

                                debug!(
                                    "Adding/Overwriting process {} with command line: {}",
                                    proc.stat.pid, cmdline
                                );
                                procs.put(proc.stat.pid, cmdline);
                            }
                        }
                        Err(e) => error!("Could not list the processes running on the host: {}", e),
                    },
                    Err(e) => error!(
                        "Could not acquire the process table lock in the process-refresher thread!. Error: {}", e
                    ),
                }
            }
            std::thread::sleep(sleep_time_b);
        }

        info!("Received termination signal. Exiting processes list refresher thread");
    });

    let dmesg_browser = thread::spawn(move || {
        let mut syslog_proto = "";
        let mut syslog_server = "";
        let mut elasticsearch_server = "";
        let mut elasticsearch_index = "";
        let mut kafka_brokers = "";
        let mut kafka_topic = "";
        let mut slack_webhook = "";
        let mut slack_channel = "";
        let mut last_observed_timestamp = time::Duration::from_secs(0);

        match get_uptime() {
            Ok(uptime) => {
                last_observed_timestamp = uptime;
                info!("Machine uptime is {:#?}", last_observed_timestamp);
            }
            Err(err) => error!("Could not determine the machine uptime: {}", err),
        }

        if let Some(s_p) = matches.value_of("syslog-proto") {
            syslog_proto = s_p;
        }

        if let Some(s_s) = matches.value_of("syslog-server") {
            syslog_server = s_s;
        }

        if let Some(e_s) = matches.value_of("elasticsearch-server") {
            elasticsearch_server = e_s;
        }

        if let Some(e_i) = matches.value_of("elasticsearch-index") {
            elasticsearch_index = e_i;
        }

        if let Some(k_b) = matches.value_of("kafka-brokers") {
            kafka_brokers = k_b;
        }

        if let Some(k_t) = matches.value_of("kafka-topic") {
            kafka_topic = k_t;
        }

        if let Some(s_w) = matches.value_of("slack-webhook") {
            slack_webhook = s_w;
        }

        if let Some(s_c) = matches.value_of("slack-channel") {
            slack_channel = s_c;
        }

        while !term_d.load(Ordering::Relaxed) {
            {
                match procs_d.lock() {
                    Ok(mut procs) => {
                        let mut entries = Vec::new();

                        match log_entries(Backend::Default, true) {
                            Ok(ok_entries) => entries = ok_entries,
                            Err(e) => error!("Could not get the log entries from the kernel ring buffer: {}", e),
                        }

                        for entry in entries {
                            let lowercase_message = entry.message.to_lowercase();
                            let timestamp_from_system_start = entry
                                .timestamp_from_system_start
                                .unwrap_or(time::Duration::from_secs(0));

                            if timestamp_from_system_start <= last_observed_timestamp {
                                debug!(
                            "Skipping kernel log entry with timestamp from system start {:?}",
                            timestamp_from_system_start
                        );
                                continue;
                            }

                            last_observed_timestamp = timestamp_from_system_start;
                            debug!("New log entry from the kernel: {}", entry.message);

                            /*
                                Example kernel log entries we want to detect:
                                Out of memory: Killed process 9865 (oom_trigger) total-vm:7468696kB, ... a lot more stuff ...
                            */

                            if lowercase_message.contains("out of memory:") {
                                let mut pid_found = false;
                                for part in lowercase_message.split_whitespace() {
                                    if is_string_numeric(part.to_string()) {
                                        if pid_found {
                                            debug!("I have already found the pid for this oom event, quitting the parsing loop");
                                            break;
                                        }
                                        let pid = part.to_string().parse::<i32>().unwrap(); // this is guaranteed to be a PID from the kernel log
                                        pid_found = true;

                                        match procs.get(&pid) {
                                    Some(cmdline) => {
                                        let full_cmdline = cmdline.clone();
                                        procs.pop(&pid);
                                        let oom_event = build_oom_event(pid, full_cmdline);
                                        info!("New OOM event: {}", &oom_event);

                                        if !elasticsearch_index.is_empty()
                                            && !elasticsearch_server.is_empty()
                                        {
                                            match Runtime::new() {
                                                Ok(rt) => {
                                                    info!("Sending event to Elasticsearch");

                                                    match rt.block_on(notifiers::elasticsearch_notifier(
                                                        &oom_event,
                                                        elasticsearch_index.to_string(),
                                                        elasticsearch_server.to_string(),
                                                    )) {
                                                        Err(e) => error!("Error while sending the oom event to the configured Elasticsearch: {}", e.to_string()),
                                                        _ => info!("OOM event successfully indexed in Elasticsearch"),
                                                    }
                                                },
                                                Err(e) => error!("Could not create a tokyo runtime instance to send the event to Elasticsearch: {}", e)
                                            }
                                        }

                                        if !slack_channel.is_empty() && !slack_webhook.is_empty() {
                                            match Runtime::new() {
                                                Ok(rt) => {
                                                    info!("Sending event to Slack on channel {}", slack_channel);

                                                    match rt.block_on(notifiers::slack_notifier(&oom_event, slack_webhook.to_string(), slack_channel.to_string())) {
                                                        Err(e) => error!("Error while sending the oom event to the configured slack webhook: {}", e.to_string()),
                                                        _ => info!("OOM event successfully delivered to Slack"),
                                                    }
                                                },
                                                Err(e) => error!("Could not create a tokyo runtime instance to send the event to Slack: {}", e),
                                            }
                                        }

                                        if !kafka_topic.is_empty() && !kafka_brokers.is_empty() {
                                            info!("Sending event to Kafka");

                                            match notifiers::kafka_notifier(&oom_event.to_string(), kafka_topic.to_string(), kafka_brokers.split(",").map(str::to_string).collect()) {
                                                Err(e) => error!("Error while sending the oom event to the configured Kafka: {}", e.to_string()),
                                                _ => info!("OOM event successfully delivered to Kafka"),
                                            }
                                        }

                                        if syslog_proto == "unix"
                                            || (!syslog_proto.is_empty()
                                                && !syslog_server.is_empty())
                                        {
                                            info!("Sending event to syslog");
                                            match notifiers::syslog_notifier(
                                                &oom_event.to_string(),
                                                syslog_proto.to_string(),
                                                syslog_server.to_string(),
                                            ) {
                                                Err(e) => error!("Error while sending the oom event to the configured syslog: {}", e.to_string()),
                                                _ => info!("OOM event successfully delivered to Syslog"),
                                            }
                                        }
                                    }
                                    _ => error!("Detected OOM for pid {} but could not obtain informations about the process", pid),
                                }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => error!("Could not acquire the process table lock in the process-refresher thread!. Error: {}", e),
                }
            }

            std::thread::sleep(sleep_time_d);
        }

        info!("Received termination signal. Exiting kernel log refresher thread");
    });

    procs_browser
        .join()
        .expect("Could not join() the process-refresher thread");
    dmesg_browser
        .join()
        .expect("Could not join() the kernel-log-refresher thread");
}
