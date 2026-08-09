# Design: Add Web UI — Sites

## Context

The `web-ui` foundation provides the shell, `WebUser` gating, CSRF, and the web
router. `SitesService` exposes `list_sites`, `get_site`, `create_site`,
`delete_site`, `enable_site`, `disable_site`, `change_owner`, `change_aliases`.
Sites also power files and SSL, so the detail page links to those sections.

## Decisions

### 1. List + detail pages, HTMX-swapped actions

**Decision**: `GET /sites` renders a table. Actions on rows use HTMX
(`hx-post`/`hx-delete` with `hx-target` the table/row) so enable/disable/delete
swap in place. Create posts to `POST /sites` and swaps the refreshed list.

**Rationale**: Consistent with the shell's "no app JS" model — all
interactivity is attribute-driven.

### 2. Delete requires confirmation

**Decision**: `DELETE /sites/{id}` renders a confirmation dialog (a small HTML
`<dialog>` opened by a checkbox/button) before the actual delete request.
Deleting also removes the nginx config and document root is left intact
(matching the service's contract).

**Rationale**: Deleting a site is destructive and irreversible; a one-click
HTMX delete is too easy to hit by accident.

### 3. RBAC respected

**Decision**: Actions render only when the caller's role allows them — the
handlers check `caller.role().can_manage_sites()` exactly as the service does;
`list_sites(caller)` already scopes visibility. Non-owners see their own
sites only and no create/delete buttons.

**Rationale**: The web layer must not widen the API's authorization
boundary — it renders what the caller may do.

### 4. Errors inline, not page death

**Decision**: Service errors (duplicate domain, forbidden) render as an
inline alert region swapped into the form, not a 500 page. Validation errors
on the form are listed in the same region.

**Rationale**: Operators fix typos in the form; a hard failure hides the
cause.

## Security

- All state-changing routes carry and validate the CSRF token.
- Site names/domains are rendered through `maud` escaping; document roots and
  paths are never rendered raw.

## Test strategy

- Unit: list rendering (rows + action buttons per role), create form fields,
  detail rendering, error/alert rendering.
- Integration via `TestServer`: list empty state, create → appears in list,
  duplicate domain shows inline error, enable/disable toggles, delete with
  confirmation, unauthenticated redirect, forbidden role hides actions.

## Notes

- No new dependencies. Reuses `SitesService` and the shell.
