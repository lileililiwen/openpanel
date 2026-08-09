## Context

Firewall changes are privileged and a malformed ruleset can sever access. Authentication failures currently have no durable throttling model. The design must be distro-aware and fail closed at the management boundary without silently replacing administrator-owned firewall rules.

## Goals / Non-Goals

**Goals:** manage an isolated OpenPanel nftables table, validate/apply/rollback transactions, preserve required access, throttle abusive panel login, and provide security event visibility.

**Non-Goals:** general-purpose firewall editing, cloud security groups, SSH configuration, WAF, intrusion prevention, antivirus, or automatic geographic blocking.

## Decisions

1. Own only table `inet openpanel`; import existing rules read-only. Generate a complete candidate, validate with `nft --check`, apply atomically, verify panel/SSH safety invariants, and rollback from the last-known-good file on failure.
2. Require at least one effective panel access path and protect the configured SSH/panel ports from deletion unless an Owner supplies a short-lived break-glass confirmation. A timed rollback watchdog confirms reachability after risky changes.
3. Model typed protocol, port/range, CIDR, direction, action, enabled state, and comment. Reject invalid/broad ambiguous values in the domain.
4. Rate-limit login by normalized account and trusted client IP using persisted exponential temporary blocks. Proxy headers are trusted only from configured proxy CIDRs. Successful login reduces account penalties; allowlists never bypass password checks.
5. Security events and firewall mutations are append-only and redact submitted credentials.

## Risks / Trade-offs

- Lockout -> last-known-good snapshot, protected ports, preview, watchdog rollback, and documented console recovery.
- nftables unavailable -> capability reports unsupported and performs no fallback shell edits.
- Distributed password spraying -> dual account/IP keys and bounded durable event retention.

## Migration Plan

Deploy login throttling first in observe-only mode, create but do not enable the nftables table, then require explicit Owner enablement after preview. Rollback removes only `inet openpanel` and disables middleware enforcement.
