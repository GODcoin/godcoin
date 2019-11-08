##### Stage 0
FROM rust:1.39-slim-buster

RUN apt-get update && \
    apt-get install -y \
        libsodium23 \
        libsodium-dev \
        make \
        clang

# Required for libsodium-sys crate
RUN rustup component add rustfmt

# Copy and build
COPY . .
RUN cargo install --path ./crates/server \
    && rm -r ./target

##### Stage 1
FROM debian:buster-slim
WORKDIR /app

ENV GODCOIN_HOME="/data"

COPY --from=0 /usr/local/cargo/bin/godcoin-server /app

STOPSIGNAL SIGINT
ENTRYPOINT ["/app/godcoin-server"]
