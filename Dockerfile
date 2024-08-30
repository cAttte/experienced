ARG LLVMTARGETARCH=x86_64
FROM --platform=${BUILDPLATFORM} ghcr.io/randomairborne/cross-cargo:${LLVMTARGETARCH} AS builder
ARG LLVMTARGETARCH=x86_64

WORKDIR /build

COPY . .

RUN cargo build --release --target ${LLVMTARGETARCH}-unknown-linux-musl

FROM alpine:latest
ARG LLVMTARGETARCH=x86_64

WORKDIR /experienced/

COPY --from=builder /build/target/${LLVMTARGETARCH}-unknown-linux-musl/release/xpd-gateway /usr/bin/xpd-gateway
COPY xpd-card-resources xpd-card-resources

ENTRYPOINT [ "/usr/bin/xpd-gateway" ]