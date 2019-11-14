##### Stage 0
FROM rust:1.39-slim-buster
WORKDIR /app

RUN apt-get update && \
    apt-get install -y \
        make \
        clang

# Copy and build
COPY . .
RUN cargo build -p godcoin-server --release

##### Stage 1
FROM debian:buster-slim
WORKDIR /app

ENV GODCOIN_HOME="/data"

COPY --from=0 /app/target/release/godcoin-server /app

STOPSIGNAL SIGINT
ENTRYPOINT ["/app/godcoin-server"]
