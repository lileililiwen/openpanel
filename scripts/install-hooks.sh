#!/usr/bin/env bash
# scripts/install-hooks.sh — install the git pre-commit hook.
#
# The pre-commit hook runs `make check` (fmt + clippy + docs + audit +
# test) before every commit. This is *informational* in v0.1: the
# install script refuses to clobber an existing hook and prints the
# instructions instead. Bypass the hook with `git commit --no-verify`.
set -euo pipefail

HOOK=.git/hooks/pre-commit
SAMPLE=.git/hooks/pre-commit.sample
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

if [[ -e "$HOOK" ]]; then
    cat <<EOF
Refusing to overwrite existing $HOOK.

To enable the OpenPanel quality gate manually, append the following
to your existing $HOOK (or merge it with whatever is already there):

    # >>> openpanel quality gate >>>
    if command -v make >/dev/null 2>&1; then
        make -C "$REPO_ROOT" check || exit 1
    fi
    # <<< openpanel quality gate <<<

Or to remove the existing hook and let this script install ours:

    rm "$HOOK"
    $0
EOF
    exit 1
fi

cat > "$HOOK" <<EOF
#!/usr/bin/env bash
# Installed by scripts/install-hooks.sh.
# Runs the OpenPanel quality gate before every commit. Bypass with
# \`git commit --no-verify\`.
set -e

if command -v make >/dev/null 2>&1; then
    make -C "$REPO_ROOT" check || exit 1
else
    echo "warning: make not found; skipping quality gate" >&2
fi
EOF
chmod +x "$HOOK"
echo "Installed $HOOK — pre-commit will now run \`make check\`."