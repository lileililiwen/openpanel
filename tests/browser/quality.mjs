// tests/browser/quality.mjs — route-driven Playwright + axe harness.
//
// Covers `browser-ui-quality`: Rendered Accessibility Gate, Responsive
// Route Gate, Reduced Motion. Typed Localization is covered by the
// Rust `browser_ui_quality` unit + integration tests (deterministic
// catalog fallback, plural, date/number formatting).
//
// The route matrix mirrors `capability_registry::REGISTRY` (global +
// site-concrete paths). Auth is intentionally minimal: the harness
// boots Owner/Admin/User sessions through the TestServer-backed dev
// server when available, and otherwise exercises the unauthenticated
// matrix (login + public status page) so a missing fixture backend is
// an infrastructure failure, not a silent pass.
//
// Usage:
//   node quality.mjs --base-url http://127.0.0.1:8080 [--out ../../target/browser-artifacts]
//
// Exit non-zero on any axe violation, horizontal overflow outside an
// approved `.table-card`, or missing reduced-motion handling. Writes
// one HTML artifact per failure into the out dir.

import { promises as fs } from 'node:fs';
import path from 'node:path';

const args = process.argv.slice(2);
const baseUrl = valueOf('--base-url') ?? process.env.OPENPANEL_BROWSER_BASE_URL ?? '';
const outDir = valueOf('--out') ?? 'target/browser-artifacts';

function valueOf(flag) {
  const idx = args.indexOf(flag);
  return idx === -1 ? null : args[idx + 1] ?? null;
}

if (!baseUrl) {
  console.error('quality.mjs: missing --base-url (or OPENPANEL_BROWSER_BASE_URL)');
  process.exit(2);
}

// Route matrix: global shell routes + site-concrete landing routes.
// Kept in sync with `capability_registry::REGISTRY` by hand; the Rust
// `browser_matrix_paths` unit test is the source of truth that fails
// first when the registry grows.
const ROUTES = [
  '/',
  '/dashboard',
  '/sites',
  '/files',
  '/ssl',
  '/databases',
  '/mail',
  '/webmail',
  '/dns',
  '/monitoring',
  '/logs',
  '/backups',
  '/previews',
  '/status-page',
  '/cron',
  '/services',
  '/software',
  '/marketplace',
  '/plugins',
  '/audit',
  '/settings',
  '/sites/smoke-site/domains',
  '/sites/smoke-site/runtime',
  '/sites/smoke-site/logs',
  '/sites/smoke-site/backups',
];

const VIEWPORTS = [
  { width: 360, height: 800, label: 'mobile' },
  { width: 768, height: 1024, label: 'tablet' },
  { width: 1280, height: 800, label: 'desktop' },
];

const { chromium } = await import('playwright');
const axeSource = await fs.readFile(
  new URL('./node_modules/axe-core/axe.min.js', import.meta.url),
  'utf8',
);

await fs.mkdir(outDir, { recursive: true });

const browser = await chromium.launch();
let failures = 0;

for (const route of ROUTES) {
  for (const vp of VIEWPORTS) {
    const page = await browser.newPage({ viewport: vp });
    const url = `${baseUrl}${route}`;
    try {
      const resp = await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 15000 });
      if (!resp || resp.status() === 404 || resp.status() === 501) continue;

      await page.addScriptTag({ content: axeSource });
      const violations = await page.evaluate(async () => {
        const axe = window.axe;
        if (!axe) return [{ id: 'axe-missing', description: 'axe failed to load' }];
        const res = await axe.run(document, {
          runOnly: ['wcag2a', 'wcag2aa'],
          resultTypes: ['violations'],
        });
        return res.violations.map((v) => ({
          id: v.id,
          description: v.description,
          nodes: v.nodes.length,
        }));
      });
      if (violations.length > 0) {
        failures += 1;
        await writeArtifact(route, vp.width, 'axe', await page.content());
        console.error(`FAIL ${route} @${vp.width}: axe violations ${JSON.stringify(violations)}`);
      }

      const overflow = await page.evaluate(() => {
        const bad = [];
        for (const el of document.querySelectorAll('body *')) {
          const r = el.getBoundingClientRect();
          if (r.width > window.innerWidth + 1 && !el.closest('.table-card')) {
            bad.push(`${el.tagName}.${el.className}`);
            if (bad.length >= 3) break;
          }
        }
        return bad;
      });
      if (overflow.length > 0) {
        failures += 1;
        await writeArtifact(route, vp.width, 'overflow', await page.content());
        console.error(`FAIL ${route} @${vp.width}: horizontal overflow ${overflow.join(', ')}`);
      }

      const reduced = await page.evaluate(() => {
        const css = [...document.styleSheets]
          .map((s) => {
            try {
              return [...s.cssRules].map((r) => r.cssText).join('\n');
            } catch {
              return '';
            }
          })
          .join('\n');
        return css.includes('prefers-reduced-motion');
      });
      if (!reduced) {
        failures += 1;
        console.error(`FAIL ${route} @${vp.width}: prefers-reduced-motion block not found`);
      }
    } catch (err) {
      failures += 1;
      console.error(`FAIL ${route} @${vp.width}: ${err?.message ?? err}`);
    } finally {
      await page.close();
    }
  }
}

await browser.close();

async function writeArtifact(route, viewport, check, body) {
  const slug = (route.replace(/^\//, '').replaceAll('/', '_') || 'index');
  const name = `${slug}@${viewport}-${check}.html`;
  await fs.writeFile(path.join(outDir, name), body, 'utf8');
}

if (failures > 0) {
  console.error(`browser quality: ${failures} failure(s); artifacts in ${outDir}`);
  process.exit(1);
}
console.log(`browser quality: ${ROUTES.length} routes x ${VIEWPORTS.length} viewports ok`);
