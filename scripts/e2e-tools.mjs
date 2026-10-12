// Outils Fusion / Division / Compresser, avec de vrais gestes souris.
//   node scripts/e2e-tools.mjs [dossier-captures]
import { chromium } from "playwright-core";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const outDir = process.argv[2] ?? "docs/captures";
const fx = path.resolve("tests/fixtures");
const work = fs.mkdtempSync(path.join(os.tmpdir(), "aura-e2e-tools-"));
const copy = (name, as = name) => {
  const p = path.join(work, as);
  fs.copyFileSync(path.join(fx, name), p);
  return p;
};
const rapport = copy("Rapport Q1 2025.pdf", "Dossier principal.pdf");
const paysage = copy("Paysage.pdf", "Plans.pdf");
const annexe = copy("Données sensibles.pdf", "Annexe financière.pdf");

const browser = await chromium.connectOverCDP(`http://127.0.0.1:${process.env.SHOT_PORT ?? 9222}`);
const page = browser.contexts()[0].pages().find((p) => !p.url().startsWith("devtools"));
const wait = (ms) => page.waitForTimeout(ms);
const check = (ok, msg) => {
  if (!ok) throw new Error("ÉCHEC : " + msg);
  console.log("ok —", msg);
};
const invoke = (cmd, args) => page.evaluate(([c, a]) => window.__TAURI_INTERNALS__.invoke(c, a), [cmd, args]);
const tool = () => page.evaluate(() => window.__auraTools.merge.getState());
const until = async (fn, ms = 20000) => {
  const t0 = Date.now();
  while (Date.now() - t0 < ms) {
    if (await fn()) return true;
    await wait(150);
  }
  return false;
};

// ---- 1. Fusion : le document ouvert sert de point de départ, puis deux fichiers ajoutés.
await page.evaluate(async (p) => {
  // Outils remis à zéro (leur état est conservé entre deux visites).
  window.__auraTools.merge.setState({ files: [], source: null, tab: "merge", outName: "", run: { kind: "idle" }, opts: { bookmarks: true, toc: false, a4: false } });
  window.__auraTools.compress.setState({ source: null, analysis: null, result: null, run: { kind: "idle" } });
  window.__aura.getState().setModule("lire");
  await window.__aura.getState().openPath(p);
  window.__aura.getState().setModule("fusion");
}, rapport);
await wait(1200);
await page.evaluate((ps) => window.__auraTools.merge.getState().add(ps), [annexe, paysage]);
await wait(900);
check((await tool()).files.length === 3, "trois fichiers dans la liste");
await page.screenshot({ path: `${outDir}/j5-fusion.png` });

// Glisser « Plans.pdf » (3e) en 2e position par sa poignée.
const grip = page.locator(".mfile__grip").nth(2);
const g = await grip.boundingBox();
await page.mouse.move(g.x + g.width / 2, g.y + g.height / 2);
await page.mouse.down();
await page.mouse.move(g.x + g.width / 2, g.y - 40, { steps: 6 });
await page.mouse.move(g.x + g.width / 2, g.y - 82, { steps: 6 });
await page.screenshot({ path: `${outDir}/j5-fusion-glisser.png` });
await page.mouse.up();
await wait(600);
check((await tool()).files.map((f) => f.name).join(",") === "Dossier principal.pdf,Plans.pdf,Annexe financière.pdf", "ordre modifié par glisser-déposer");

await page.click('.tool__side button[role="switch"][aria-label="Ajouter une table des matières"]');
await page.fill(".tool__side input", "Dossier complet");
await page.click('.tool__side button:has-text("Fusionner 3 fichiers")');
check(await until(async () => (await tool()).run.kind === "done"), "fusion terminée");
const merged = (await tool()).run.outputs[0];
check(merged.pages === 14 + 4 + 3 + 1 && merged.path.endsWith("Dossier complet.pdf"), `fichier fusionné : ${merged.pages} pages`);
await wait(500);
await page.screenshot({ path: `${outDir}/j5-fusion-terminee.png` });
const info = await invoke("tool_inspect", { path: merged.path });
check(info.marks.filter((m) => m.depth === 0).map((m) => m.title).join("|") === "Sommaire|Dossier principal|Plans|Annexe financière", "un signet par fichier, sommaire en tête");

// ---- 2. Division par plages puis par signets.
await page.click('.tool__head button:has-text("Diviser")');
await wait(700);
await page.fill('.tool__main input[aria-label="Plages de pages"]', "1-3, 4-9, 10-14");
await wait(500);
await page.screenshot({ path: `${outDir}/j5-division.png` });
await page.click('.tool__method:has-text("Par signets")');
await wait(600);
await page.screenshot({ path: `${outDir}/j5-division-signets.png` });
await page.click('.tool__method:has-text("Par pages")');
await page.click(".tool__side .btn--primary");
check(await until(async () => (await tool()).run.kind === "done"), "division terminée");
const parts = (await tool()).run.outputs;
check(parts.map((p) => p.pages).join(",") === "3,6,5", `trois fichiers : ${parts.map((p) => p.pages)}`);
await wait(400);
await page.screenshot({ path: `${outDir}/j5-division-terminee.png` });

// ---- 3. Compresser le fichier fusionné (images, sommaire) au niveau « Forte ».
await page.evaluate(() => window.__aura.getState().setModule("compress"));
await page.evaluate((p) => window.__auraTools.compress.getState().setSource(p), merged.path);
check(await until(async () => !!(await page.evaluate(() => window.__auraTools.compress.getState().analysis))), "analyse prête");
await page.click('.clvl:has-text("Forte")');
await wait(900);
await page.screenshot({ path: `${outDir}/j5-compresser.png` });
await page.click('.tool__side .btn--primary:has-text("Compresser")');
await wait(250);
await page.screenshot({ path: `${outDir}/j5-compresser-etapes.png` });
check(await until(async () => !!(await page.evaluate(() => window.__auraTools.compress.getState().result))), "compression terminée et analysée");
const c = await page.evaluate(() => {
  const s = window.__auraTools.compress.getState();
  return { before: s.analysis.total, after: s.result.total, out: s.run.outputs[0].path };
});
check(c.after < c.before, `taille réduite : ${c.before} → ${c.after} octets`);
const back = await invoke("tool_inspect", { path: c.out });
check(back.pages === 22, "toutes les pages conservées");
await wait(700);
await page.screenshot({ path: `${outDir}/j5-compresser-termine.png` });
console.log("Scénario Outils réussi.", work);
await browser.close();
