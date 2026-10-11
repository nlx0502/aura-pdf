// Zones de texte : nue par défaut, puis stylée ; rendu avant et après enregistrement.
import { chromium } from "playwright-core";

const [, , pdf, outDir = "docs/captures"] = process.argv;
const browser = await chromium.connectOverCDP(`http://127.0.0.1:${process.env.SHOT_PORT ?? 9222}`);
const page = browser.contexts()[0].pages().find((p) => !p.url().startsWith("devtools"));
const wait = (ms) => page.waitForTimeout(ms);

await page.evaluate(async (p) => {
  const s = window.__aura.getState();
  await s.openPath(p);
  const st = window.__aura.getState();
  st.updateTab(st.active, { fit: "width", view: "continu" });
  st.setModule("annoter");
}, pdf);
await wait(1800);
await page.evaluate(() => {
  const s = window.__aura.getState();
  s.jumpTo(s.active, 0);
});
await wait(900);
const pt = async (x, y) =>
  page.evaluate(
    ([x, y]) => {
      const svg = document.querySelector('.page[data-page="0"] .alayer__svg');
      const p = new DOMPoint(x, y).matrixTransform(svg.getScreenCTM());
      return [p.x, p.y];
    },
    [x, y],
  );

// 1. Zone nue (style par défaut).
await page.keyboard.press("t");
await page.mouse.click(...(await pt(70, 330)));
await wait(250);
await page.keyboard.type("Texte simple, sans cadre ni fond");
await page.mouse.click(...(await pt(560, 440)));
await wait(300);

// 2. Zone stylée : réglages choisis avant de cliquer.
await page.keyboard.press("t");
await wait(200);
await page.click('.rsec [title="Avec empattement (Times)"]');
await page.click('.rsec button[aria-label="Gras"]');
await page.click('.rsec button[aria-label="Italique"]');
for (let i = 0; i < 4; i++) await page.click('.rsec button[aria-label="Plus grand"]');
await page.click('.rsec button[aria-label="Centrer"]');
// Couleur libre : bulle « + » puis palette étendue (orange).
await page.click('.rsec .cdots[aria-label="Couleur du texte"] .dot--more');
await wait(200);
await page.click('.cpop__swatch[title="#F28C38"]');
await page.click('.rsec .cdots[aria-label="Fond"] .dot[title="jaune pâle"]');
await page.click('.rsec .cdots[aria-label="Bordure"] .dot[title="noir"]');
await page.mouse.click(...(await pt(330, 395)));
await wait(250);
await page.keyboard.type("Validé par la direction");
await wait(200);
await page.screenshot({ path: `${outDir}/j3-texte-edition.png` });
await page.mouse.click(...(await pt(560, 440)));
await wait(400);
await page.keyboard.press("Escape");
await page.screenshot({ path: `${outDir}/j3-texte-avant.png` });
await page.keyboard.press("Control+s");
await wait(2500);
await page.screenshot({ path: `${outDir}/j3-texte-apres.png` });
const relu = await page.evaluate(async () => {
  const s = window.__aura.getState();
  const tab = s.tabs.find((t) => t.key === s.active);
  const list = await window.__TAURI_INTERNALS__.invoke("annot_load", { doc: tab.info.id });
  return list.filter((a) => a.kind === "text").map((a) => ({ texte: a.text, police: a.font, gras: a.bold, italique: a.italic, taille: a.fontSize, couleur: a.color, fond: a.fill, bordure: a.borderColor, alignement: a.align }));
});
console.log(JSON.stringify(relu, null, 1));
await browser.close();
