#!/usr/bin/env bash
# packages/installer/smoke-container.sh — container smoke tests.
#
# Boots the OpenPanel image, asserts the /health endpoint, confirms
# the running UID is non-root, and verifies SIGTERM terminates the
# container cleanly within the deadline. CI runs this on every tag
# build; locally it requires a built image.

set -euo pipefail

IMAGE="${1:-openpanel:dev}"
DEADLINE_SECONDS="${DEADLINE_SECONDS:-30}"

if ! command -v docker >/dev/null 2>&1; then
    echo "skip - docker not available"
    exit 0
fi

if ! docker image inspect "${IMAGE}" >/dev/null 2>&1; then
    echo "skip - image ${IMAGE} not present locally"
    exit 0
fi

name="openpanel-smoke-$$"
trap 'docker rm -f "${name}" >/dev/null 2>&1 || true' EXIT

docker run --detach --name "${name}" -p 18080:8080 "${IMAGE}" >/dev/null

healthy=0
for _ in $(seq 1 "${DEADLINE_SECONDS}"); do
    sleep 1
    if curl --silent --fail --max-time 2 \
        http://127.0.0.1:18080/health >/dev/null 2>&1; then
        healthy=1
        break
    fi
done

if [ "${healthy}" -ne 1 ]; then
    echo "FAIL - /health never returned 200 within ${DEADLINE_SECONDS}s"
    docker logs "${name}" || true
    exit 1
fi
echo "ok   - /health returned 200 within ${DEADLINE_SECONDS}s"

uid="$(docker exec "${name}" id -u)"
if [ "${uid}" = "0" ]; then
    echo "FAIL - container is running as root (uid 0)"
    exit 1
fi
echo "ok   - container runs as uid ${uid} (non-root)"

start="$(date +%s)"
docker kill --signal SIGTERM "${name}" >/dev/null
for _ in $(seq 1 10); do
    if ! docker ps --filter "name=${name}" --format '{{.Names}}' \
        | grep -q "^${name}$"; then
        elapsed=$(( $(date +%s) - start ))
        if [ "${elapsed}" -le 10 ]; then
            echo "ok   - SIGTERM produced a clean exit in ${elapsed}s"
            exit 0
        fi
        echo "FAIL - container exited but took ${elapsed}s (>10s deadline)"
        exit 1
    fi
    sleep 1
done

echo "FAIL - SIGTERM did not stop the container within 10s"
docker kill --signal SIGKILL "${name}" >/dev/null 2>&1 || true
exit 1
