# About
oom-notifier is a small daemon to be notified about processes killed by the Linux [oom-killer](https://utcc.utoronto.ca/~cks/space/blog/linux/OOMKillerWhen). It can report the full command line of the oomed process (something that it's not printed in the kernel ring buffer).


# How to build
You need a working installation of the [Rust](https://www.rust-lang.org/) compiler, then you can build the service issuing the following command:
```bash
cargo build --release
```
if the build completes then you will find the compiled service at the following path in the current directory: **target/release/oom-notifier**


# Run as a docker container
It is possible to build a docker image of the service issuing the following command (after building the service):
```bash
docker build -t oom-notifier .
```
and then run it:
```bash
docker run -v /proc:/proc --privileged  oom-notifier /oom-notifier
```


# How to use
The daemon needs to run with enough privileges to access **/dev/kmsg** (kernel logs) so it can know about OOMs happening in the system.
The events can be sent to different backends, at the moment **Syslog**, **Elasticsearch**, **Kafka** and **Slack** are supported.
Send events to an elasticsearch cluster:
```bash
./oom-notifier --elasticsearch-server https://my-elasticsearch-cluster:9200 --elasticsearch-index my-index
```

Send events to a remote syslog server (over TCP):
```bash
./oom-notifier --syslog-server my-syslog-server:9999 --syslog-proto tcp
```

Send events to a Kafka cluster:
```bash
./oom-notifier --kafka-topic oom-events --kafka-brokers broker1:9092,broker2:9092,broker3:9092
```

Send events to a Slack channel (learn more [here](https://api.slack.com/messaging/webhooks)):
```bash
./oom-notifier --slack-webhook https://hooks.slack.com/services/T00000000/B00000000/XXXXXXXXXXXXXXXXXXXXXXXX --slack-channel #oom-notifications
```


You can adjust the logging level of the daemon setting the environment variable **LOGGING_LEVEL** (default level is info).

# Run on Kubernetes
It is possible to run the service as a [Daemonset](https://kubernetes.io/docs/concepts/workloads/controllers/daemonset/) on a Kubernetes cluster.
It must be run as a **privileged** Daemonset and with the option **hostPID** enabled (see [here](https://kubernetes.io/docs/tasks/configure-pod-container/security-context/) and [here](https://kubernetes.io/docs/concepts/policy/pod-security-policy/#host-namespaces)). A YAML template ready to be deployed (after adapting it to your environment) is available at **k8s/daemonset.yaml**.


# Caveats
The tool can only notify about OOMs caused by the Linux oom killer. If you use a userspace mechanism then it will not be able to detect them.
Some example of userspace services that act as oom-killer:
* [oomd](https://github.com/facebookincubator/oomd)
* [earlyoom](https://github.com/rfjakob/earlyoom)

If you want to prevent the daemon itself to be killed by the oom-killer you can adjust the **oom_adj** parameter as described [here](https://backdrift.org/oom-killer-how-to-create-oom-exclusions-in-linux)


# Author
Angelo Poerio <angelo.poerio@gmail.com>
