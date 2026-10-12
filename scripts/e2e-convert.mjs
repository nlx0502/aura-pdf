// Jalon 8 : Convertir (PDF → Word, Excel, images, texte ; Word → PDF) et OCR
// d'un document numérisé, puis création d'un PDF depuis un document Office.
//   node scripts/e2e-convert.mjs [dossier-captures] [--no-ocr]
import { chromium } from "playwright-core";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const outDir = process.argv[2] && !process.argv[2].startsWith("--") ? process.argv[2] : "docs/captures";
const withOcr = !process.argv.includes("--no-ocr");
const work = fs.mkdtempSync(path.join(os.tmpdir(), "aura-e2e-j8-"));
const report = path.join(work, "Rapport Q1 2025.pdf");
fs.copyFileSync(path.resolve("tests/fixtures/Rapport Q1 2025.pdf"), report);
const scan = path.join(work, "Scan contrat.pdf");
if (fs.existsSync("tests/fixtures/Scan contrat.pdf")) fs.copyFileSync(path.resolve("tests/fixtures/Scan contrat.pdf"), scan);

const browser = await chromium.connectOverCDP(`http://127.0.0.1:${process.env.SHOT_PORT ?? 9222}`);
const page = browser.contexts()[0].pages().find((p) => !p.url().startsWith("devtools"));
const wait = (ms) => page.waitForTimeout(ms);
const check = (ok, msg) => {
  if (!ok) throw new Error("ÉCHEC : " + msg);
  console.log("ok —", msg);
};
const until = async (fn, ms = 60000) => {
  const t0 = Date.now();
  while (Date.now() - t0 < ms) {
    if (await fn()) return true;
    await wait(200);
  }
  return false;
};
const conv = () => page.evaluate(() => window.__auraConvert.getState());

// ================================================================ Convertir
await page.evaluate(async (p) => {
  window.__auraConvert.setState({ files: [], outputs: null, tab: "conv", dir: "from", fmt: "word", running: false });
  await window.__aura.getState().openPath(p);
  window.__aura.getState().setModule("convert");
}, report);
await wait(1200);
check((await conv()).files.length === 1, "document actif proposé dans la file");
await page.screenshot({ path: `${outDir}/j8-convertir.png` });

for (const [fmt, ext] of [
  ["Word", ".docx"],
  ["Excel", ".xlsx"],
  ["Texte brut", ".txt"],
]) {
  await page.locator(".cfmt", { hasText: fmt }).click();
  await page.locator(".tool__side .btn--primary").click();
  if (fmt === "Word") {
    await wait(300);
    await page.screenshot({ path: `${outDir}/j8-conversion.png` });
  }
  check(await until(async () => !(await conv()).running && (await conv()).outputs), `conversion ${fmt}`);
  const out = (await conv()).outputs[0];
  check(out.path.endsWith(ext) && fs.existsSync(out.path) && fs.statSync(out.path).size > 500, `${path.basename(out.path)} écrit`);
  if (fmt === "Word") {
    await wait(500);
    await page.screenshot({ path: `${outDir}/j8-converti.png` });
  }
}
await page.locator(".cfmt", { hasText: "Images" }).click();
await page.locator(".tool__side .chip", { hasText: "150 ppp" }).click();
await page.locator(".tool__side .btn--primary").click();
check(await until(async () => !(await conv()).running && (await conv()).outputs), "conversion en images");
const imgs = (await conv()).outputs[0];
check(fs.statSync(imgs.path).isDirectory() && fs.readdirSync(imgs.path).length === 14, "14 images PNG");

// Vers un PDF : le document Word produit.
const docx = path.join(work, "Rapport Q1 2025.docx");
await page.locator(".seg__btn", { hasText: "Vers un PDF" }).click();
await page.locator(".cfmt", { hasText: "Word" }).click();
await page.evaluate((p) => window.__auraConvert.getState().addFiles([p]), docx);
await wait(300);
await page.screenshot({ path: `${outDir}/j8-vers-pdf.png` });
await page.locator(".tool__side .btn--primary").click();
check(await until(async () => !(await conv()).running && (await conv()).outputs, 120000), "Word → PDF");
const back = (await conv()).outputs[0];
check(back.path.endsWith(".pdf") && back.pages >= 10, `PDF produit (${back.pages} pages)`);
await wait(400);

// ================================================================ OCR
if (withOcr && fs.existsSync(scan)) {
  await page.evaluate(async (p) => {
    await window.__aura.getState().openPath(p);
    window.__auraConvert.setState({ tab: "ocr", ocrPages: [], ocrOutput: null });
    window.__aura.getState().setModule("convert");
  }, scan);
  await wait(1200);
  await page.screenshot({ path: `${outDir}/j8-ocr.png` });
  await page.locator(".tool__side .btn--primary").click();
  check(await until(async () => (await conv()).ocrPage !== null), "OCR lancé");
  await wait(600);
  await page.screenshot({ path: `${outDir}/j8-ocr-balayage.png` });
  check(await until(async () => !(await conv()).running && (await conv()).ocrOutput, 180000), "OCR terminé");
  const st = await conv();
  const words = st.ocrPages.flatMap((p) => p.lines.map((l) => l.t)).join(" ");
  check(/contrat/i.test(words) && /prestat/i.test(words), `texte reconnu : « ${words.slice(0, 80)}… »`);
  await wait(1200);
  await page.screenshot({ path: `${outDir}/j8-ocr-texte.png` });
  // Le PDF produit est recherchable.
  const out = st.ocrOutput.path;
  const found = await page.evaluate(async (p) => {
    await window.__aura.getState().openPath(p);
    const s = window.__aura.getState();
    const tab = s.tabs.find((t) => t.key === s.active);
    const words = await window.__TAURI_INTERNALS__.invoke("page_text", { doc: tab.info.id, page: 0 });
    return words.map((w) => w.t).join(" ");
  }, out);
  check(/contrat/i.test(found), "PDF recherchable : texte présent dans la page");
}

// ================================================================ Créer un PDF
for (const source of ["doc", "scan", "clip"]) {
  await page.evaluate((src) => window.__aura.getState().set({ create: { open: true, source: src, images: [] } }), source);
  await wait(700);
  await page.screenshot({ path: `${outDir}/j8-creer-${source}.png` });
  await page.keyboard.press("Escape");
  await wait(300);
}

console.log("Scénario jalon 8 réussi.");
await browser.close();
