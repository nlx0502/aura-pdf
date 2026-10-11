// Module Éditer, avec de vrais gestes souris : sélection, rotation, réglages,
// glisser-déposer, suppression, caviardage, édition du contenu, enregistrement.
//   node scripts/e2e-edit.mjs [dossier-captures]
import { chromium } from "playwright-core";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const outDir = process.argv[2] ?? "docs/captures";
const fixtures = path.resolve("tests/fixtures");
const work = fs.mkdtempSync(path.join(os.tmpdir(), "aura-e2e-edit-"));
const rapport = path.join(work, "Dossier d’audit Q4.pdf");
const sensible = path.join(work, "Données sensibles.pdf");
fs.copyFileSync(path.join(fixtures, "Rapport Q1 2025.pdf"), rapport);
fs.copyFileSync(path.join(fixtures, "Données sensibles.pdf"), sensible);

const browser = await chromium.connectOverCDP(`http://127.0.0.1:${process.env.SHOT_PORT ?? 9222}`);
const page = browser.contexts()[0].pages().find((p) => !p.url().startsWith("devtools"));
const wait = (ms) => page.waitForTimeout(ms);
const invoke = (cmd, args) => page.evaluate(([c, a]) => window.__TAURI_INTERNALS__.invoke(c, a), [cmd, args]);
const state = () =>
  page.evaluate(() => {
    const app = window.__aura.getState();
    const d = window.__auraEdit.getState().docs[app.active];
    return { doc: app.tabs.find((t) => t.key === app.active).info, pages: d?.pages.map((p) => p.src), sel: d?.sel.length, dirty: d?.dirty };
  });
const check = (ok, msg) => {
  if (!ok) throw new Error("ÉCHEC : " + msg);
  console.log("ok —", msg);
};
const card = (i) => page.locator(".ecard__pg").nth(i);

// ---- 1. Ouverture dans le module Éditer
await page.evaluate(async (p) => {
  const s = window.__aura.getState();
  await s.openPath(p);
  window.__aura.getState().setModule("editer");
}, rapport);
await wait(1500);
check((await state()).pages.length === 14, "14 pages affichées");

// ---- 2. Sélection 2, 3, 5 ; rotation de la page 4 ; numérotation ; filigrane
await card(1).click();
await card(2).click();
await card(4).click();
check((await state()).sel === 3, "trois pages sélectionnées");
await page.evaluate(() => {
  const app = window.__aura.getState();
  const e = window.__auraEdit.getState();
  const d = e.docs[app.active];
  e.rotate(app.active, [d.pages[3].key]);
});
await page.click('.eside button[role="switch"][aria-label="Numérotation"]');
await page.click('.eside .chip:has-text("1 / 14")');
await page.fill('.eside input[placeholder="Aucun filigrane"]', "CONFIDENTIEL");
await wait(900);
await page.screenshot({ path: `${outDir}/j4-organiser.png` });

// ---- 3. Glisser la page 1 après la page 3
await page.click('.eselbar button[aria-label="Effacer la sélection"]');
const a = await card(0).boundingBox();
const b = await card(2).boundingBox();
await page.mouse.move(a.x + a.width / 2, a.y + a.height / 2);
await page.mouse.down();
await page.mouse.move(a.x + a.width / 2 + 20, a.y + a.height / 2, { steps: 4 });
await page.mouse.move(b.x + b.width * 0.8, b.y + b.height / 2, { steps: 12 });
await wait(150);
await page.screenshot({ path: `${outDir}/j4-glisser.png` });
await page.mouse.up();
await wait(500);
const order = (await state()).pages.slice(0, 4).map((s) => s.index);
check(JSON.stringify(order) === "[1,2,0,3]", `page 1 déplacée après la page 3 (${order})`);

// ---- 4. Suppression animée de deux pages (Maj pour une plage)
await card(12).click();
await card(13).click({ modifiers: ["Shift"] });
await page.click('.eselbar button:has-text("Supprimer")');
await wait(150);
await page.screenshot({ path: `${outDir}/j4-suppression.png` });
await wait(500);
check((await state()).pages.length === 12, "deux pages supprimées");

// ---- 5. Annuler / rétablir
await page.keyboard.press("Control+z");
await wait(300);
check((await state()).pages.length === 14, "annuler rétablit les pages");
await page.keyboard.press("Control+y");
await wait(300);
check((await state()).pages.length === 12, "rétablir");

// ---- 6. Contenu : titre modifié, lien ajouté
await page.evaluate(() => {
  const app = window.__aura.getState();
  const e = window.__auraEdit.getState();
  e.patch(app.active, { mode: "content", contentPage: 0 });
});
await wait(1200);
const title = page.locator('.cobj[aria-label*="Stratégie"]');
await title.dblclick();
await wait(300);
await page.keyboard.press("Control+a");
await page.keyboard.type("Dossier d’audit Q4 — synthèse");
await page.locator(".cview__hint").click();
await wait(900);
await page.click('.cview__tools .chip:has-text("Lien")');
const pg = await page.locator(".cview__layer").boundingBox();
await page.mouse.move(pg.x + pg.width * 0.1, pg.y + pg.height * 0.6);
await page.mouse.down();
await page.mouse.move(pg.x + pg.width * 0.5, pg.y + pg.height * 0.63, { steps: 6 });
await page.mouse.up();
await wait(300);
await page.fill('.ecpanel input[placeholder^="https://"]', "audit.exemple.fr/q4");
await page.keyboard.press("Enter");
await page.click('.cview__tools .chip:has-text("Texte")');
await wait(600);
await page.screenshot({ path: `${outDir}/j4-contenu.png` });

// ---- 7. Enregistrement
await page.keyboard.press("Control+s");
await wait(2500);
let st = await state();
check(st.doc.pageCount === 12 && !st.dirty, `enregistré : ${st.doc.pageCount} pages`);
// L'ancienne page 1 est désormais la 3e.
const words = (await invoke("page_text", { doc: st.doc.id, page: 2 })).map((w) => w.t).join(" ");
check(words.includes("synthèse") && !words.includes("innovation") && words.includes("3 / 12") && words.includes("CONFIDENTIEL"), "titre, numéro et filigrane écrits dans le fichier");
const links = await invoke("edit_links", { doc: st.doc.id });
check(links[2].some((l) => l.uri === "https://audit.exemple.fr/q4"), "lien enregistré");
await page.evaluate(() => window.__auraEdit.getState().patch(window.__aura.getState().active, { mode: "pages" }));
await wait(1200);
await page.screenshot({ path: `${outDir}/j4-enregistre.png` });

// ---- 8. Caviardage sur un autre document
await page.evaluate(async (p) => {
  await window.__aura.getState().openPath(p);
  window.__aura.getState().setModule("editer");
}, sensible);
await wait(1500);
await page.click('.eside .chip:has-text("Téléphone")');
await page.click('.eside button:has-text("Rechercher et caviarder")');
await wait(500);
await page.screenshot({ path: `${outDir}/j4-caviardage-scan.png` });
await wait(1800);
await page.screenshot({ path: `${outDir}/j4-caviardage.png` });
await page.keyboard.press("Control+s");
await wait(2500);
st = await state();
const hits = await invoke("redact_find", { doc: st.doc.id, patterns: ["iban", "email", "tel"], terms: [] });
check(hits.length === 0, "plus aucune donnée sensible dans le fichier enregistré");
const raw = fs.readFileSync(sensible, "latin1");
check(!raw.includes("jeanne"), "aucune trace brute dans le fichier");

console.log("Scénario Éditer réussi.", work);
await browser.close();
