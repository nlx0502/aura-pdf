import { describe, expect, it } from "vitest";
import type { RecentEntry } from "./api";
import { filterRecents, groupRecents, tagOf } from "./recents";

function entry(p: Partial<RecentEntry>): RecentEntry {
  return {
    path: "C:\\x.pdf",
    name: "x.pdf",
    pageCount: 1,
    fileSize: 1,
    lastPage: 0,
    openedAt: new Date().toISOString(),
    favorite: false,
    signed: false,
    form: false,
    scanned: false,
    annotated: false,
    marks: [],
    folder: "Documents",
    place: "docs",
    exists: true,
    ...p,
  };
}

describe("filterRecents", () => {
  const list = [
    entry({ name: "Contrat signé.pdf", signed: true, favorite: true }),
    entry({ name: "Formulaire.pdf", form: true, place: "dl", folder: "Téléchargements" }),
    entry({ name: "Scan.pdf", scanned: true, place: "desk" }),
  ];
  const base = { q: "", view: "home" as const, place: null, filter: "all" as const };

  it("recherche par nom et dossier", () => {
    expect(filterRecents(list, { ...base, q: "contrat" })).toHaveLength(1);
    expect(filterRecents(list, { ...base, q: "télécharg" })).toHaveLength(1);
  });
  it("favoris, emplacement, étiquette", () => {
    expect(filterRecents(list, { ...base, view: "fav" })).toHaveLength(1);
    expect(filterRecents(list, { ...base, place: "desk" })[0].name).toBe("Scan.pdf");
    expect(filterRecents(list, { ...base, filter: "form" })[0].name).toBe("Formulaire.pdf");
  });
});

describe("groupRecents", () => {
  it("trie et regroupe du plus récent au plus ancien", () => {
    const now = new Date(2026, 9, 10, 12);
    const items = [new Date(2026, 8, 1), new Date(2026, 9, 10, 9), new Date(2026, 9, 9, 9), new Date(2026, 9, 6)];
    const groups = groupRecents(items, (d) => d, now);
    expect(groups.map((g) => g.group)).toEqual(["today", "yesterday", "week", "older"]);
  });
});

describe("tagOf", () => {
  it("priorité des étiquettes", () => {
    expect(tagOf({ signed: true, form: true, scanned: false, annotated: false })).toBe("signed");
    expect(tagOf({ signed: false, form: false, scanned: false, annotated: false })).toBeNull();
  });
});
