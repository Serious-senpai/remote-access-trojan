# Reference: https://docs.docker.com/reference/dockerfile/
FROM ubuntu:24.04

RUN apt-get update && apt-get install -y \
    build-essential \
    curl \
    git \
    && curl --proto '=https' --tlsv1.2 https://sh.rustup.rs -sSf | sh -s -- --default-toolchain=1.93 -y \
    && /root/.cargo/bin/rustup toolchain install nightly
