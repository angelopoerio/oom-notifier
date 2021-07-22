use std::env;
use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time;

use clap::{App, Arg};
use lru::LruCache;
use procfs;
use rmesg::log_entries;
use rmesg::Backend;
use serde_json::json;
use signal_hook::flag;
use tokio::runtime::Runtime;

mod notifiers;

fn is_string_numeric(str: String) -> bool {
    for c in str.chars() {
        if !c.is_numeric() {
            return false;
        }
    }
    return true;
}

fn get_pid_max() -> usize {
    let content = fs::read_to_string("/proc/sys/kernel/pid_max").unwrap();
    return content.trim().parse::<usize>().unwrap();
}

fn build_oom_event(pid: i32, cmdline: String) -> serde_json::Value {
    let mut machine_hostname = "";
    match env::var("HOSTNAME") {
        Ok(val) => machine_hostname = val,
        Err(_) => match fs::read_to_string("/proc/sys/kernel/hostname") {
            Ok(host_name) => machine_hostname = host_name.trim().to_string(),
            Err(e) => machine_hostname = e.to_string(),
        },
    }
    let message = json!({ "cmdline": cmdline, "pid": pid.to_string(), "hostname":machine_hostname,
                "time": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis().to_string()});

    return message;
}

fn main() {
    let mut sleep_time_b = time::Duration::from_millis(5000);
    let mut sleep_time_d = time::Duration::from_millis(10000);
    let pid_max = get_pid_max();
    let processes = Arc::new(Mutex::new(LruCache::new(pid_max)));
    let procs_b = Arc::clone(&processes);
    let procs_d = Arc::clone(&processes);

    let matches = App::new("oom-notifier")
        .version("0.1")
        .author("Angelo Poerio <angelo.poerio@gmail.com>")
        .about("Notify about oomed processes reporting full command line")
        .arg(
            Arg::new("process-refresh")
                .long("process-refresh")
                .value_name("process_refresh")
                .about("Set the frequency to refresh the list of processes in milliseconds")
                .takes_value(true)
                .default_value("5000"),
        )
        .arg(
            Arg::new("kernel-log-refresh")
                .long("kernel-log-refresh")
                .value_name("kernel_refresh")
                .about("Set the frequency to refresh the list of processes in milliseconds")
                .takes_value(true)
                .default_value("10000"),
        )
        .arg(
            Arg::new("syslog-proto")
                .long("syslog-proto")
                .value_name("syslog_proto")
                .about("Set protocol to connect to the syslog-server. Options: unix/tcp/udp")
                .takes_value(true)
                .required(false)
        )
        .arg(
            Arg::new("syslog-server")
                .long("syslog-server")
                .value_name("syslog_server")
                .about("Syslog server where to send the oom events. It must have the form hostname:port. If unix protocol is used this option is ignored")
                .takes_value(true)
                .required(false)
        )
        .arg(
            Arg::new("elasticsearch-server")
                .long("elasticsearch-server")
                .value_name("elasticsearch_server")
                .about("Elasticsearch server where to send the events. It must have the format http://hostname:port")
                .takes_value(true)
                .required(false)
        )
        .arg(
            Arg::new("elasticsearch-index")
                .long("elasticsearch-index")
                .value_name("elasticsearch_index")
                .about("The name of the elasticsearch index where to index the oom events")
                .takes_value(true)
                .required(false)
        )
        .arg(
            Arg::new("kafka-brokers")
                .long("kafka-brokers")
                .value_name("kafka_brokers")
                .about("Kafka cluster where to send the events. It must have the format broker1:port1,broker2:port2, ... , brokerN:portN")
                .takes_value(true)
                .required(false)
        )
        .arg(
            Arg::new("kafka-topic")
                .long("kafka-topic")
                .value_name("kafka_topic")
                .about("The name of the kafka topic where to send the oom events")
                .takes_value(true)
                .required(false)
        )
        .get_matches();

    if let Some(p_r) = matches.value_of("process-refresh") {
        sleep_time_b = time::Duration::from_millis(p_r.parse::<u64>().unwrap());
    }

    if let Some(k_r) = matches.value_of("kernel-log-refresh") {
        sleep_time_d = time::Duration::from_millis(k_r.parse::<u64>().unwrap());
    }

    println!("pid_max of the system is {}", pid_max);

    let term_b = Arc::new(AtomicBool::new(false));
    flag::register(signal_hook::consts::SIGTERM, Arc::clone(&term_b)).unwrap();
    flag::register(signal_hook::consts::SIGINT, Arc::clone(&term_b)).unwrap();

    let term_d = Arc::new(AtomicBool::new(false));
    flag::register(signal_hook::consts::SIGTERM, Arc::clone(&term_d)).unwrap();
    flag::register(signal_hook::consts::SIGINT, Arc::clone(&term_d)).unwrap();

    let procs_browser = thread::spawn(move || {
        while !term_b.load(Ordering::Relaxed) {
            {
                let mut procs = procs_b.lock().unwrap();

                for proc in procfs::process::all_processes().unwrap() {
                    let cmdline = match proc.cmdline() {
                        Ok(cmdline) => cmdline.join(" "),
                        Err(error) => error.to_string(),
                    };

                    procs.put(proc.stat.pid, cmdline);
                }
            }
            std::thread::sleep(sleep_time_b);
        }

        println!("Received termination signal. Exiting processes list refresher thread");
    });

    let dmesg_browser = thread::spawn(move || {
        let mut syslog_proto = "";
        let mut syslog_server = "";
        let mut elasticsearch_server = "";
        let mut elasticsearch_index = "";
        let mut kafka_brokers = "";
        let mut kafka_topic = "";

        if let Some(s_p) = matches.value_of("syslog-proto") {
            syslog_proto = s_p;
        }

        if let Some(s_s) = matches.value_of("syslog-server") {
            syslog_server = s_s.clone();
        }

        if let Some(e_s) = matches.value_of("elasticsearch-server") {
            elasticsearch_server = e_s.clone();
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

        while !term_d.load(Ordering::Relaxed) {
            {
                let mut procs = procs_d.lock().unwrap();

                let entries = log_entries(Backend::Default, true).unwrap();
                for entry in entries {
                    let lowercase_message = entry.message.to_lowercase();

                    /*
                        Example kernel log entries we want to detect:
                        Out of memory: Killed process 9865 (oom_trigger) total-vm:7468696kB, ... a lot more stuff ...
                    */

                    if lowercase_message.contains("out of memory:") {
                        for part in lowercase_message.split_whitespace() {
                            if is_string_numeric(part.to_string()) {
                                let pid = part.to_string().parse::<i32>().unwrap();

                                match procs.get(&pid) {
                                    Some(cmdline) => {
                                        let full_cmdline = cmdline.clone();
                                        procs.pop(&pid);
                                        let oom_event = build_oom_event(pid, full_cmdline);
                                        println!("{}", &oom_event);

                                        if !elasticsearch_index.is_empty()
                                            && !elasticsearch_server.is_empty()
                                        {
                                            let rt = Runtime::new().unwrap();
                                            println!("Sending event to Elasticsearch");

                                            match rt.block_on(notifiers::elasticsearch_notifier(
                                                &oom_event,
                                                elasticsearch_index.to_string(),
                                                elasticsearch_server.to_string(),
                                            )) {
                                                Err(e) => println!("Error while sending the oom event to the configured Elasticsearch: {}", e.to_string()),
                                                _ => (),
                                            }
                                        }

                                        if !kafka_topic.is_empty() && !kafka_brokers.is_empty() {
                                            println!("Send event to Kafka");

                                            match notifiers::kafka_notifier(&oom_event.to_string(), kafka_topic.to_string(), kafka_brokers.split(",").map(str::to_string).collect()) {
                                                Err(e) => println!("Error while sending the oom event to the configured Kafka: {}", e.to_string()),
                                                _ => (),
                                            }
                                        }

                                        if syslog_proto == "unix"
                                            || (!syslog_proto.is_empty()
                                                && !syslog_server.is_empty())
                                        {
                                            println!("Sending event to syslog");
                                            match notifiers::syslog_notifier(
                                                &oom_event.to_string(),
                                                syslog_proto.to_string(),
                                                syslog_server.to_string(),
                                            ) {
                                                Err(e) => println!("Error while sending the oom event to the configured syslog: {}", e.to_string()),
                                                _ => (),
                                            }
                                        }
                                    }
                                    _ => (),
                                }
                            }
                        }
                    }
                }
            }

            std::thread::sleep(sleep_time_d);
        }

        println!("Received termination signal. Exiting kernel log refresher thread");
    });

    procs_browser.join().unwrap();
    dmesg_browser.join().unwrap();
}
