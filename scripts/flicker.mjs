// Détection de clignotement : enregistre les images composées pendant une
// interaction et signale les images anormalement petites (écran vide/noir).
//   node scripts/flicker.mjs "<script js déclenchant l'interaction>" [durée-ms]
import { chromium } from "playwright-core";

const [, , js = "", dur = "800"] = process.argv;
const browser = await chromium.connectOverCDP(`http://127.0.0.1:${process.env.SHOT_PORT ?? 9222}`);
const ctx = browser.contexts()[0];
const page = ctx.pages().find((p) => !p.url().startsWith("devtools")) ?? ctx.pages()[0];
const cdp = await ctx.newCDPSession(page);
const sizes = [];
cdp.on("Page.screencastFrame", async (f) => {
  sizes.push(Math.round((f.data.length * 3) / 4 / 1024));
  await cdp.send("Page.screencastFrameAck", { sessionId: f.sessionId }).catch(() => {});
});
await cdp.send("Page.startScreencast", { format: "jpeg", quality: 60, everyNthFrame: 1 });
await page.waitForTimeout(300);
if (js) await page.evaluate(js);
await page.waitForTimeout(Number(dur));
await cdp.send("Page.stopScreencast");
const median = [...sizes].sort((a, b) => a - b)[Math.floor(sizes.length / 2)] ?? 0;
const suspicious = sizes.filter((s) => s < median * 0.5);
console.log(JSON.stringify({ frames: sizes.length, median, sizesKo: sizes, suspicious: suspicious.length }));
await browser.close();
