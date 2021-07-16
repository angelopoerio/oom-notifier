# About
oom-notifier is a small daemon to be notified about oomed processes. It can report the full command line of the oomed process (something that it's not printed in the kernel ring buffer).


# How to use
The daemon needs to run with enough privileges to access /dev/kmsg (kernel logs) so it can know about OOMs happening in the system.
The events can be sent to different backends, at the moment syslog and Elasticsearch are supported.
Send event to an elasticsearch cluster:
```bash
./oom-notifier --elasticsearch-server https://my-elasticsearch-cluster:9200 --elasticsearch-index my-index
```

Send an event to a remote syslog server (over TCP):
```bash
./oom-notifier --syslog-server my-syslog-server:9999 --syslog-proto tcp
```

# Run on Kubernetes
It is possible to run the service as a [Daemonset](https://kubernetes.io/docs/concepts/workloads/controllers/daemonset/) on a Kubernetes cluster.
Depending on the configuration it might be needed to run it as a priviled pod (see [here](https://kubernetes.io/docs/tasks/configure-pod-container/security-context/))

# Caveats
The tool can only notify about oom caused by the Linux oom killer. If you use a userspace mechanism then it will not be able to detect them.
Some example of userspace services that act as oom-killer:
* [https://github.com/facebookincubator/oomd](oomd)
* [https://github.com/rfjakob/earlyoom](earlyoom)



# Author
Angelo Poerio <angelo.poerio@gmail.com>
