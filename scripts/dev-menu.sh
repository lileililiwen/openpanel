#!/usr/bin/env bash
# Dev wrapper: runs the menu against packages/openpanel-menu/ in
# development without publishing to npm. Builds the TypeScript
# and runs the compiled `dist/index.js` so the test matrix is the
# same as production.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PKG_DIR="${REPO_ROOT}/packages/openpanel-menu"

cd "${PKG_DIR}"
if [ ! -d node_modules ]; then
  npm install
fi
npm run build
exec node ./dist/index.js "$@"
