use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::{thread, time};

use clap::{App, Arg};
use gethostname::gethostname;
use lru::LruCache;
use procfs::process::all_processes;
use rmesg::log_entries;
use rmesg::Backend;
use signal_hook::flag;

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

fn notify_oom(pid: i32, cmdline: String) {
    println!(
        "hostname:{:?} pid:{} cmdline:{}",
        gethostname(),
        pid,
        cmdline
    );
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
                .short('p')
                .long("process-refresh")
                .value_name("process_refresh")
                .about("Set the frequency to refresh the list of processes in milliseconds")
                .takes_value(false)
                .default_value("5000"),
        )
        .arg(
            Arg::new("kernel-log-refresh")
                .short('k')
                .long("kernel-log-refresh")
                .value_name("kernel_refresh")
                .about("Set the frequency to refresh the list of processes in milliseconds")
                .takes_value(true)
                .default_value("10000"),
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
        while !term_d.load(Ordering::Relaxed) {
            {
                let mut procs = procs_d.lock().unwrap();

                let entries = log_entries(Backend::Default, true).unwrap();
                for entry in entries {
                    let lowercase_message = entry.message.to_lowercase();

                    /*
                        Example kernel log entries we want to detect:
                        [Sat Jun 26 22:34:14 2021] Out of memory: Killed process 9865 (oom_trigger) total-vm:7468696kB, ... a lot more stuff ...
                    */

                    if lowercase_message.contains("out of memory:") {
                        for part in lowercase_message.split_whitespace() {
                            if is_string_numeric(part.to_string()) {
                                let pid = part.to_string().parse::<i32>().unwrap();

                                match procs.get(&pid) {
                                    Some(cmdline) => {
                                        let full_cmdline = cmdline.clone();
                                        procs.pop(&pid);
                                        notify_oom(pid, full_cmdline);
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
