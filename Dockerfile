# strategy sourced from:

# https://www.fpcomplete.com/blog/2018/07/deploying-rust-with-docker-and-kubernetes
# http://whitfin.io/speeding-up-rust-docker-builds/
# https://kerkour.com/blog/rust-small-docker-image/

# This image will build all dependencies before you introducing the project's source code, 
# which means they'll be cached most of the time.

# Build Stage
FROM rust:latest AS builder
WORKDIR /usr/src/
ENV OPENSSL_DIR=/usr \
    PKG_CONFIG_ALLOW_CROSS=1 \
    OPENSSL_STATIC=true

RUN apt update && apt upgrade -y && apt-get install -y pkg-config libssl-dev musl-tools && rm -rf /var/lib/apt/lists/*
RUN rustup target add x86_64-unknown-linux-musl
 
RUN cargo new --bin oom-notifier
WORKDIR /usr/src/oom-notifier
COPY Cargo.toml Cargo.lock ./
RUN cargo build --release --target x86_64-unknown-linux-musl --features vendored

COPY src ./src
RUN cargo install --target x86_64-unknown-linux-musl --path .
RUN rm src/*.rs

# Bundle Stage
FROM scratch
COPY --from=builder /oom-notifier/target/x86_64-unknown-linux-musl/release/oom-notifier /
USER 1000
CMD ["./oom-notifier"]


