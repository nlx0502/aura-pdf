/**
 * Recherche dans le document : les résultats arrivent page par page depuis
 * le cœur Rust (événements `search:progress` / `search:done`).
 */
import { listen } from "@tauri-apps/api/event";
import { api, type SearchHit } from "../../lib/api";
import { emptySearch, useApp } from "../../stores/app";

let tokenSeq = 1;
let listening = false;

interface ProgressPayload {
  doc: number;
  token: number;
  hits: SearchHit[];
  scanned: number;
  pageCount: number;
  total: number;
}

/** Résultats reçus en attente d'affichage (regroupés par image : un seul rendu par trame). */
let pending: ProgressPayload[] = [];
let frame = 0;

function flush() {
  frame = 0;
  const batch = pending;
  pending = [];
  const byTab = new Map<string, { hits: SearchHit[]; scanned: number }>();
  const { tabs, updateTab } = useApp.getState();
  for (const p of batch) {
    const tab = tabs.find((t) => t.info.id === p.doc && t.search.token === p.token);
    if (!tab) continue;
    const cur = byTab.get(tab.key) ?? { hits: [], scanned: 0 };
    cur.hits.push(...p.hits);
    cur.scanned = Math.max(cur.scanned, p.scanned);
    byTab.set(tab.key, cur);
  }
  for (const [key, add] of byTab) {
    const before = useApp.getState().tabs.find((t) => t.key === key);
    updateTab(key, (t) => {
      const hits = add.hits.length ? [...t.search.hits, ...add.hits] : t.search.hits;
      // Premier résultat : le sélectionner et y aller.
      let active = t.search.active;
      if (active < 0 && hits.length) active = 0;
      return { search: { ...t.search, hits, scanned: Math.max(t.search.scanned, add.scanned), active } };
    });
    const after = useApp.getState().tabs.find((t) => t.key === key);
    if (before && after && before.search.active < 0 && after.search.active === 0) gotoHit(key, 0);
  }
}

/** Abonnement unique aux événements de recherche. */
export function initSearchEvents() {
  if (listening) return;
  listening = true;
  void listen<ProgressPayload>("search:progress", (e) => {
    pending.push(e.payload);
    if (!frame) frame = requestAnimationFrame(flush);
  });
  void listen<{ doc: number; token: number }>("search:done", (e) => {
    // Les derniers résultats sont affichés avant de marquer la fin.
    if (frame) {
      cancelAnimationFrame(frame);
      flush();
    }
    const { tabs, updateTab } = useApp.getState();
    const tab = tabs.find((t) => t.info.id === e.payload.doc && t.search.token === e.payload.token);
    if (tab) updateTab(tab.key, (t) => ({ search: { ...t.search, done: true, scanned: t.info.pageCount } }));
  });
}

/** Lance (ou efface) une recherche ; les recherches de moins de 2 caractères sont ignorées. */
export function runSearch(key: string, query: string) {
  const { tabs, updateTab } = useApp.getState();
  const tab = tabs.find((t) => t.key === key);
  if (!tab) return;
  const q = query.trim();
  if (q.length < 2) {
    void api.searchCancel(tab.info.id).catch(() => {});
    updateTab(key, { search: { ...emptySearch(), query } });
    return;
  }
  const token = tokenSeq++;
  updateTab(key, { search: { query, token, hits: [], scanned: 0, done: false, active: -1 } });
  void api.searchStart(tab.info.id, q, token).catch(() => {});
}

/** Va au résultat n° `index` (avec bouclage). */
export function gotoHit(key: string, index: number) {
  const { tabs, updateTab, jumpTo } = useApp.getState();
  const tab = tabs.find((t) => t.key === key);
  if (!tab || !tab.search.hits.length) return;
  const n = tab.search.hits.length;
  const i = ((index % n) + n) % n;
  const hit = tab.search.hits[i];
  updateTab(key, (t) => ({ search: { ...t.search, active: i } }));
  jumpTo(key, hit.page, hit.rects[0]?.y ?? 0);
}
