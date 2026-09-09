#!/usr/bin/env bash
# packages/installer/entrypoint.sh — container entrypoint.
#
# Initializes the data volume layout on first start, then execs the
# release binary as the openpanel service account. Health and
# readiness are exposed by the binary itself; the orchestrator should
# use the Dockerfile HEALTHCHECK (or an equivalent probe) to gate
# traffic.

set -euo pipefail

DATA_DIR="${OPENPANEL_DATA_DIR:-/var/lib/openpanel}"
CONFIG_DIR="${OPENPANEL_CONFIG%/*}"

mkdir -p "${DATA_DIR}" "${CONFIG_DIR:-/etc/openpanel}"

if [ ! -f "${CONFIG_DIR:-/etc/openpanel}/openpanel.toml" ] \
    && [ -f /etc/openpanel/openpanel.toml.dist ]; then
    cp /etc/openpanel/openpanel.toml.dist \
       "${CONFIG_DIR:-/etc/openpanel}/openpanel.toml"
fi

exec /usr/local/bin/openpanel "$@"
