# Bilateral form ↔ source config editor — Design notes

Status: design notes for a future phase. The current config editor
(`config.rs` in `openpanel-app`) only exposes a raw textarea. This
note captures the analysis behind the "form fields that stay in sync
with the source file, with comments preserved" feature, so the
implementation can pick this up without re-deriving the trade-offs.

## Why this is hard

The user wants each managed config file to be editable in **two ways**
that must stay perfectly consistent:

1. A **grouped form** (per-section fields, the Baota-style UX).
2. A **source textarea** (the raw config file verbatim).

Editing either side must update the other side live, and saving must
write the source file on disk without losing comments, blank lines,
unknown keys, or unknown sections.

The naive approach — re-serialise a structured model from the form —
loses comments and unknown content. That is the failure mode the user
flagged ("the comments of the source file in the textarea should not
remove these comments").

## The global strategy (text ↔ JSON ↔ form)

A small, reusable, two-layer pipeline:

```text
source text  ---parser--->  canonical JSON model  ---renderer--->  grouped form
source text  <--serializer--  canonical JSON model  <--form edits--  grouped form
```

The **canonical JSON model** is the source of truth. It is a
**preserving** intermediate: it carries comments, blank lines,
unknown keys, and unknown sections alongside the modeled fields. The
form renderer reads the modeled subset; the serializer reads the
whole thing.

This is the "global strategy" the user described: "the configuration
are a well structured text which may translated to a JSON. and the
JSON can be then thus translated to a grouped fields (such as one
section of the ini file may be grouped toggether as a grouped
fields)".

### Why one neutral model instead of per-component schemas

A per-component hard-coded schema (one struct + form for fail2ban,
one for nginx, …) does not generalise. The global strategy is:

* **One neutral JSON model** for any structured-text file.
* **One generic grouped-form renderer** that turns the JSON model
  into inputs (sections → fieldsets, fields → typed inputs by value
  heuristic).
* **One serializer per file dialect** (INI, nginx-block, …) that
  takes the JSON model plus the original text and emits preserving
  text.
* **One parser per file dialect** that yields the JSON model from
  source text.

The form/grouping logic is shared. The text-dialect adapters are
the only per-component code. New components drop in by adding a
small dialect parser/serializer and a default JSON model.

## Dialect scope for the first cut

The managed components split naturally by dialect:

| Dialect         | Files                                                          |
| --------------- | -------------------------------------------------------------- |
| INI             | jail.local, php.ini, redis.conf, mysqld.cnf, 50-server.cnf     |
| nginx block DSL | nginx.conf                                                     |

**First cut:** the INI dialect + the generic grouped-form renderer.
That covers 5 of the 6 non-nginx files and is the right complexity
for one phase. jail.local is the showcase because it is the file
that almost never exists on a fresh install — the blank-textaer
problem is real there.

**Follow-up:** the nginx block dialect. nginx.conf is a nested
directive-in-`{ }` format; the JSON model still represents it
(sections → blocks, fields → directives), only the parser/serializer
changes. The grouped-form renderer is reused.

php.ini, redis.conf, mysqld.cnf, and 50-server.cnf are all INI and
inherit the jail.local work for free; each only needs a default JSON
model and a typed-field heuristic.

## The preserving JSON model (INI dialect)

The JSON model is the file's AST plus a thin schema overlay. It
preserves the original text byte-for-byte for everything the schema
does not model, and stores original line ordering for fields inside
modeled sections.

```json
{
  "dialect": "ini",
  "sections": [
    {
      "name": "DEFAULT",
      "leading": ["# Ban hosts for 1 hour", ""],
      "fields": [
        { "key": "bantime",  "value": "2h", "leading": [] },
        { "key": "findtime", "value": "10m", "leading": [] },
        { "key": "maxretry", "value": "5",  "leading": [] }
      ],
      "trailing": [""]
    },
    {
      "name": "sshd",
      "leading": ["# SSH jail"],
      "fields": [
        { "key": "enabled", "value": "true",  "leading": [] },
        { "key": "port",    "value": "ssh",   "leading": [] },
        { "key": "filter",  "value": "sshd",  "leading": [] },
        { "key": "logpath", "value": "/var/log/auth.log", "leading": [] }
      ],
      "trailing": []
    }
  ],
  "tail": []
}
```

Per-section:

* `leading` — verbatim lines BEFORE the `[name]` header (comments,
  blanks). Preserved on serialize.
* `fields` — ordered list of modeled keys with their current value
  and any per-field leading comments. Order is preserved across
  edits so the textarea regenerated from the form does not shuffle
  unrelated lines.
* `trailing` — verbatim lines AFTER the last modeled field of the
  section, before the next section header. This is where unknown
  keys, comments, and blank lines inside a known section live. They
  are preserved as raw lines; the form ignores them, the textarea
  shows them, save keeps them.

`sections` is an ordered list (preserves original section order).
`tail` preserves content after the last section.

### Round-trip guarantees

For any file whose dialect is INI:

1. `parse(text)` → AST → `serialize(AST, original_text)` yields
   `text` for the comments/unknowns/whitespace that the schema does
   not model. Modeled values are re-emitted in the original order
   with the new values.
2. Editing a modeled field, then serialising, preserves every
   comment and every unknown line. The user-visible file never
   loses information.
3. Editing the textarea (comments, unknown keys, manual reorderings)
   survives the next form-driven serialisation, because the parser
   re-extracts the file into the AST and the serializer re-emits
   verbatim.

### What the schema is and is not

The schema is a **thin overlay** for the form renderer:

```rust
pub struct ComponentSchema {
    pub component: &'static str,
    pub default_model: fn() -> IniModel,    // for the blank-file case
    pub modeled_sections: &'static [&'static str],
    pub modeled_fields: &'static [(&'static str, &'static [&'static str])],
}
```

It does NOT encode the JSON model. The JSON model is dialect
generic. The schema only tells the form renderer which
`(section, key)` pairs to render as inputs.

Consequence: a section the schema does not model is preserved
verbatim by the AST regardless. Adding a new modeled field later
does not break old files.

## Solving the blank-textarea problem (the original motivation)

The user opened the change with: "if the config file is blank, i
don't want to edit a blank textarea form".

With the preserving JSON model, the fix is generic and lives at the
model layer:

1. Each component has a `default_model` — a curated AST with the
   default values and helpful comments.
2. `read_config` returns the AST (and the rendered source text) from
   the **default model** when the file is missing. The `exists: false`
   flag is preserved.
3. The form renders the default AST. The textarea shows the
   serialized default (with helpful comments).
4. The user edits the form. The textarea updates live. Save creates
   the file with the user's edits plus the defaults' comments.

This is the global solution. There is no "blank textarea" special
case in the UI; the model is always non-empty.

## Bilateral sync on the client (vanilla JS, no framework)

The panel's stack is Rust + htmx + minimal vanilla JS. The user
referenced Vue/React `v-model` style bilateral binding; the right
fit here is a small vanilla JS controller per config page, not a
framework introduction.

### What the server renders

The server renders, from the AST:

* A `<form>` with `<fieldset>` per modeled section, `<legend>` for
  the section name, and an `<input>` per modeled field. Each input
  carries `data-section="<name>"` and `data-key="<key>"`.
* A `<textarea name="content">` whose initial value is the
  serialized AST (file content if it exists, else the default).
* A `<script type="application/json" id="config-model">` embedding
  the canonical JSON model. The JS controller reads this as the
  single source of truth on the client.

### What the client controller does (~50–80 lines)

* On `input` from a form field: locate the field in the JSON model
  by `data-section` / `data-key`, update its value, re-serialise the
  AST to text, assign to the textarea. The textarea is the
  authoritative text for save.
* On `input` from the textarea: lightweight section-aware re-parse
  (split by lines, track the current section by `[name]` headers,
  find the modeled key in the current section, update the JSON
  model and the corresponding form input). Comments, blanks, and
  unknown lines are simply not interpreted — they stay in the textarea
  and in the JSON model's `leading` / `trailing` / `tail` slots (the
  server re-parses on save to rebuild the AST properly).

The client's re-parse is a **best-effort live preview**, not the
authoritative parser. The authoritative parse happens on the server
on save (and on page load). This keeps the JS controller tiny and
aligned with the panel's no-framework philosophy.

### Save flow

The save POST sends the textarea content (the source of truth for
the user). The server reads the textarea content, parses it into the
AST (re-applying any comments / unknowns the user added in the
textarea that the live JS couldn't round-trip), and writes the file
through the existing `save_config` atomic-write path. The form's
value updates don't need to round-trip through the server — the
textarea already carries them.

This is the lightest-weight server change: `save_config` already
takes a string and writes it atomically. The only new step is
"re-parse on save to keep the AST consistent", which is a service
helper, not a new endpoint.

## Test plan for the future phase

* **Parser**: `parse(serialize(parse(text))) == parse(text)`. Golden
  INI files with comments, blanks, unknown sections, unknown keys
  inside known sections, and trailing blanks round-trip identically.
* **Serializer**: known fields are updated; unknown lines come back
  unchanged byte-for-byte.
* **Blank-file**: missing file → default model renders a populated
  form and a non-empty textarea whose initial text equals
  `serialize(default_model)`.
* **Form → textarea**: form edit mutates the textarea text in real
  time and preserves every comment.
* **Textarea → form**: adding a comment in the textarea does not
  destroy it; editing a modeled key in the textarea updates the
  form field.
* **Save**: writes the textarea content through the atomic write
  path; the previous file is preserved on validation failure.

## Open questions for the future phase

* **Multi-file**: the manifest already supports `files[1..]`. The
  grouped editor should expose them as additional tabs/fields in the
  same UI. The JSON model is per-file, so this is a tab wrapper.
* **Reload**: hook the existing `packages.reload(component)` (or a
  per-component reload command) after a successful save, gated by
  an `allow_reload` manifest flag. The reload command is a
  follow-up to the current change.
* **Schema growth**: when a schema gains a new modeled field, old
  files load fine because the AST preserves the unknown lines and
  the form just renders an additional input.
* **nginx dialect**: separate parser/serializer for the
  directive-in-`{ }` block format. The renderer and JSON-model
  shape generalise; only the dialect adapter changes.

## Why this is its own phase

The current config editor (textarea-only, atomic write, 128 KiB
ceiling, validation-restore) is a complete, shippable feature. The
bilateral form/textarea editor is a substantial UX layer on top of
it: a preserving parser/serializer, a JSON model, a grouped-form
renderer, and a vanilla JS controller. It earns its own change so
the design and review have room to breathe.
