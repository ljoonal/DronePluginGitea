FROM rust:latest AS builder
WORKDIR /usr/local/src/drone-plugin-gitea
COPY src src/
COPY Cargo.toml Cargo.toml
COPY Cargo.lock Cargo.lock
RUN apt-get update && apt-get -y install ca-certificates cmake musl-tools libssl-dev && rm -rf /var/lib/apt/lists/*
RUN ["rustup", "target", "add", "x86_64-unknown-linux-musl"]
RUN ["cargo", "build", "--no-default-features", "--release", "--target", "x86_64-unknown-linux-musl"]

FROM scratch

LABEL org.opencontainers.image.authors="ljoonal"

USER 1000

COPY --from=builder --chown=1000:1000 /usr/local/src/drone-plugin-gitea/target/x86_64-unknown-linux-musl/release/drone-plugin-gitea /bin/drone-plugin-gitea

ENTRYPOINT ["/bin/drone-plugin-gitea"]
