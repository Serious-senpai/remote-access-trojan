# Reference: https://docs.docker.com/reference/dockerfile/
FROM ghcr.io/tianocore/containers/ubuntu-24-dev:51b0b07

RUN apt-get update && apt-get install -y \
    curl \
    git \
    musl-tools \
    ovmf \
    socat

RUN curl --proto '=https' --tlsv1.2 https://sh.rustup.rs -sSf | sh -s -- --default-toolchain=1.94 -y
RUN /root/.cargo/bin/rustup toolchain install nightly
RUN /root/.cargo/bin/rustup target add x86_64-unknown-linux-gnu x86_64-unknown-linux-musl

RUN curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.4/install.sh | bash
ENV NVM_DIR=/root/.nvm
RUN bash -c "source $NVM_DIR/nvm.sh && nvm install 25"

RUN git clone -b stable/202508 --recursive https://github.com/tianocore/edk2 /edk2
RUN cd /edk2 && make -C BaseTools
