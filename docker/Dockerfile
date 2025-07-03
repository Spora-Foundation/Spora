# syntax=docker/dockerfile:1

FROM ubuntu:24.04
SHELL ["/bin/bash", "-c"]

ARG SHELL
ARG WORKDIR
ARG DK_USER

ARG DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt install -y sudo

# sudo user
# RUN useradd -rmU ${DK_USER} -u 1000 -G sudo
RUN sed -i 's|%sudo.ALL=(ALL:ALL).ALL|%sudo ALL=(ALL:ALL) NOPASSWD: ALL|' /etc/sudoers

WORKDIR ${WORKDIR}
USER ${DK_USER}

# Timezone & loacale
RUN sudo apt-get install -y locales tzdata
RUN sudo localedef -i en_US -c -f UTF-8 -A /usr/share/locale/locale.alias en_US.UTF-8

# Common tools
RUN sudo apt-get install -y ca-certificates curl vim git git-lfs
RUN sudo apt-get install -y build-essential cmake libssl-dev pkg-config

# Rust toolchain
RUN export RUSTUP_DIST_SERVER="https://rsproxy.cn"
RUN export RUSTUP_UPDATE_ROOT="https://rsproxy.cn/rustup"
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- --default-toolchain stable -y

# Build
RUN sudo apt-get install -y protobuf-compiler libprotobuf-dev
RUN sudo apt-get install -y libclang-dev

ENV LANG=en_US.utf8
ENV SHELL=${SHELL}

CMD ["bash"]
