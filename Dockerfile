# Reference: https://docs.docker.com/reference/dockerfile/
FROM rust:1.93 AS rust-builder

RUN apt-get update && apt-get install -y musl-tools
RUN rustup target add x86_64-unknown-linux-musl

COPY . /app
WORKDIR /app

# Build each workspace member separately because a full workspace build use the union of all
# dependencies, which causes some executables to include unnecessary stuff.
RUN cargo build --release -p rat-client --target x86_64-unknown-linux-musl
RUN cargo build --release -p rat-server --target x86_64-unknown-linux-musl

RUN strip target/x86_64-unknown-linux-musl/release/rat-client
RUN strip target/x86_64-unknown-linux-musl/release/rat-server

FROM node:25.7 AS node-builder

COPY rat-frontend /app
WORKDIR /app

RUN npm install
RUN npm run build

FROM ubuntu:24.04 AS ubuntu-client-runtime

COPY --from=rust-builder /app/target/x86_64-unknown-linux-musl/release/rat-client /rat-client
ENTRYPOINT [ "/rat-client" ]

FROM centos:7 AS centos-client-runtime

COPY --from=rust-builder /app/target/x86_64-unknown-linux-musl/release/rat-client /rat-client
ENTRYPOINT [ "/rat-client" ]

FROM debian:stable-20260223 AS debian-client-runtime

COPY --from=rust-builder /app/target/x86_64-unknown-linux-musl/release/rat-client /rat-client
ENTRYPOINT [ "/rat-client" ]

FROM alpine:3.23 AS alpine-client-runtime

COPY --from=rust-builder /app/target/x86_64-unknown-linux-musl/release/rat-client /rat-client
ENTRYPOINT [ "/rat-client" ]

FROM scratch AS server-runtime

COPY --from=rust-builder /app/target/x86_64-unknown-linux-musl/release/rat-server /rat-server
COPY --from=node-builder /app/dist /frontend
ENTRYPOINT [ "/rat-server", "--frontend-static-files", "/frontend" ]
