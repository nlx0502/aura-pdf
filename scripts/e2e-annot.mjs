// Scénario de bout en bout du module Annoter, avec de vrais gestes souris.
//   node scripts/e2e-annot.mjs <pdf> <dossier-captures>
import { chromium } from "playwright-core";

const [, , pdf, outDir = "docs/captures"] = process.argv;
const browser = await chromium.connectOverCDP(`http://127.0.0.1:${process.env.SHOT_PORT ?? 9222}`);
const ctx = browser.contexts()[0];
const page = ctx.pages().find((p) => !p.url().startsWith("devtools")) ?? ctx.pages()[0];
const wait = (ms) => page.waitForTimeout(ms);

await page.evaluate(async (p) => {
  const s = window.__aura.getState();
  await s.openPath(p);
  const st = window.__aura.getState();
  st.updateTab(st.active, { fit: "width", view: "continu" });
  st.jumpTo(st.active, 0);
  st.setModule("annoter");
}, pdf);
await wait(2500);

// Position écran d'un point de la page 0 exprimé en points PDF.
const pt = async (x, y) =>
  page.evaluate(
    ([x, y]) => {
      const svg = document.querySelector('.page[data-page="0"] .alayer__svg');
      const p = new DOMPoint(x, y).matrixTransform(svg.getScreenCTM());
      return [p.x, p.y];
    },
    [x, y],
  );
const drag = async (a, b, steps = 12) => {
  const [x0, y0] = await pt(...a);
  const [x1, y1] = await pt(...b);
  await page.mouse.move(x0, y0);
  await page.mouse.down();
  await page.mouse.move(x1, y1, { steps });
  await page.mouse.up();
  await wait(250);
};

// Surligner la première ligne du premier paragraphe (outil H).
await page.keyboard.press("h");
await drag([60, 205], [420, 205]);
// Souligner (U) et barrer (S) d'autres lignes.
await page.keyboard.press("u");
await drag([60, 290], [300, 290]);
// Flèche (A) et rectangle (R) verts.
await page.click('.atoolbar .dot[aria-label*="vert"]');
await page.keyboard.press("a");
await drag([470, 360], [400, 420]);
await page.keyboard.press("r");
await drag([60, 420], [470, 520]);
// Dessin libre (P) en indigo.
await page.click('.atoolbar .dot[aria-label*="indigo"]');
await page.keyboard.press("p");
{
  const pts = Array.from({ length: 30 }, (_, i) => [300 + i * 6, 600 + Math.sin(i / 3) * 18]);
  const [sx, sy] = await pt(...pts[0]);
  await page.mouse.move(sx, sy);
  await page.mouse.down();
  for (const p of pts.slice(1)) {
    const [x, y] = await pt(...p);
    await page.mouse.move(x, y);
  }
  await page.mouse.up();
}
await wait(300);
// Tampon « Approuvé ».
await page.click(".astamp >> nth=0");
await wait(200);
await page.screenshot({ path: `${outDir}/j3-animation.png` });
await wait(1200);
// Sélectionner le surlignage et le commenter.
await page.keyboard.press("v");
// Centre de la bande surlignée, lu dans le DOM.
const [hx, hy] = await page.evaluate(() => {
  const r = document.querySelector('.page[data-page="0"] .ann__sweep').getBoundingClientRect();
  return [r.x + r.width / 2, r.y + r.height / 2];
});
await page.mouse.click(hx, hy);
await wait(400);
await page.screenshot({ path: `${outDir}/j3-selection.png` });
await page.click(".hud .btn >> nth=0");
await page.fill(".acomments__input input", "Ces chiffres ont-ils été validés par la finance ?");
await page.keyboard.press("Enter");
await wait(500);
const state = await page.evaluate(() => {
  const s = window.__aura.getState();
  return { tab: s.active };
});
const summary = await page.evaluate((k) => {
  // Accès au magasin des annotations via React : on lit le DOM de la liste.
  return [...document.querySelectorAll(".aitem .aitem__kind")].map((e) => e.textContent.trim());
}, state.tab);
await page.screenshot({ path: `${outDir}/j3-annoter.png` });
// Enregistrement (Ctrl+S) puis relecture du fichier par le cœur.
await page.keyboard.press("Escape");
await page.keyboard.press("Control+s");
await wait(2500);
const reread = await page.evaluate(async () => {
  const s = window.__aura.getState();
  const tab = s.tabs.find((t) => t.key === s.active);
  const list = await window.__TAURI_INTERNALS__.invoke("annot_load", { doc: tab.info.id });
  return list.map((a) => `${a.kind}:${a.color}${a.contents ? ` « ${a.contents} »` : ""}`);
});
await page.screenshot({ path: `${outDir}/j3-enregistre.png` });
console.log(JSON.stringify({ annotations: summary, relu: reread }, null, 1));
await browser.close();
