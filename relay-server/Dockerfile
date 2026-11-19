# Premium relay server image.
#
# Build context is the repository root (Render's default), because the relay uses
# the shared `premium-auth` crate:
#   docker build -f relay-server/Dockerfile -t premium-relay .

FROM rust:1-bookworm AS build
WORKDIR /src

# Only the two crates the relay needs. A small workspace replaces the repository's
# root Cargo.toml, which also lists the desktop app and Android bridge.
COPY premium-auth premium-auth
COPY relay-server relay-server
COPY Cargo.lock Cargo.lock
RUN printf '[workspace]\nresolver = "2"\nmembers = ["premium-auth", "relay-server"]\n' > Cargo.toml
RUN cargo build --release -p premium-relay

FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --no-create-home relay
COPY --from=build /src/target/release/premium-relay /usr/local/bin/premium-relay

# Render sets PORT (10000 by default) and terminates TLS in front of this service.
ENV PORT=10000
EXPOSE 10000
USER relay
CMD ["premium-relay"]
