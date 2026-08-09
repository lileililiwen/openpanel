# Firewall recovery

OpenPanel owns only the nftables table `inet openpanel`. It does not flush or
rewrite distribution, container, cloud-agent, or administrator-managed tables.

If panel or SSH access is interrupted, use the provider console or another
out-of-band terminal and run:

```sh
openpanel security rollback
```

If the OpenPanel binary cannot start, inspect the saved rules under the
configured `OPENPANEL__SECURITY__STATE_DIR` (the default is
`.openpanel-firewall` relative to the service working directory). Restore the
last-known-good file with nft directly:

```sh
nft --check -f .openpanel-firewall/last-good.nft
nft -f .openpanel-firewall/last-good.nft
```

As a final recovery measure, remove only OpenPanel's isolated table:

```sh
nft delete table inet openpanel
```

Never use `nft flush ruleset` for OpenPanel recovery; that also removes foreign
firewall policy. After access is restored, inspect the candidate with
`openpanel security preview`, correct the offending rule, and apply again.
