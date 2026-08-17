# Refine sites with multi-PHP runtime, clone, and template export

## Why

`openspec/specs/sites/spec.md` covers site lifecycle. The README
notes that "full PHP support lands in v0.2" and `php_version` is
described as optional. cPanel and Baota expose **multiple installed
PHP runtimes with per-site selection** and ship **clone / template
export** so a site can be replicated safely. Without these,
OpenPanel's v0.1 sites capability stops short of commercial
parity. This refinement introduces the formal domain fields the
later per-site-php-runtime and clone changes need, with no
behavioural change yet for PHP swap; behaviour is added in the
follow-on `add-per-site-php-runtime` and `add-site-clone-and-template-export`
changes which depend on the fields introduced here.

## What Changes

- New site fields: `php_runtime` (typed `PhpRuntimeRef | None`),
  `clone_template_id` (`Option<Uuid>`), `instance_origin_id`
  (`Option<Uuid>`).
- New behavior: `Site::new` no longer rejects `php_enabled=true`
  in v0.2 once the value of `php_runtime` is set; in v0.1 the
  field is still allowed but the value MUST be `None`.
- `Site::clone(source_id, target_domain, new_owner_id)` returns a
  draft `Site` with `instance_origin_id = source.id` and is a
  no-op until `add-site-clone-and-template-export` lands.

## Capabilities

### Modified Capabilities

- `sites`: `php_runtime`, `clone_template_id`, `instance_origin_id`
  fields; placeholder `Site::clone` API.

## Impact

- Domain: `Site` aggregate gains three fields; new
  `Site::clone` no-op helper.
- App: SQLite migration adds the three columns as nullable; repo
  functions `find_by_clone_template` and `find_by_origin` declared
  with empty default impl until follow-on changes.
- API/CLI/web: no endpoint change in this change; documentation
  updates only.
