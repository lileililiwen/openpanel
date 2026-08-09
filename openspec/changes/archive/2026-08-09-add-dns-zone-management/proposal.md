## Why

Provisioning a site does not make its domain resolve, and ACME/mail workflows depend on correct DNS. OpenPanel needs provider-backed zone management before self-hosted authoritative DNS, which would add substantial availability and delegation risk.

## What Changes

- Add DNS provider accounts, zone/record synchronization, record CRUD, drift status, and propagation checks.
- Support A, AAAA, CNAME, TXT, MX, CAA, NS, and SRV records with type-specific validation.
- Integrate optional site record creation and DNS-01/verification workflows through a provider port.
- Add REST, CLI, web UI, encrypted credentials, audit, and concurrency protection.

## Capabilities

### New Capabilities

- `dns`: external-provider DNS zones, records, synchronization, and verification.

### Modified Capabilities

None.

## Impact

Adds a bounded context, migrations, provider adapters (starting with RFC 2136 or Cloudflare), encrypted credential storage, HTTP/CLI/web surfaces, and optional sites/SSL integration.
