FROM rust:1.38-slim-buster
WORKDIR /app

ENV GODCOIN_HOME="/data"

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

STOPSIGNAL SIGINT
ENTRYPOINT ["godcoin-server"]
