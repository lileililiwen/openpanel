# openpanel-menu

The Baota-style numbered-menu CLI for OpenPanel operators.

`opctl` (canonical) and `openpanel-menu` (long alias) print a
seven-item menu and, for each choice, either delegate to
`openpanel-cli` or talk to systemd + `/etc/openpanel/openpanel.toml`
directly.

## Install

### npm

```sh
npm install -g openpanel-menu
opctl --version
```

`opctl` and `openpanel-menu` are both on `PATH` after install.

### curl | bash

```sh
curl -fsSL https://openpanel.dev/install.sh | bash
opctl
```

The installer bootstraps Node.js 20+ via the system package
manager (apt / dnf / pacman / apk) and then runs
`npm install -g openpanel-menu`.

## Menu items

| id | action             |
|----|--------------------|
| 1  | restart panel      |
| 2  | stop panel         |
| 3  | start panel        |
| 4  | change panel port  |
| 5  | change admin password |
| 6  | show info          |
| 7  | upgrade            |
| 0  | exit               |

## Requirements

- Node.js >= 20
- A running OpenPanel install at `/etc/openpanel/openpanel.toml`
  for items 4 and 5.
- `openpanel-cli` on `PATH` for item 5 (and the upgrade sub-actions).
- `systemctl` on `PATH` for items 1, 2, 3, 4, 6, 7.

## License

MIT — see [LICENSE](../../LICENSE).
