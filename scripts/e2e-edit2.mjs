// Mode Contenu (images, caviardage manuel) et calques du document.
//   node scripts/e2e-edit2.mjs [dossier-captures]
import { chromium } from "playwright-core";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const outDir = process.argv[2] ?? "docs/captures";
const work = fs.mkdtempSync(path.join(os.tmpdir(), "aura-e2e-edit2-"));
const doc = path.join(work, "Annexe.pdf");
fs.copyFileSync(path.resolve("tests/fixtures/Données sensibles.pdf"), doc);
const plan = path.resolve("tests/fixtures/Plan avec calques.pdf");

const browser = await chromium.connectOverCDP(`http://127.0.0.1:${process.env.SHOT_PORT ?? 9222}`);
const page = browser.contexts()[0].pages().find((p) => !p.url().startsWith("devtools"));
const wait = (ms) => page.waitForTimeout(ms);
const check = (ok, msg) => {
  if (!ok) throw new Error("ÉCHEC : " + msg);
  console.log("ok —", msg);
};
const items = () => page.evaluate(() => window.__TAURI_INTERNALS__.invoke("content_list", { doc: window.__aura.getState().tabs.find((t) => t.key === window.__aura.getState().active).info.id, page: 1 }));

await page.evaluate(async (p) => {
  await window.__aura.getState().openPath(p);
  const app = window.__aura.getState();
  app.setModule("editer");
}, doc);
await wait(1200);
await page.evaluate(() => window.__auraEdit.getState().patch(window.__aura.getState().active, { mode: "content", contentPage: 1 }));
await wait(1200);

// Image : sélection, rotation, agrandissement par la poignée.
await page.click('.cview__tools .chip:has-text("Image")');
const before = (await items()).find((i) => i.kind === "image").rect;
await page.locator(".cobj").first().click();
await wait(200);
await page.click('.ecpanel button:has-text("Pivoter")');
await wait(800);
let img = (await items()).find((i) => i.kind === "image").rect;
check(Math.abs(img.w - before.h) < 1 && Math.abs(img.h - before.w) < 1, "image pivotée d’un quart de tour");
const h = await page.locator(".cobj.is-sel .chandle--3").boundingBox();
await page.mouse.move(h.x + 4, h.y + 4);
await page.mouse.down();
await page.mouse.move(h.x + 60, h.y + 60, { steps: 8 });
await page.mouse.up();
await wait(800);
const after = (await items()).find((i) => i.kind === "image").rect;
check(after.w > img.w + 20, `image agrandie (${img.w.toFixed(0)} → ${after.w.toFixed(0)} pt)`);

// Caviardage manuel d'une ligne de texte.
await page.click('.cview__tools .chip:has-text("Caviarder")');
await page.locator('.cobj[aria-label^="Ligne directe"]').click();
await wait(500);
await page.screenshot({ path: `${outDir}/j4-contenu-image.png` });
const marks = await page.evaluate(() => window.__auraEdit.getState().docs[window.__aura.getState().active].pages[1].redact.length);
check(marks === 1, "zone de caviardage ajoutée à la main");
await page.keyboard.press("Control+z");
await wait(300);

// Calques : panneau de lecture.
await page.evaluate(async (p) => {
  await window.__aura.getState().openPath(p);
  const app = window.__aura.getState();
  app.setModule("lire");
  app.set({ leftPanel: "layers", leftOpen: true });
}, plan);
await wait(1500);
await page.click('button[role="switch"][aria-label*="Mobilier"]');
await wait(1200);
await page.screenshot({ path: `${outDir}/j4-calques.png` });
check(await page.locator('button[role="switch"][aria-label*="Mobilier"][aria-checked="true"]').count() === 1, "calque Mobilier affiché");
console.log("Scénario Contenu / calques réussi.");
await browser.close();
