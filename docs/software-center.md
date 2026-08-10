# Software Center

OpenPanel's Owner-only Software Center is available at `/software` and through
`openpanel software`. It is a curated control-panel catalog, not a general
distribution package browser and never executes catalog-provided shell text.

## Supported host packages

The production APT adapter recognizes Ubuntu 22.04/24.04 and Debian 12 on
x86-64 or ARM64. Every catalog entry declares a narrower platform matrix; an
entry is shown as `unsupported` when its fixed packages are not provided by the
host's approved repositories. The recovery catalog includes Nginx, PHP-FPM and
CMS extensions, MySQL, MariaDB, and Redis. MySQL and MariaDB are mutually
exclusive.

From the panel, open **Administration → Software Center**, review the detected
state, select an action, inspect the immutable package plan, and confirm it.
Externally installed packages must be adopted explicitly before OpenPanel will
update or remove them. Removal is refused while managed sites or databases
depend on a component.

Equivalent CLI operations are:

```text
openpanel software catalog
openpanel software inventory
openpanel software preview --id redis
openpanel software install --id redis
openpanel software adopt --id nginx
openpanel software update --id redis
openpanel software uninstall --id redis
openpanel software jobs
openpanel software diagnostics
```

Previews expire after five minutes and their confirmation token can be used
once. `software execute --digest ... --confirmation-token ...` can execute a
preview created by a previous CLI process. Active jobs can be cancelled at a
safe checkpoint; interrupted component installations offer fresh retry and
recipe-supported rollback flows.

## WordPress and Drupal

WordPress 7.0.3 and Drupal 11.3.12 are pinned in the recovery catalog. Artifact
downloads are HTTPS-origin allowlisted, redirect-free, bounded, and checked
against the pinned upstream digest before extraction. Extraction occurs only
in a new staging directory and rejects absolute/parent paths, links, device
entries, excessive expansion, excessive file counts, and unexpected archive
roots.

Application deployment is shown as available only when OpenPanel is composed
with an application deployment adapter. Development and end-to-end tests use
the deterministic adapter by setting `OPENPANEL__SOFTWARE__ADAPTER=fake`; do
not use that adapter on a production server. Generated administrator
credentials appear only in the successful deployment response and are absent
from jobs, logs, audits, and deployment records.

## Recovery and security

Package transactions are serialized in-process and through a PID-aware durable
SQLite lock. A restart reconciles abandoned active jobs to `interrupted`
without taking a lock from a live panel process. Plans and jobs are durable;
confirmation tokens are stored only as SHA-256 hashes. Diagnostic output is
bounded and secret-bearing lines are redacted.

Remote catalog snapshots are data-only JSON signed with the embedded Ed25519
trust root. OpenPanel rejects non-canonical payloads, unknown fields, invalid
signatures, unsupported schemas, oversized manifests, and expired snapshots.
Activation is atomic, so a rejected refresh leaves the last-known-good catalog
active.
