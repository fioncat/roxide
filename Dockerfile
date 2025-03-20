ARG BUILD_IMAGE="golang:1-alpine"
ARG BASE_IMAGE="alpine:latest"

FROM ${BUILD_IMAGE} AS builder

RUN apk add --no-cache git make gcc build-base musl-dev

COPY . /roxide
WORKDIR /roxide
RUN make build

FROM ${BASE_IMAGE}

RUN apk add --no-cache git bash sqlite vim fzf bash-completion
ENV EDITOR=vim
COPY --from=builder /roxide/bin/roxide /usr/local/bin/roxide

RUN mkdir -p /root/.config/roxide/remotes
COPY ./pkg/config/test_remote_github.toml /root/.config/roxide/remotes/github.toml
COPY ./pkg/config/test_remote_local.toml /root/.config/roxide/remotes/test.toml

RUN echo "source <(roxide init bash)" >> /root/.bashrc

ENTRYPOINT ["bash"]
