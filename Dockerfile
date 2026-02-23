# Reference: https://docs.docker.com/reference/dockerfile/
FROM rust:1.93 AS builder

RUN apt-get update && apt-get install -y musl-tools
RUN rustup target add x86_64-unknown-linux-musl

COPY . /app
WORKDIR /app

RUN cargo build --release --target x86_64-unknown-linux-musl

FROM scratch AS client-runtime

COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/rat-client /rat-client
ENTRYPOINT [ "/rat-client" ]

FROM scratch AS server-runtime

COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/rat-server /rat-server
ENTRYPOINT [ "/rat-server" ]
