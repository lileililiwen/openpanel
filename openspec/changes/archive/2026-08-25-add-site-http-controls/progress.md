# Progress — add-site-http-controls

**Goal:** per-site HTTP controls (error pages, redirects, protected
dirs, hotlink, IP rules, MIME, index policy) as a new
`site-http-controls` capability.

**Approach:** domain aggregate + pure renderer (WAF compiler pattern),
SQLite JSON document repo, service writing 0600 htpasswd files then
applying through the generator's insert-before-first-location
pipeline, REST + CLI surfaces.

**Done:** domain (14 unit/property tests), app renderer + repo +
service + module (4 renderer tests), REST route wired into router,
CLI `site-http {show,set}`, CLI E2E test, 3 integration tests
(round-trip + rendering + secret hygiene, escape/loop rejection,
owner-only). All quality gates green; full workspace suite passes.

**Done (session 2):** web UI tab shipped (`/sites/{id}/http`, CSRF
form, sub-nav link) with a full integration test; sandboxed smoke
assertions cover directive placement and htpasswd permissions. All
gates green. Screenshots at three breakpoints remain a PR-description
step for the human reviewer; live-nginx `-t`/reload requires an nginx
host. Ready to archive.
