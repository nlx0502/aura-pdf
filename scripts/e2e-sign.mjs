// Module Signer : dessin réel dans le pad, champ de signature, signature
// numérique (certificat personnel créé pour l'occasion), puis manuscrite.
//   node scripts/e2e-sign.mjs [dossier-captures]
import { chromium } from "playwright-core";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const outDir = process.argv[2] ?? "docs/captures";
const work = fs.mkdtempSync(path.join(os.tmpdir(), "aura-e2e-sign-"));
const pdf = path.join(work, "Accord de consortium.pdf");
fs.copyFileSync(path.resolve("tests/fixtures/Accord de consortium.pdf"), pdf);
const cert = path.join(work, "Julien Martin.p12");

const browser = await chromium.connectOverCDP(`http://127.0.0.1:${process.env.SHOT_PORT ?? 9222}`);
const page = browser.contexts()[0].pages().find((p) => !p.url().startsWith("devtools"));
const wait = (ms) => page.waitForTimeout(ms);
const check = (ok, msg) => {
  if (!ok) throw new Error("ÉCHEC : " + msg);
  console.log("ok —", msg);
};
const sign = () => page.evaluate(() => window.__auraSign.getState());
const report = async () => (await page.evaluate(() => window.__auraSign.getState().reports[window.__aura.getState().active])) ?? { signatures: [], empty: [] };
const until = async (fn, ms = 20000) => {
  const t0 = Date.now();
  while (Date.now() - t0 < ms) {
    if (await fn()) return true;
    await wait(150);
  }
  return false;
};

// ---- Ouverture : dernière page (champs « Société A » et « Société B »).
await page.evaluate(async (p) => {
  window.__auraSign.setState({ strokes: [], placement: null, mode: "draw", certOn: false, certPath: null, cert: null, typed: "Julien Martin", step: -1 });
  await window.__aura.getState().openPath(p);
  const s = window.__aura.getState();
  s.updateTab(s.active, { fit: "width", view: "continu" });
  s.setModule("signer");
}, pdf);
await wait(1500);
await page.evaluate(() => window.__aura.getState().jumpTo(window.__aura.getState().active, 1));
await wait(900);
check((await report()).empty.length === 2, "deux champs de signature à remplir");
await page.screenshot({ path: `${outDir}/j6-signer.png` });

// ---- Signature dessinée à la souris dans le pad.
const pad = await page.locator(".spad svg").boundingBox();
const pts = [[30, 104], [45, 60], [58, 34], [66, 50], [60, 100], [78, 70], [92, 66], [100, 96], [118, 74], [134, 92], [160, 62], [180, 98], [215, 84], [260, 82]];
await page.mouse.move(pad.x + pts[0][0], pad.y + pts[0][1]);
await page.mouse.down();
for (const [x, y] of pts.slice(1)) await page.mouse.move(pad.x + x, pad.y + y, { steps: 5 });
await page.mouse.up();
await wait(200);
check((await sign()).strokes[0].length > 20, "signature dessinée dans le pad");

// ---- Certificat personnel créé puis choisi.
await page.evaluate(async (c) => {
  await window.__TAURI_INTERNALS__.invoke("cert_create", { name: "Julien Martin", email: "julien@exemple.fr", organization: "Société B", password: "secret", out: c });
  window.__auraSign.setState({ certOn: true, certPath: c, certPassword: "secret", certify: "0" });
  await window.__auraSign.getState().checkCert();
}, cert);
await wait(300);
check((await sign()).cert?.name === "Julien Martin", "certificat lu");

// ---- Champ « Société B », puis signature.
await page.locator(".sslot", { hasText: "Cliquez pour signer" }).nth(1).click();
await wait(400);
check((await sign()).placement?.field === "Société B", "champ choisi");
await page.screenshot({ path: `${outDir}/j6-placement.png` });
// Certification (DocMDP, formulaires et signatures permis) et profil PAdES.
const certSw = page.locator(".srow", { hasText: "Certifier le document" }).locator("[role=switch]");
await certSw.click();
check(await until(async () => (await sign()).certify === "2", 3000), "certification activée");
await page.locator(".srow", { hasText: "Certifier le document" }).scrollIntoViewIfNeeded();
await page.screenshot({ path: `${outDir}/j9-certifier.png` });
await page.click(".sactions .btn--primary");
await wait(200);
await page.screenshot({ path: `${outDir}/j6-etapes.png` });
check(await until(async () => (await report()).signatures.length === 1), "signature numérique posée");
let r = await report();
check(r.signatures[0].signer === "Julien Martin" && r.signatures[0].status === "untrusted" && r.signatures[0].field === "Société B", `vérifiée : ${r.signatures[0].status}`);
check(r.signatures[0].certified === 2 && r.signatures[0].pades, "document certifié, profil PAdES");
check(r.empty.length === 1, "il reste un champ");
await wait(400);
await page.screenshot({ path: `${outDir}/j6-signe-encre.png` });
await wait(1600);

// ---- Signature tapée, sans certificat, dans le champ « Société A ».
await page.click('.signw__side .seg__btn:has-text("Taper")');
await page.click('.sfont[aria-label="Style Mrs Saint Delafield"]');
await page.evaluate(() => window.__auraSign.setState({ certOn: false }));
await page.locator(".sslot").first().click();
await wait(300);
await page.click(".sactions .btn--primary");
check(await until(async () => (await report()).signatures.length === 2), "seconde signature (manuscrite)");
r = await report();
const digital = r.signatures.find((s) => s.kind === "digital");
check(digital.status === "untrusted" && digital.modifiedAfter, "la signature numérique reste intacte et signale l’ajout ultérieur");
check(r.empty.length === 0 && r.signatures.some((s) => s.kind === "visual" && s.field === "Société A"), "champ « Société A » rempli par la signature manuscrite");
await wait(1800);
await page.click('.signw__side button:has-text("Vérifier")');
await wait(300);
await page.screenshot({ path: `${outDir}/j6-verification.png` });
await wait(1200);
await page.screenshot({ path: `${outDir}/j6-signe.png` });
console.log("Scénario Signer réussi.", work);
await browser.close();
