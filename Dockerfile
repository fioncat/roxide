FROM rust:1.75-alpine3.19 AS builder

WORKDIR /usr/src/csync
COPY . .

RUN apk add --no-cache musl-dev git libressl-dev

RUN cargo build --release --target x86_64-unknown-linux-musl --locked
RUN mv ./target/x86_64-unknown-linux-musl/release/roxide /usr/local/cargo/bin/roxide

FROM alpine:3.19

RUN apk add --no-cache git fzf zsh neovim starship

COPY --from=builder /usr/local/cargo/bin/roxide /usr/local/bin

RUN mkdir -p /root/.config/roxide
COPY ./config/docker-config.yml /root/.config/roxide/config.yml

COPY ./scripts/docker-zshrc.zsh /root/.zshrc

WORKDIR /root

ENTRYPOINT [ "sleep", "infinity" ]
