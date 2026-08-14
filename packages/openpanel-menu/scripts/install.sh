#!/usr/bin/env bash
# OpenPanel control-menu installer (curl | bash).
#
# Bootstraps Node.js 20+ via the system package manager and then
# runs `npm install -g openpanel-menu`. The script is idempotent
# and silent on partial failure (logs to stderr, exit code
# reflects the outcome).
#
# Supported distros (auto-detected from /etc/os-release):
#   - Debian / Ubuntu            (apt)
#   - Fedora / RHEL / CentOS     (dnf)
#   - Arch / Manjaro             (pacman)
#   - Alpine                     (apk)
#
# Anything else: exit 78 (EX_CONFIG) with a URL hint.

set -euo pipefail

# Resolve sudo: root needs none, non-root needs sudo.
SUDO=""
if [ "$(id -u)" -ne 0 ]; then
  if ! command -v sudo >/dev/null 2>&1; then
    printf '安装包需要 root 权限,但 sudo 不可用. 请以 root 重新运行此脚本.\n' >&2
    exit 1
  fi
  SUDO="sudo"
fi

log()  { printf '[INFO] %s\n' "$*"; }
ok()   { printf '[OK] %s\n'   "$*"; }
warn() { printf '[WARN] %s\n' "$*"; }
err()  { printf '[ERROR] %s\n' "$*" >&2; }

node_major() {
  # Print the major version of `node` if present, else empty.
  if ! command -v node >/dev/null 2>&1; then
    return 0
  fi
  node -v 2>/dev/null | sed -E 's/^v([0-9]+).*/\1/'
}

node_major_ge() {
  # True if the current node major is >= target.
  local current="$1" target="$2"
  if [ -z "$current" ]; then
    return 1
  fi
  [ "$current" -ge "$target" ]
}

# Idempotent: return 0 if Node >= 20 is already on PATH.
if node_major_ge "$(node_major)" 20; then
  ok "Node.js v$(node -v | tr -d 'v') 已安装,无需再装."
else
  if [ -r /etc/os-release ]; then
    . /etc/os-release
  else
    err "未找到 /etc/os-release,无法识别系统."
    err "请从 https://nodejs.org/en/download 手动安装 Node.js 20+,然后再运行此脚本."
    exit 78
  fi

  case "${ID:-}" in
    ubuntu|debian)
      log "检测到 Debian 系列,使用 apt 安装 Node.js 20."
      $SUDO apt-get update -y
      $SUDO apt-get install -y --no-install-recommends curl ca-certificates
      if [ "${ID}" = "debian" ]; then
        curl -fsSL https://deb.nodesource.com/setup_20.x | $SUDO bash -
      else
        curl -fsSL https://deb.nodesource.com/setup_20.x | $SUDO bash -
      fi
      $SUDO apt-get install -y --no-install-recommends nodejs
      ;;
    fedora|rhel|centos|rocky|almalinux)
      log "检测到 RHEL 系列,使用 dnf 安装 Node.js 20."
      $SUDO dnf install -y curl
      curl -fsSL https://rpm.nodesource.com/setup_20.x | $SUDO bash -
      $SUDO dnf install -y nodejs
      ;;
    arch|manjaro)
      log "检测到 Arch 系列,使用 pacman 安装 Node.js."
      $SUDO pacman -Sy --noconfirm --needed nodejs npm
      ;;
    alpine)
      log "检测到 Alpine,使用 apk 安装 Node.js."
      $SUDO apk add --no-cache nodejs npm
      ;;
    *)
      err "暂不支持的系统: ${ID:-unknown}."
      err "请从 https://nodejs.org/en/download 手动安装 Node.js 20+,然后再运行此脚本."
      exit 78
      ;;
  esac
fi

# Re-check after the install.
if ! node_major_ge "$(node_major)" 20; then
  err "Node.js 20+ 未安装成功,中止."
  exit 1
fi

log "安装 openpanel-menu (npm install -g)."
$SUDO npm install -g openpanel-menu@latest

if command -v opctl >/dev/null 2>&1; then
  ok "opctl 已就绪. 运行 'opctl' 进入菜单."
else
  err "opctl 未出现在 PATH. 请检查 npm 全局 bin 目录."
  exit 1
fi
