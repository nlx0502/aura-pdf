// Audit de performances sur « Gros document.pdf » (600 pages) : ouverture,
// affichage après un saut, fluidité du défilement, recherche, mémoire.
//   node scripts/audit-perf.mjs
import { chromium } from "playwright-core";
import { execSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const work = fs.mkdtempSync(path.join(os.tmpdir(), "aura-perf-"));
const big = path.join(work, "Gros document.pdf");
fs.copyFileSync(path.resolve("tests/fixtures/Gros document.pdf"), big);
const browser = await chromium.connectOverCDP(`http://127.0.0.1:${process.env.SHOT_PORT ?? 9222}`);
const page = browser.contexts()[0].pages().find((p) => !p.url().startsWith("devtools"));
const wait = (ms) => page.waitForTimeout(ms);
const results = {};

// Mémoire du processus Aura et de ses processus WebView2 (descendants seulement).
const PS = `$all = Get-CimInstance Win32_Process; $root = ($all | Where-Object { $_.Name -eq 'aura-pdf.exe' -and $_.ExecutablePath -like '*${process.env.AURA_BUILD ?? ""}*' } | Select-Object -First 1).ProcessId; $ids = @($root); $added = $true; while ($added) { $added = $false; foreach ($p in $all) { if ($ids -contains $p.ParentProcessId -and -not ($ids -contains $p.ProcessId)) { $ids += $p.ProcessId; $added = $true } } }; ($all | Where-Object { $ids -contains $_.ProcessId } | Measure-Object WorkingSetSize -Sum).Sum`;
const memory = () => {
  try {
    const out = execSync(`powershell -NoProfile -Command "${PS}"`).toString().trim();
    return Math.round(Number(out) / 1048576);
  } catch {
    return null;
  }
};

await page.evaluate(() => {
  const s = window.__aura.getState();
  for (const t of [...s.tabs]) s.closeTab(t.key);
  s.goHome();
});
await wait(1000);
results.memoryIdleMo = memory();

// Ouverture : jusqu'à l'affichage de la première page.
results.openMs = await page.evaluate(async (p) => {
  const t0 = performance.now();
  await window.__aura.getState().openPath(p);
  window.__aura.getState().setModule("lire");
  const s = window.__aura.getState();
  s.updateTab(s.active, { fit: "width", view: "continu" });
  for (;;) {
    const c = document.querySelector('.page[data-page="0"] canvas');
    if (c && c.width > 0) break;
    await new Promise((r) => setTimeout(r, 5));
  }
  return Math.round(performance.now() - t0);
}, big);

// Saut à une page lointaine : délai jusqu'à son affichage.
const jumps = [];
for (const target of [300, 599, 150, 450]) {
  jumps.push(
    await page.evaluate(async (pg) => {
      const s = window.__aura.getState();
      const t0 = performance.now();
      s.jumpTo(s.active, pg);
      for (;;) {
        const c = document.querySelector(`.page[data-page="${pg}"] canvas`);
        if (c && c.width > 0) break;
        if (performance.now() - t0 > 10000) return -1;
        await new Promise((r) => setTimeout(r, 5));
      }
      return Math.round(performance.now() - t0);
    }, target),
  );
}
results.jumpMs = jumps;

// Défilement continu (≈ 2 écrans par seconde pendant 4 s) : images par seconde et pages blanches.
results.scroll = await page.evaluate(async () => {
  const v = document.querySelector(".viewer");
  v.scrollTop = 0;
  await new Promise((r) => setTimeout(r, 500));
  let frames = 0;
  let blankSamples = 0;
  let samples = 0;
  let longest = 0;
  let last = performance.now();
  const t0 = performance.now();
  await new Promise((resolve) => {
    const step = (now) => {
      frames++;
      longest = Math.max(longest, now - last);
      last = now;
      v.scrollTop += (v.clientHeight * 2) / 60;
      if (frames % 10 === 0) {
        samples++;
        const vis = [...document.querySelectorAll(".page")].filter((p) => {
          const r = p.getBoundingClientRect();
          return r.bottom > 0 && r.top < innerHeight;
        });
        if (vis.some((p) => !(p.querySelector("canvas")?.width > 0))) blankSamples++;
      }
      if (now - t0 < 4000) requestAnimationFrame(step);
      else resolve();
    };
    requestAnimationFrame(step);
  });
  const secs = (performance.now() - t0) / 1000;
  return { fps: Math.round(frames / secs), longestFrameMs: Math.round(longest), blankPct: Math.round((blankSamples / samples) * 100) };
});

// Recherche dans les 600 pages : premier résultat et parcours complet.
await page.evaluate(() => window.__aura.getState().setModule("lire"));
await wait(400);
const t0 = Date.now();
await page.locator('input[placeholder^="Rechercher"]').first().fill("latence");
let first = null;
for (;;) {
  const st = await page.evaluate(() => {
    const s = window.__aura.getState();
    const t = s.tabs.find((x) => x.key === s.active);
    return { hits: t.search.hits.length, done: t.search.done && t.search.query === "latence" && t.search.token > 0 };
  });
  if (st.hits && first === null) first = Date.now() - t0;
  if (st.done || Date.now() - t0 > 30000) {
    results.search = { firstHitMs: first, allPagesMs: Date.now() - t0, hits: st.hits };
    break;
  }
  await wait(20);
}
await wait(1500);
results.memoryOpenMo = memory();
console.log(JSON.stringify(results, null, 2));
await browser.close();
