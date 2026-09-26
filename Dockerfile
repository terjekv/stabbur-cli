# syntax=docker/dockerfile:1
FROM docker.io/library/rust:1.97.0-alpine3.24@sha256:ec9c91e77119ce498cd1e87d96d77e0f75b2cee21655a29bc2bf75a51a2b20a4 AS builder

ARG CARGO_BUILD_FLAGS="--locked --release"
ARG STABBUR_CLI_BUILD_CHANNEL="dev"
ARG STABBUR_CLI_BUILD_GIT_SHA=""

WORKDIR /usr/src/stabbur-cli

RUN apk add --no-cache build-base cmake

# Cargo resolves the public client from its immutable released Git revision.
COPY . .

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/usr/src/stabbur-cli/target \
    STABBUR_CLI_BUILD_CHANNEL="${STABBUR_CLI_BUILD_CHANNEL}" \
    STABBUR_CLI_BUILD_GIT_SHA="${STABBUR_CLI_BUILD_GIT_SHA}" \
    cargo build ${CARGO_BUILD_FLAGS} --bin stabbur && \
    cp target/release/stabbur /tmp/stabbur

RUN /tmp/stabbur --version

FROM scratch AS release-artifacts

COPY --from=builder /tmp/stabbur /stabbur
