## Why

OpenPanel protects application secrets but has no host firewall or login-abuse controls. A server panel must safely expose network policy and panel brute-force defense without letting an operator lock out the panel or SSH accidentally.

## What Changes

- Add host firewall status and managed inbound allow/deny rules through an nftables adapter.
- Add panel-login rate limiting, temporary IP/account blocks, allowlists, and security events.
- Add a security posture summary, REST/CLI/web management, audit, and rollback safeguards.
- Defer SSH daemon configuration, WAF, malware scanning, and OS hardening.

## Capabilities

### New Capabilities

- `host-security`: firewall policy and panel authentication abuse protection.

### Modified Capabilities

None.

## Impact

Adds a bounded context, migrations, nftables command adapter, identity middleware integration, API/CLI/web surfaces, and privileged deployment requirements.
