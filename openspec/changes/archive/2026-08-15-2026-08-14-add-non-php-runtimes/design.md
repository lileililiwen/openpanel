# Add non-PHP runtimes — Design

## SiteRuntime model

```rust
pub enum RuntimeKind { Node, Python, Ruby, Go }

pub struct SiteRuntime {
    pub site_id: SiteId,
    pub kind: RuntimeKind,
    pub version: RuntimeVersion,   // e.g. "20", "3.12", "3.3", "1.22"
    pub app_port: AppPort,         // loopback-only, e.g. 31000+
    pub entry: PathBuf,            // entry file / command under chroot
    pub status: RuntimeStatus,     // Stopped | Running | Crashed
    pub unit_name: String,         // supervisor unit id
}
```

## Supervisor unit layout

```
/var/www/<domain>/.runtime/            runtime working dir (0700, owner-only)
/etc/supervisor/conf.d/openpanel-<site>.conf   per-user unit
```

The unit launches the runtime as the site user, cwd inside the chroot,
listening on `127.0.0.1:APP_PORT`. Supervisor performs auto-restart on
crash.

## Lifecycle flow

```
set_runtime(site_id, kind, version):
  validate version is in the allowed pin list
  write /etc/supervisor/conf.d/openpanel-<site>.conf
  supervisorctl reread && update
  regenerate nginx site: proxy_pass http://127.0.0.1:APP_PORT
  audit RuntimeChanged{kind, version}

control(site_id, action):   // start|stop|restart
  supervisorctl <action> openpanel-<site>
  sync RuntimeStatus

logs(site_id):
  supervisorctl tail -f openpanel-<site> (capped, no secrets)
```

## Endpoints

```
GET  /api/v1/sites/{id}/runtime      -> current runtime + status
PUT  /api/v1/sites/{id}/runtime      body { kind, version, entry? }
GET  /api/v1/sites/{id}/runtime/logs  query { action: tail|start|stop|restart }
```

## Tests

```
1.1 Unit: version-pin validation; supervisor unit render; nginx proxy
        block render (loopback-only port).
1.2 Property: app port never binds to a non-loopback address; working
        dir stays inside the site chroot.
1.3 Service tests w/ mock supervisor + mock nginx: set, start, stop,
        restart, status sync.
1.4 Integration: live unit starts app, nginx proxies to APP_PORT.
1.5 CLI E2E: set -> start -> logs -> stop.
1.6 Web: Runtime tab (CSRF), select+pin, control buttons, log view.
```
