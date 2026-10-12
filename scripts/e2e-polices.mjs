// Polices du système (zone de texte) et page de garde non numérotée.
//   node scripts/e2e-polices.mjs [dossier-captures]
import { chromium } from "playwright-core";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const outDir = process.argv[2] ?? "docs/captures";
const work = fs.mkdtempSync(path.join(os.tmpdir(), "aura-e2e-polices-"));
const pdf = path.join(work, "Rapport.pdf");
fs.copyFileSync(path.resolve("tests/fixtures/Rapport Q1 2025.pdf"), pdf);

const browser = await chromium.connectOverCDP(`http://127.0.0.1:${process.env.SHOT_PORT ?? 9222}`);
const page = browser.contexts()[0].pages().find((p) => !p.url().startsWith("devtools"));
const wait = (ms) => page.waitForTimeout(ms);
const check = (ok, msg) => {
  if (!ok) throw new Error("ÉCHEC : " + msg);
  console.log("ok —", msg);
};

await page.evaluate(async (p) => {
  await window.__aura.getState().openPath(p);
  const s = window.__aura.getState();
  s.updateTab(s.active, { fit: "width", view: "continu" });
  s.setModule("annoter");
}, pdf);
await wait(1500);

// 1. Outil Texte, police « Georgia » choisie dans le menu des polices installées.
await page.keyboard.press("t");
await wait(200);
await page.click(".rsec .fpick__more");
await wait(500);
await page.fill(".fpop input", "georgia");
await wait(200);
await page.screenshot({ path: `${outDir}/j4-polices.png` });
await page.keyboard.press("Enter");
await wait(200);
check((await page.locator(".rsec .fpick__name").innerText()) === "Georgia", "Georgia choisie");
const svg = await page.locator('.page[data-page="0"] .alayer__svg').boundingBox();
await page.mouse.click(svg.x + svg.width * 0.12, svg.y + svg.height * 0.42);
await wait(250);
await page.keyboard.type("Note en Georgia");
await page.mouse.click(svg.x + svg.width * 0.9, svg.y + svg.height * 0.55);
await wait(300);
await page.keyboard.press("Control+s");
await wait(1800);
const list = await page.evaluate(() => window.__TAURI_INTERNALS__.invoke("annot_load", { doc: window.__aura.getState().tabs.find((t) => t.key === window.__aura.getState().active).info.id }));
check(list.some((a) => a.kind === "text" && a.font === "Georgia" && a.text === "Note en Georgia"), "zone de texte enregistrée et relue en Georgia");
await page.screenshot({ path: `${outDir}/j4-polices-page.png` });

// 2. Numérotation sans numéro sur la page de garde.
await page.evaluate(() => window.__aura.getState().setModule("editer"));
await wait(1200);
const sw = page.locator('.eside button[role="switch"][aria-label="Numérotation"]');
if ((await sw.getAttribute("aria-checked")) !== "true") await sw.click();
await page.click('.eside button[role="switch"][aria-label="Page de garde sans numéro"]');
await wait(600);
await page.screenshot({ path: `${outDir}/j4-page-de-garde.png` });
const labels = await page.locator(".ecard__over .eov__num").allInnerTexts();
check(labels[0] === "2 / 14" && labels.length === 13, `page de garde sans numéro (${labels.slice(0, 2)})`);
await browser.close();
