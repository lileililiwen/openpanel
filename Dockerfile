# syntax=docker/dockerfile:1.7
# OpenPanel container image.
#
# The image embeds the statically-linked release binary built by
# `.github/workflows/release.yml` and exposes the same `/health`
# contract the metal installation uses. It runs as a non-root user,
# persists data through a declared volume, and terminates cleanly on
# SIGTERM so the orchestrator can stop the container without losing
# state.
#
# Build context: the repo root, with the binary pre-staged at
# `target/release/openpanel`. The release workflow does this; for
# local builds:
#   cargo build --release
#   cp target/release/openpanel ./openpanel
#   docker build -t openpanel:dev .

FROM debian:bookworm-slim AS runtime

ARG OPENPANEL_UID=10001
ARG OPENPANEL_GID=10001

ENV OPENPANEL_DATA_DIR=/var/lib/openpanel
ENV OPENPANEL_CONFIG=/etc/openpanel/openpanel.toml
ENV RUST_LOG=info

RUN groupadd --system --gid ${OPENPANEL_GID} openpanel \
 && useradd --system --uid ${OPENPANEL_UID} --gid openpanel \
    --home-dir ${OPENPANEL_DATA_DIR} --shell /usr/sbin/nologin \
    --comment "openpanel service account" openpanel \
 && mkdir -p ${OPENPANEL_DATA_DIR} /etc/openpanel \
 && chown -R openpanel:openpanel ${OPENPANEL_DATA_DIR} /etc/openpanel

COPY --chown=openpanel:openpanel openpanel /usr/local/bin/openpanel
COPY --chown=openpanel:openpanel packages/installer/entrypoint.sh /usr/local/bin/openpanel-entrypoint

RUN chmod 0755 /usr/local/bin/openpanel /usr/local/bin/openpanel-entrypoint \
 && apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates tini \
 && rm -rf /var/lib/apt/lists/*

USER openpanel
WORKDIR ${OPENPANEL_DATA_DIR}

VOLUME ["${OPENPANEL_DATA_DIR}"]

EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
  CMD ["/usr/local/bin/openpanel", "healthcheck"]

STOPSIGNAL SIGTERM

ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/openpanel-entrypoint"]
