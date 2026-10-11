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

/** Abonnement unique aux événements de recherche. */
export function initSearchEvents() {
  if (listening) return;
  listening = true;
  void listen<ProgressPayload>("search:progress", (e) => {
    const p = e.payload;
    const { tabs, updateTab } = useApp.getState();
    const tab = tabs.find((t) => t.info.id === p.doc && t.search.token === p.token);
    if (!tab) return;
    updateTab(tab.key, (t) => {
      const hits = p.hits.length ? [...t.search.hits, ...p.hits] : t.search.hits;
      // Premier résultat : le sélectionner et y aller.
      let active = t.search.active;
      if (active < 0 && hits.length) active = 0;
      return { search: { ...t.search, hits, scanned: p.scanned, active } };
    });
    const after = useApp.getState().tabs.find((t) => t.key === tab.key);
    if (after && tab.search.active < 0 && after.search.active === 0) gotoHit(tab.key, 0);
  });
  void listen<{ doc: number; token: number }>("search:done", (e) => {
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
