# Add themeable UI and white-label — Design

## Override model

```rust
pub struct ThemeOverride {
    pub owner_id: UserId,                   // the reseller
    pub brand_name: String,
    pub logo_path: String,                  // /srv/branding/<owner>/logo.svg
    pub palette: Palette,
    pub typography: Typography,
    pub default_radius: u16,
    pub panel_domain: Option<PanelDomain>,
}

pub struct Palette {
    pub color_bg: String,
    pub color_fg: String,
    pub color_accent: String,
    pub color_muted: String,
    pub color_success: String,
    pub color_warn: String,
    pub color_danger: String,
    pub contrast_min: f32,                  // default 4.5 (WCAG AA)
}

pub struct Typography {
    pub font_body: String,
    pub font_mono: String,
    pub size_base_px: u16,
}

pub struct PanelDomain {
    pub fqdn: String,                       // "panel.example.com"
    pub tls: TlsRef,                        // cert ref (ssl cap)
}
```

The palette values are CSS color literals (`#hex`, `rgb()`,
`oklch()`) validated server-side; the panel enforces the
`contrast_min` between `color_fg` and `color_bg` by computing
the contrast ratio and refusing values below 4.5 (WCAG AA).

## Request routing

```
nginx -> tls -> openpanel-api
  Host header → ThemeResolver.resolve(host)
    PanelDomain matched: theme = override
    No panel domain:    theme = system default
```

The shell template includes a per-request `<style>` block
generated from the resolved theme; CSS variables in templates
inherit the values. The override is never persisted into the
master `tokens.css`.

## Hosting-plan coupling

```rust
pub enum BrandingScope { Disabled, Reseller, System }
```

A plan MAY set `BrandingScope=Reseller` to grant the user
branding capability; `System` is reserved for the panel's
own themes and cannot be set from the API.

## Endpoints

```
GET    /api/v1/admin/branding
PUT    /api/v1/admin/branding
PUT    /api/v1/admin/branding/logo          multipart upload (≤ 256 KiB SVG/PNG)
PUT    /api/v1/admin/branding/palette       body: Palette
PUT    /api/v1/admin/branding/panel-domain  body: { fqdn }   requires existing cert
```

## CLI

```
openpanel branding show
openpanel branding set --brand-name <s> --logo <path>
openpanel branding palette --bg <hex> --fg <hex> --accent <hex>
openpanel branding panel-domain set --fqdn <d> --cert-id <c>
openpanel branding panel-domain clear
```

## Tests

```
1.1  Unit: contrast ratio computation; logo mime validation;
      fqdn normalisation.
1.2  Property: contrast_min is enforced; theme override never
      modifies master tokens.css.
1.3  Service tests: PUT/GET/clear; palette validation; logo
      upload and serving path.
1.4  Integration: override applied per host header; default
      theme for unknown host.
1.5  CLI E2E: set / palette / clear.
1.6  Web: /admin/branding editor (CSRF); live preview of
      palette change.
```
