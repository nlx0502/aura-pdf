// Capture d'écran de l'application en cours d'exécution (mode développement).
//
// Lancer l'application avec le débogage distant WebView2 :
//   $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS='--remote-debugging-port=9222'; npm run app:dev
// puis :
//   node scripts/shot.mjs <sortie.png> [script-js-à-exécuter-avant] [attente-ms]
import { chromium } from "playwright-core";

const [, , out = "shot.png", js = "", wait = "600"] = process.argv;
const browser = await chromium.connectOverCDP(`http://127.0.0.1:${process.env.SHOT_PORT ?? 9222}`);
const ctx = browser.contexts()[0];
const page = ctx.pages().find((p) => !p.url().startsWith("devtools")) ?? ctx.pages()[0];
// Largeur émulée (ex. SHOT_W=1100) pour vérifier le repli des panneaux.
if (process.env.SHOT_W) {
  const cdp = await ctx.newCDPSession(page);
  await cdp.send("Emulation.setDeviceMetricsOverride", { width: Number(process.env.SHOT_W), height: Number(process.env.SHOT_H ?? 800), deviceScaleFactor: 0, mobile: false });
  await page.waitForTimeout(400);
}
if (process.env.SHOT_RESET) {
  const cdp = await ctx.newCDPSession(page);
  await cdp.send("Emulation.clearDeviceMetricsOverride");
}
if (js) {
  const res = await page.evaluate(js);
  if (res !== undefined) console.log(JSON.stringify(res));
}
await page.waitForTimeout(Number(wait));
await page.screenshot({ path: out });
console.log("capture :", out, page.viewportSize());
await browser.close();
