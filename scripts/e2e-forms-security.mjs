// Jalon 7 : Formulaires (saisie réelle, ajout de champ, enregistrement,
// aplatissement et annulation) puis Sécurité (mot de passe AES-256,
// permissions, réouverture, PDF/A).
//   node scripts/e2e-forms-security.mjs [dossier-captures]
import { chromium } from "playwright-core";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const outDir = process.argv[2] ?? "docs/captures";
const work = fs.mkdtempSync(path.join(os.tmpdir(), "aura-e2e-j7-"));
const form = path.join(work, "Demande d’ouverture.pdf");
fs.copyFileSync(path.resolve("tests/fixtures/Demande d’ouverture.pdf"), form);
const report = path.join(work, "Rapport Q1 2025.pdf");
fs.copyFileSync(path.resolve("tests/fixtures/Rapport Q1 2025.pdf"), report);
const archive = path.join(work, "Rapport archivé.pdf");
fs.copyFileSync(path.resolve("tests/fixtures/Rapport Q1 2025.pdf"), archive);

const browser = await chromium.connectOverCDP(`http://127.0.0.1:${process.env.SHOT_PORT ?? 9222}`);
const page = browser.contexts()[0].pages().find((p) => !p.url().startsWith("devtools"));
const wait = (ms) => page.waitForTimeout(ms);
const check = (ok, msg) => {
  if (!ok) throw new Error("ÉCHEC : " + msg);
  console.log("ok —", msg);
};
const until = async (fn, ms = 20000) => {
  const t0 = Date.now();
  while (Date.now() - t0 < ms) {
    if (await fn()) return true;
    await wait(150);
  }
  return false;
};
const forms = () => page.evaluate(() => window.__auraForms.getState().docs[window.__aura.getState().active]);
const tab = () => page.evaluate(() => window.__aura.getState().tabs.find((t) => t.key === window.__aura.getState().active));
const field = async (name) => (await forms()).fields.find((f) => f.name === name || f.label === name);
const closeActive = () => page.evaluate(() => window.__aura.getState().closeTab(window.__aura.getState().active));

// ================================================================ Formulaires
await page.evaluate(async (p) => {
  await window.__aura.getState().openPath(p);
  const s = window.__aura.getState();
  s.updateTab(s.active, { fit: "width", view: "continu" });
  s.setModule("forms");
}, form);
check(await until(async () => (await forms())?.loaded), "formulaire chargé");
let d = await forms();
check(d.fields.length === 7, `7 champs lus (${d.fields.map((f) => f.kind).join(", ")})`);
await wait(900);
await page.screenshot({ path: `${outDir}/j7-formulaires.png` });

// Saisie réelle sur la page.
await page.locator('input[aria-label="Adresse e-mail"]').click();
await page.keyboard.type("camille@exemple.fr", { delay: 15 });
await page.locator('select[aria-label="Type de compte"]').selectOption("Professionnel");
await page.locator('[role="radio"][aria-label$="Annuelle"]').click();
await page.locator('[role="checkbox"][aria-label^="J’accepte"]').click();
await page.locator('input[aria-label="Date de naissance"]').fill("1990-03-14");
await wait(300);
check((await field("email")).value === "camille@exemple.fr", "e-mail saisi au clavier");
check((await field("naissance")).value === "14/03/1990", "date au format jj/mm/aaaa");
check((await field("frequence")).value === "Annuelle" && (await field("cgu")).value === "Oui", "choix et case");
check((await forms()).dirty, "formulaire modifié");
await wait(800);
await page.screenshot({ path: `${outDir}/j7-remplissage.png` });

// Ajout d'un champ e-mail sur la page, mis en forme.
await page.locator(".fadd__k", { hasText: "E-mail" }).click();
const pg = await page.locator(".page").first().boundingBox();
await page.mouse.click(pg.x + pg.width * 0.62, pg.y + pg.height * 0.36);
await wait(400);
d = await forms();
check(d.fields.length === 8 && d.fields.at(-1).isNew && d.fields.at(-1).caption, "nouveau champ placé, avec son libellé visible");
await page.locator(".fprops label", { hasText: "Libellé" }).first().locator("input").fill("E-mail du conseiller");
await page.locator(".fprops label", { hasText: "Texte indicatif" }).locator("input").fill("conseiller@banque.fr");
await page.locator('.fprops .sw[aria-label="Obligatoire"]').click();
await page.locator(".fprops .seg__btn", { hasText: "Apparence" }).click();
await page.locator(".fprops .seg__btn", { hasText: "Serif" }).click();
await page.locator(".fprops .seg__btn", { hasText: "Pointillé" }).click();
await page.locator('.fprops button[aria-label="Fond Lavande"]').click();
await page.locator('.fprops button[aria-label="Contour Violet"]').click();
await page.locator('.fprops button[aria-label="Plus"]').first().click();
// Déplacement au clavier (Maj+flèche : 10 points).
await page.locator(".fprops .seg__btn", { hasText: "Champ" }).click();
await page.locator(".page").first().click({ position: { x: 5, y: 5 } });
const before = (await forms()).fields.at(-1).widgets[0].rect;
await page.evaluate(() => document.activeElement?.blur());
await page.keyboard.press("Shift+ArrowDown");
await page.keyboard.press("Shift+ArrowDown");
d = await forms();
const moved = d.fields.find((f) => f.label === "E-mail du conseiller");
check(moved && Math.abs(moved.widgets[0].rect.y - before.y - 20) < 0.5, "champ déplacé au clavier");
check(moved.caption.text === "E-mail du conseiller *", "libellé visible avec astérisque");
// Groupe de boutons radio.
await page.locator(".fadd__k", { hasText: "Choix" }).click();
await page.mouse.click(pg.x + pg.width * 0.6, pg.y + pg.height * 0.64);
await wait(300);
d = await forms();
const radio = d.fields.at(-1);
check(radio.kind === "radio" && radio.widgets.length === 2 && radio.widgets.every((w) => w.caption), "groupe radio créé avec ses options");
await page.locator(".fprops textarea").fill("Téléphone\nCourrier\nE-mail");
await page.locator(".fprops label", { hasText: "Libellé" }).first().locator("input").click();
await wait(300);
check((await forms()).fields.at(-1).widgets.length === 3, "trois options");
await page.locator('[role="radio"][aria-label$="Courrier"]').click();
await wait(500);
await page.screenshot({ path: `${outDir}/j7-ajout.png` });

// Enregistrement (Ctrl+S) puis relecture depuis le fichier.
await page.keyboard.press("Control+s");
check(await until(async () => !(await forms()).dirty && !(await forms()).saving), "formulaire enregistré");
d = await forms();
const added = d.orig.find((f) => f.label === "E-mail du conseiller");
check(added && added.kind === "email" && added.required && added.placeholder === "conseiller@banque.fr", "champ ajouté relu du fichier (e-mail, obligatoire, texte indicatif)");
check(added.style.font === "serif" && added.style.borderStyle === "dashed" && added.style.background === "#eef2ff" && added.caption?.text === "E-mail du conseiller *", "apparence et libellé relus du fichier");
const r2 = d.orig.find((f) => f.kind === "radio" && f.name !== "frequence");
check(r2 && r2.value === "Courrier" && r2.options.map((o) => o.label).join(",") === "Téléphone,Courrier,E-mail", "groupe radio relu");
check(d.orig.find((f) => f.name === "type").value === "Professionnel", "valeur relue du fichier");
fs.mkdirSync(path.join(os.tmpdir(), "aura-acrobat"), { recursive: true });
fs.copyFileSync(form, path.join(os.tmpdir(), "aura-acrobat", "formulaire-e2e.pdf"));
await wait(1200);
await page.screenshot({ path: `${outDir}/j7-enregistre.png` });

// Aplatissement, puis annulation.
await page.locator(".fflat button", { hasText: "Aplatir le formulaire" }).click();
await page.locator(".fflat__btns button", { hasText: "Confirmer" }).click();
await wait(250);
await page.screenshot({ path: `${outDir}/j7-aplatissement.png` });
check(await until(async () => (await forms()).fields.length === 0 && (await forms()).flatBackup), "formulaire aplati");
await wait(2500);
await page.screenshot({ path: `${outDir}/j7-aplati.png` });
await page.locator(".fflat--done button").click();
check(await until(async () => (await forms()).fields.length === 9), "aplatissement annulé");
await closeActive();
await wait(300);

// ================================================================ Sécurité
await page.evaluate(async (p) => {
  await window.__aura.getState().openPath(p);
  window.__aura.getState().setModule("securite");
}, report);
check(await until(() => page.locator(".scard").first().isVisible()), "écran Sécurité");
await wait(700);
await page.screenshot({ path: `${outDir}/j7-securite.png` });

await page.locator('.scard .sw[aria-label="Protéger par mot de passe"]').click();
await page.locator('.scard input[type="password"]').first().click();
await page.keyboard.type("Aura-2025!", { delay: 30 });
await page.locator('input[placeholder="Recommandé si des permissions sont retirées"]').fill("patron-2025");
await page.locator('.sperm .sw[aria-label="Copier le texte"]').click();
await page.locator('.sperm .sw[aria-label="Extraire des pages"]').click();
await page.locator('.smeta input').first().fill("Rapport Q1 2025 — confidentiel");
await wait(700);
check((await page.locator(".smeter__l").textContent()).includes("Excellent"), "jauge : excellent");
await page.screenshot({ path: `${outDir}/j7-securite-reglages.png` });

await page.locator(".sec__foot .btn--primary").click();
await wait(150);
await page.screenshot({ path: `${outDir}/j7-securite-application.png` });
check(await until(async () => (await page.locator(".sec__foot .tool__done").count()) > 0), "sécurité appliquée");
let info = (await tab()).info;
check(info.encrypted && info.permissions.copy, "chiffré, propriétaire : tous les droits");
await wait(600);
await page.screenshot({ path: `${outDir}/j7-securite-applique.png` });

// Réouverture avec le mot de passe d'ouverture : droits restreints.
await closeActive();
await wait(300);
const opened = await page.evaluate(async (p) => window.__aura.getState().openPath(p, "Aura-2025!"), report);
check(opened, "réouverture avec le mot de passe");
info = (await tab()).info;
check(info.encrypted && !info.permissions.copy && !info.permissions.extract && info.permissions.print, "copie et extraction interdites");
check(info.metadata.title === "Rapport Q1 2025 — confidentiel", "métadonnées chiffrées relues");
await page.evaluate(() => window.__aura.getState().setModule("securite"));
await wait(900);
check((await page.locator(".sec__unlock").count()) === 1, "déverrouillage proposé");
await page.screenshot({ path: `${outDir}/j7-securite-restreint.png` });
await closeActive();
await wait(300);

// PDF/A-2b.
await page.evaluate(async (p) => {
  await window.__aura.getState().openPath(p);
  window.__aura.getState().setModule("securite");
}, archive);
await wait(900);
await page.locator(".schips button", { hasText: "PDF/A-2b" }).click();
await page.locator(".scard button", { hasText: "Vérifier la conformité" }).click();
await wait(700);
await page.screenshot({ path: `${outDir}/j7-pdfa-verification.png` });
check(await until(async () => (await page.locator(".scard button", { hasText: "Conforme après enregistrement" }).count()) > 0), "PDF/A réalisable (corrigé à l'enregistrement)");
await wait(400);
await page.screenshot({ path: `${outDir}/j7-pdfa.png` });
await page.locator(".sec__foot .btn--primary").click();
check(await until(async () => (await page.locator(".sec__foot .tool__done").count()) > 0), "archivage appliqué");
await page.locator(".scard button", { hasText: "Vérifier la conformité" }).click();
check(await until(async () => (await page.locator(".scard button", { hasText: "Conforme PDF/A-2b" }).count()) > 0), "conforme PDF/A-2b après enregistrement");
await wait(300);
await page.screenshot({ path: `${outDir}/j7-pdfa-conforme.png` });
await closeActive();

console.log("Scénario jalon 7 réussi.");
await browser.close();
