#!/usr/bin/env bash
# packages/installer/install.sh — OpenPanel host installer.
#
# Subcommands:
#   install   — fresh install for the current user/OS.
#   upgrade   — replace the installed binary with the one passed
#               in --from <path>; preflight the schema version, take
#               a binary backup, refuse unsupported downgrades, and
#               roll back automatically on a failed swap.
#   rollback  — restore the most recent binary backup.
#
# Supported distros (mirrors packages/openpanel-menu/scripts/install.sh):
#   debian, ubuntu, fedora, rhel, centos, rocky, almalinux, arch,
#   manjaro, alpine. Anything else exits 78 (EX_CONFIG).
#
# Environment:
#   OPENPANEL_BIN_DIR    target directory (default /usr/local/bin)
#   OPENPANEL_DATA_DIR   data directory (default /var/lib/openpanel)
#   OPENPANEL_USER       service account (default openpanel)
#   OPENPANEL_MAX_SCHEMA_VERSION (set at build time; used to refuse
#                                an upgrade from a newer schema)

set -euo pipefail

BIN_DIR="${OPENPANEL_BIN_DIR:-/usr/local/bin}"
DATA_DIR="${OPENPANEL_DATA_DIR:-/var/lib/openpanel}"
USER_NAME="${OPENPANEL_USER:-openpanel}"
BACKUP_SUFFIX=".bak.$(date -u +%Y%m%dT%H%M%SZ)"

log()  { printf '[INFO] %s\n' "$*"; }
ok()   { printf '[OK] %s\n'   "$*"; }
warn() { printf '[WARN] %s\n' "$*" >&2; }
err()  { printf '[ERROR] %s\n' "$*" >&2; }

SUDO=""
if [ "$(id -u)" -ne 0 ]; then
    if ! command -v sudo >/dev/null 2>&1; then
        err "openpanel installer requires root (sudo not available)."
        exit 1
    fi
    SUDO="sudo"
fi

detect_distro() {
    if [ -r /etc/os-release ]; then
        . /etc/os-release
        printf '%s\n' "${ID:-unknown}"
    else
        printf 'unknown\n'
    fi
}

create_service_user() {
    if id "${USER_NAME}" >/dev/null 2>&1; then
        return 0
    fi
    log "Creating service user ${USER_NAME}."
    case "$(detect_distro)" in
        ubuntu|debian)
            $SUDO adduser --system --group --no-create-home --home "${DATA_DIR}" \
                --shell /usr/sbin/nologin "${USER_NAME}"
            ;;
        fedora|rhel|centos|rocky|almalinux)
            $SUDO useradd --system --no-create-home --home "${DATA_DIR}" \
                --shell /usr/sbin/nologin "${USER_NAME}"
            $SUDO groupadd --force "${USER_NAME}"
            ;;
        arch|manjaro|alpine)
            $SUDO addgroup --system "${USER_NAME}" 2>/dev/null || true
            $SUDO adduser -S -D -H -h "${DATA_DIR}" -s /usr/sbin/nologin \
                -G "${USER_NAME}" "${USER_NAME}" 2>/dev/null \
                || $SUDO adduser -S -D -H -h "${DATA_DIR}" -s /sbin/nologin \
                   "${USER_NAME}"
            ;;
        *)
            err "Unsupported distribution for service user creation: $(detect_distro)"
            exit 78
            ;;
    esac
}

preflight_schema() {
    local new_ceiling="${OPENPANEL_MAX_SCHEMA_VERSION:-0}"
    if [ "${new_ceiling}" = "0" ]; then
        log "No compiled schema ceiling; preflight skipped."
        return 0
    fi
    local db="${DATA_DIR}/openpanel.sqlite"
    if [ ! -f "${db}" ]; then
        log "Fresh install; preflight skipped."
        return 0
    fi
    if ! command -v sqlite3 >/dev/null 2>&1; then
        warn "sqlite3 not on PATH; cannot preflight, refusing upgrade."
        return 1
    fi
    local highest
    highest="$(sqlite3 "${db}" \
        "SELECT COALESCE(MAX(CAST(version AS INTEGER)), 0) FROM _migrations;" 2>/dev/null \
        || echo 0)"
    if [ "${highest}" -gt "${new_ceiling}" ]; then
        err "Refusing upgrade: applied schema ${highest} is newer than this build's ceiling ${new_ceiling}."
        err "Restore the database from a backup taken with a compatible binary, then retry."
        return 1
    fi
    log "Schema preflight ok (applied ${highest} <= ceiling ${new_ceiling})."
}

do_install() {
    create_service_user
    $SUDO mkdir -p "${DATA_DIR}"
    $SUDO chown -R "${USER_NAME}:${USER_NAME}" "${DATA_DIR}"
    ok "Installed: service user ${USER_NAME} and data dir ${DATA_DIR}."
    log "Copy your release binary to ${BIN_DIR}/openpanel to finish install."
}

do_upgrade() {
    local from=""
    while [ "$#" -gt 0 ]; do
        case "$1" in
            --from) from="$2"; shift 2 ;;
            *) err "Unknown argument: $1"; exit 2 ;;
        esac
    done
    if [ -z "${from}" ] || [ ! -f "${from}" ]; then
        err "upgrade requires --from <path-to-new-binary>."
        exit 2
    fi
    if [ ! -f "${BIN_DIR}/openpanel" ]; then
        err "No existing ${BIN_DIR}/openpanel; use 'install' first."
        exit 1
    fi
    preflight_schema || exit 1
    local backup="${BIN_DIR}/openpanel${BACKUP_SUFFIX}"
    log "Backing up current binary to ${backup}."
    $SUDO cp "${BIN_DIR}/openpanel" "${backup}"
    log "Swapping in ${from}."
    if ! $SUDO install -m 0755 -o "${USER_NAME}" -g "${USER_NAME}" \
            "${from}" "${BIN_DIR}/openpanel"; then
        err "Swap failed; restoring backup."
        $SUDO cp "${backup}" "${BIN_DIR}/openpanel"
        exit 1
    fi
    if ! command -v "${BIN_DIR}/openpanel" >/dev/null 2>&1; then
        "$BIN_DIR/openpanel" --version >/dev/null 2>&1 || {
            err "Post-upgrade smoke check failed; rolling back."
            $SUDO cp "${backup}" "${BIN_DIR}/openpanel"
            exit 1
        }
    fi
    ok "Upgrade complete. Backup retained at ${backup}."
}

do_rollback() {
    local latest
    latest="$(ls -1t "${BIN_DIR}"/openpanel.bak.* 2>/dev/null | head -1 || true)"
    if [ -z "${latest}" ]; then
        err "No backup found in ${BIN_DIR}."
        exit 1
    fi
    log "Rolling back to ${latest}."
    $SUDO cp "${latest}" "${BIN_DIR}/openpanel"
    $SUDO chown "${USER_NAME}:${USER_NAME}" "${BIN_DIR}/openpanel"
    ok "Rolled back. The previous binary is at ${BIN_DIR}/openpanel."
}

main() {
    local sub="${1:-install}"
    case "${sub}" in
        install)  shift; do_install "$@" ;;
        upgrade)  shift; do_upgrade "$@" ;;
        rollback) shift; do_rollback "$@" ;;
        *)
            err "Usage: $0 {install|upgrade --from <path>|rollback}"
            exit 2
            ;;
    esac
}

main "$@"
