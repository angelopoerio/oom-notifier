# Strategy sourced from:
# https://www.fpcomplete.com/blog/2018/07/deploying-rust-with-docker-and-kubernetes
# http://whitfin.io/speeding-up-rust-docker-builds/
# https://kerkour.com/blog/rust-small-docker-image/

###############
# Build Stage #
###############
FROM rust:latest AS builder

WORKDIR /usr/src/
ENV OPENSSL_DIR=/usr \
    PKG_CONFIG_ALLOW_CROSS=1 \
    OPENSSL_STATIC=true

RUN apt update && apt upgrade -y && apt-get install -y musl gcc g++ musl-dev pkg-config libssl-dev musl-tools && rm -rf /var/lib/apt/lists/*
RUN rustup target add x86_64-unknown-linux-musl
 
RUN cargo new --bin oom-notifier
WORKDIR /usr/src/oom-notifier
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --target x86_64-unknown-linux-musl --features vendored

################
# Bundle Stage #
################
FROM scratch

COPY --from=builder /usr/src/oom-notifier/target/x86_64-unknown-linux-musl/release/oom-notifier /
USER 1000
CMD ["./oom-notifier"]
