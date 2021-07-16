FROM fedora
ADD ./target/release/oom-notifier /oom-notifier
CMD ["chmod","+x","/oom-notifier"]
