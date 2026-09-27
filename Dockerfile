# syntax=docker/dockerfile:1

# Builds a fully static musl binary and ships it on a distroless base that
# holds nothing but CA certificates and a non-root user.
#
# The build stage always runs on the build machine's own architecture and
# cross-compiles with xx, so building for another platform does not run the
# compiler under emulation:
#
#   docker buildx build --platform linux/amd64,linux/arm64 -t dut .

ARG RUST_VERSION=1.98
ARG ALPINE_VERSION=3.24

FROM --platform=$BUILDPLATFORM tonistiigi/xx:1.9.0 AS xx

FROM --platform=$BUILDPLATFORM rust:${RUST_VERSION}-alpine${ALPINE_VERSION} AS build
COPY --from=xx / /
RUN apk add --no-cache clang lld
ARG TARGETPLATFORM
RUN xx-apk add --no-cache gcc musl-dev

# Release settings for the image only, so local release builds stay quick.
ENV CARGO_PROFILE_RELEASE_LTO=true \
    CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 \
    CARGO_PROFILE_RELEASE_STRIP=symbols

WORKDIR /src
# The sources are bind-mounted rather than copied, and the registry and
# target directory live in cache mounts, so a code change recompiles only
# this crate and no layer ever holds the sources or build artefacts.
RUN --mount=type=bind,source=Cargo.toml,target=Cargo.toml \
    --mount=type=bind,source=Cargo.lock,target=Cargo.lock \
    --mount=type=bind,source=src,target=src \
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target,id=dut-target-${TARGETPLATFORM} \
    xx-cargo build --release --locked --bin dut \
 && binary="target/$(xx-cargo --print-target-triple)/release/dut" \
 && xx-verify --static "$binary" \
 && cp "$binary" /usr/local/bin/dut

FROM gcr.io/distroless/static-debian13:nonroot
COPY --from=build /usr/local/bin/dut /usr/local/bin/dut
# Listen on every interface: localhost inside the container is unreachable
# from outside it.
ENV DUT_BIND_ADDRESS=0.0.0.0:3000
EXPOSE 3000
ENTRYPOINT ["/usr/local/bin/dut"]
