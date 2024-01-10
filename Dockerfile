FROM ubuntu:22.04

RUN mkdir -p /usr/local/bin
COPY ./target/x86_64-unknown-linux-musl/release/roxide /usr/local/bin

RUN apt update
RUN apt install -y git fzf

ENTRYPOINT [ "bash" ]
