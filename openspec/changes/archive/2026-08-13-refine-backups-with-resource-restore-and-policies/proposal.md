# Refine backups with resource-scoped restore and remote-target policy

## Why

`openspec/specs/backups/spec.md` defines backup plans, runs,
manifests, integrity, secret safety, and restore lifecycle. It
does not pin **per-resource restore** (single site, single
database, single mail domain, single mailset) or a **remote-target
policy** distinguishing local from off-site destinations. cPanel
and Baota both let an operator restore a single mailbox or a
single site without rebuilding the whole host. Off-site backups
are typically a separate capability from local backups. This
refinement adds the formal hooks; the storage layer lands in the
follow-on `add-offsite-backup-targets` change.

## What Changes

- New `ResourceKind` enum on every plan and every restore
  request: `Site | Database | MailDomain | Mailbox | AuditLog |
  Configuration`.
- New `RestoreScope { ResourceKind, Selector }` value object
  with kind-specific selectors (site id, db name, mailbox email,
  time range).
- New `BackupTargetKind` enum: `Local`, `OffsiteS3`,
  `OffsiteRsync`, `OffsiteB2`, `OffsiteWasabi`.
- `backup_plan.target_kind` field recorded at plan creation.

## Capabilities

### Modified Capabilities

- `backups`: typed restore scope; typed target kind; no
  storage impact in this change.

## Impact

- Domain: `ResourceKind`, `RestoreScope`, `BackupTargetKind`.
- App: `BackupsService::create_plan` accepts
  `target_kind`; restore endpoints accept `RestoreScope`.
- Storage: migration adds `target_kind` to `backup_plans`.
