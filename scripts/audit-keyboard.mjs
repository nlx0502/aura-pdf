// Audit clavier : parcours à la touche Tab de chaque écran ; chaque élément
// atteint doit avoir un nom accessible et un indicateur de focus visible.
//   node scripts/audit-keyboard.mjs
import { chromium } from "playwright-core";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const work = fs.mkdtempSync(path.join(os.tmpdir(), "aura-kbd-"));
const report = path.join(work, "Rapport.pdf");
fs.copyFileSync(path.resolve("tests/fixtures/Rapport Q1 2025.pdf"), report);
const browser = await chromium.connectOverCDP(`http://127.0.0.1:${process.env.SHOT_PORT ?? 9222}`);
const page = browser.contexts()[0].pages().find((p) => !p.url().startsWith("devtools"));
const wait = (ms) => page.waitForTimeout(ms);
let problems = 0;

async function walk(name, steps = 200) {
  await wait(800);
  await page.evaluate(() => document.activeElement?.blur?.());
  const seen = [];
  for (let i = 0; i < steps; i++) {
    await page.keyboard.press("Tab");
    const info = await page.evaluate(() => {
      const el = document.activeElement;
      if (!el || el === document.body) return null;
      const cs = getComputedStyle(el);
      // Curseurs : l'anneau est dessiné sur le bouton du curseur (::-webkit-slider-thumb).
      const range = el.matches?.(".slider input[type=range]");
      const ring = range || (cs.outlineStyle !== "none" && parseFloat(cs.outlineWidth) > 0) || (cs.boxShadow && cs.boxShadow !== "none");
      const label = el.getAttribute("aria-label") || el.getAttribute("title") || el.textContent?.trim() || el.getAttribute("placeholder") || (el.labels?.[0]?.textContent ?? "");
      // Repère unique de l'élément (le parcours s'arrête au premier retour).
      if (!el.dataset.kbd) el.dataset.kbd = String(Math.random()).slice(2);
      return { tag: el.tagName.toLowerCase(), cls: (el.className?.toString?.() ?? "").split(" ")[0], label: label.slice(0, 40), ring, key: el.dataset.kbd };
    });
    if (!info) continue;
    if (seen.includes(info.key)) break;
    seen.push(info.key);
    const bad = [];
    if (!info.label) bad.push("sans nom");
    if (!info.ring) bad.push("focus invisible");
    if (bad.length) {
      problems++;
      console.log(`  ✗ ${name} : <${info.tag} class="${info.cls}"> « ${info.label} » — ${bad.join(", ")}`);
    }
  }
  console.log(`${name} : ${seen.length} éléments atteints au clavier`);
}

await page.evaluate(() => {
  const s = window.__aura.getState();
  for (const t of [...s.tabs]) s.closeTab(t.key);
  s.goHome();
});
await walk("Accueil");
await page.evaluate(async (p) => {
  await window.__aura.getState().openPath(p);
  window.__aura.getState().setModule("lire");
}, report);
for (const [mod, name] of [
  ["lire", "Lecture"],
  ["annoter", "Annoter"],
  ["editer", "Éditer"],
  ["signer", "Signer"],
  ["securite", "Sécurité"],
  ["forms", "Formulaires"],
  ["convert", "Convertir"],
  ["compress", "Compresser"],
  ["fusion", "Fusionner"],
]) {
  await page.evaluate((m) => window.__aura.getState().setModule(m), mod);
  await walk(name);
}
console.log(problems ? `${problems} problème(s).` : "Aucun problème.");
await browser.close();
