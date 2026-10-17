// Audit d'accessibilité (axe-core, règles WCAG 2.1 A et AA) de chaque écran.
//   node scripts/audit-a11y.mjs [rapport.json]
import { chromium } from "playwright-core";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const axeSrc = fs.readFileSync(require.resolve("axe-core/axe.min.js"), "utf8");
const out = process.argv[2] ?? path.join(os.tmpdir(), "aura-a11y.json");
const work = fs.mkdtempSync(path.join(os.tmpdir(), "aura-a11y-"));
const copy = (n) => {
  const p = path.join(work, n);
  fs.copyFileSync(path.resolve("tests/fixtures", n), p);
  return p;
};
const report = copy("Rapport Q1 2025.pdf");
const form = copy("Demande d’ouverture.pdf");

const browser = await chromium.connectOverCDP(`http://127.0.0.1:${process.env.SHOT_PORT ?? 9222}`);
const page = browser.contexts()[0].pages().find((p) => !p.url().startsWith("devtools"));
const wait = (ms) => page.waitForTimeout(ms);
const app = (fn, arg) => page.evaluate(fn, arg);

async function audit(name) {
  await wait(900);
  await page.evaluate(axeSrc);
  const res = await page.evaluate(async () => {
    const r = await window.axe.run(document, { runOnly: { type: "tag", values: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"] }, resultTypes: ["violations"] });
    return r.violations.map((v) => ({ id: v.id, impact: v.impact, help: v.help, nodes: v.nodes.slice(0, 6).map((n) => ({ target: n.target.join(" "), summary: n.failureSummary?.split("\n").slice(0, 3).join(" | ") })), count: v.nodes.length }));
  });
  console.log(`\n=== ${name} : ${res.length} règle(s) en défaut`);
  for (const v of res) console.log(`  [${v.impact}] ${v.id} ×${v.count} — ${v.help}\n      ${v.nodes.map((n) => n.target).join("\n      ")}`);
  return { name, violations: res };
}

const results = [];
await app(() => {
  const s = window.__aura.getState();
  for (const t of [...s.tabs]) s.closeTab(t.key);
  s.goHome();
});
results.push(await audit("Accueil"));
await app(() => window.__aura.getState().set({ create: { open: true, source: "blank", images: [] } }));
results.push(await audit("Créer un PDF"));
await page.keyboard.press("Escape");
await app(() => window.__aura.getState().set({ paletteOpen: true }));
results.push(await audit("Palette de commandes"));
await page.keyboard.press("Escape");

await app(async (p) => {
  await window.__aura.getState().openPath(p);
  window.__aura.getState().setModule("lire");
}, report);
results.push(await audit("Lecture"));
await app(() => window.__aura.getState().set({ printOpen: true }));
await wait(1500);
results.push(await audit("Impression"));
await page.keyboard.press("Escape");
for (const [mod, name] of [
  ["annoter", "Annoter"],
  ["editer", "Éditer"],
  ["signer", "Signer"],
  ["securite", "Sécurité"],
  ["fusion", "Fusionner"],
  ["compress", "Compresser"],
  ["convert", "Convertir"],
]) {
  await app((m) => window.__aura.getState().setModule(m), mod);
  results.push(await audit(name));
}
await app(() => window.__auraConvert.setState({ tab: "ocr" }));
results.push(await audit("OCR"));
await app(() => window.__auraConvert.setState({ tab: "conv" }));
await app(async (p) => {
  await window.__aura.getState().openPath(p);
  window.__aura.getState().setModule("forms");
}, form);
results.push(await audit("Formulaires"));

fs.writeFileSync(out, JSON.stringify(results, null, 2));
const total = results.reduce((n, r) => n + r.violations.length, 0);
console.log(`\nTotal : ${total} règle(s) en défaut sur ${results.length} écrans. Rapport : ${out}`);
await browser.close();
