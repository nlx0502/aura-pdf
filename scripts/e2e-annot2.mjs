// Suite du scénario Annoter : zone de texte, note, annuler, fermeture non enregistrée.
import { chromium } from "playwright-core";

const [, , outDir = "docs/captures"] = process.argv;
const browser = await chromium.connectOverCDP(`http://127.0.0.1:${process.env.SHOT_PORT ?? 9222}`);
const ctx = browser.contexts()[0];
const page = ctx.pages().find((p) => !p.url().startsWith("devtools")) ?? ctx.pages()[0];
const wait = (ms) => page.waitForTimeout(ms);
const pt = async (x, y) => {
  // Page 1 rendue (le visualiseur peut se reconstruire après un enregistrement).
  await page.waitForSelector('.page[data-page="0"] .alayer__svg', { timeout: 15000 }).catch(async () => {
    console.log("page 1 absente :", await page.evaluate(() => [...document.querySelectorAll(".page")].map((e) => e.dataset.page).join(",")));
  });
  return ptNow(x, y);
};
const ptNow = async (x, y) =>
  page.evaluate(
    ([x, y]) => {
      const svg = document.querySelector('.page[data-page="0"] .alayer__svg');
      const p = new DOMPoint(x, y).matrixTransform(svg.getScreenCTM());
      return [p.x, p.y];
    },
    [x, y],
  );
const count = () => page.evaluate(() => document.querySelectorAll(".aitem").length);

// État neutre : aucun champ de saisie actif (le commentaire saisi à l'étape précédente).
await page.evaluate(() => document.activeElement?.blur?.());
// Retour en page 1 (le document vient peut-être d'être rechargé après Ctrl+S).
for (let i = 0; i < 20; i++) {
  await page.evaluate(() => {
    const s = window.__aura.getState();
    s.jumpTo(s.active, 0);
  });
  await wait(400);
  if (await page.evaluate(() => !!document.querySelector('.page[data-page="0"] .alayer__svg'))) break;
}
await wait(400);
await page.keyboard.press("Escape");
const before = await count();
// Zone de texte : clic, saisie, validation par clic ailleurs.
await page.keyboard.press("t");
await page.mouse.click(...(await pt(380, 560)));
await wait(300);
await page.keyboard.type("Valider avec le service achats");
await page.screenshot({ path: `${outDir}/j3-zone-texte.png` });
await page.mouse.click(...(await pt(560, 800)));
await wait(400);
// Note : clic, commentaire saisi dans le panneau.
await page.keyboard.press("n");
await page.mouse.click(...(await pt(540, 120)));
await wait(300);
await page.keyboard.type("À reformuler");
await page.keyboard.press("Enter");
await wait(800);
await page.screenshot({ path: `${outDir}/j3-note.png` });
const afterAdd = await count();
// Annuler deux fois (focus hors du champ de commentaire).
await page.mouse.click(...(await pt(560, 820)));
await page.keyboard.press("Control+z");
await page.keyboard.press("Control+z");
await wait(300);
const afterUndo = await count();
await page.keyboard.press("Control+y");
await wait(300);
const afterRedo = await count();
// Fermeture de l'onglet : confirmation demandée.
await page.keyboard.press("Control+w");
await wait(500);
const dialog = await page.evaluate(() => document.querySelector(".dialog")?.textContent ?? null);
await page.screenshot({ path: `${outDir}/j3-confirmation.png` });
await page.keyboard.press("Escape");
await wait(300);
const stillOpen = await page.evaluate(() => window.__aura.getState().tabs.length);
console.log(JSON.stringify({ before, afterAdd, afterUndo, afterRedo, dialog, stillOpen }, null, 1));
await browser.close();
