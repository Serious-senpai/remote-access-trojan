# Reference: https://docs.docker.com/reference/dockerfile/
FROM ubuntu:24.04

RUN apt-get update && apt-get install -y \
    build-essential \
    curl \
    git \
    musl-tools

RUN curl --proto '=https' --tlsv1.2 https://sh.rustup.rs -sSf | sh -s -- --default-toolchain=1.93 -y
RUN /root/.cargo/bin/rustup toolchain install nightly
RUN /root/.cargo/bin/rustup target add x86_64-unknown-linux-gnu x86_64-unknown-linux-musl
