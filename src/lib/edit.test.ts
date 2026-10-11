import { describe, expect, it } from "vitest";
import { buildPlan, defaultSettings, moveSelected, moveTo, paperName, shownSize, toggleSelection, type EditPage } from "./edit";

const pages = ["a", "b", "c", "d", "e"].map((key) => ({ key }));
const keys = (l: { key: string }[]) => l.map((p) => p.key).join("");

describe("module Éditer", () => {
  it("sélection au clic et par plage", () => {
    const order = keys(pages).split("");
    expect(toggleSelection(order, [], null, "b", false)).toEqual(["b"]);
    expect(toggleSelection(order, ["b"], "b", "b", false)).toEqual([]);
    expect(toggleSelection(order, ["b"], "b", "d", true)).toEqual(["b", "c", "d"]);
  });

  it("déplacement d'un cran, en bloc", () => {
    expect(keys(moveSelected(pages, ["b", "d"], -1))).toBe("badce");
    expect(keys(moveSelected(pages, ["a"], -1))).toBe("abcde");
    expect(keys(moveSelected(pages, ["d", "e"], 1))).toBe("abcde");
  });

  it("glisser-déposer à un index", () => {
    expect(keys(moveTo(pages, ["a"], 3))).toBe("bcade");
    expect(keys(moveTo(pages, ["d", "e"], 0))).toBe("deabc");
    expect(keys(moveTo(pages, ["b", "c"], 5))).toBe("adebc");
  });

  it("taille affichée après rotation", () => {
    expect(shownSize({ w: 595, h: 842, rot: 1 })).toEqual({ w: 842, h: 595 });
    expect(shownSize({ w: 595, h: 842, rot: -2 })).toEqual({ w: 595, h: 842 });
  });

  it("formats de papier", () => {
    expect(paperName(595.28, 841.89)).toBe("A4");
    expect(paperName(841.89, 595.28)).toBe("A4");
    expect(paperName(612, 792)).toBe("Lettre US");
    expect(paperName(283.46, 283.46)).toBe("100 × 100 mm");
  });

  it("plan d'enregistrement", () => {
    const list: EditPage[] = [0, 1].map((i) => ({ key: `k${i}`, src: { kind: "orig", index: i }, rot: i, w: 600, h: 800, view: null, crop: false, redact: [] }));
    const s = defaultSettings("Rapport.pdf");
    s.crop = { on: true, top: 10, bottom: 0, left: 0, right: 0, scope: "sel" };
    s.watermark.text = "  BROUILLON ";
    const links = new Map([[0, [{ op: "add" as const, rect: { x: 0, y: 0, w: 10, h: 10 }, uri: "" }, { op: "delete" as const, index: 2 }]]]);
    const p = buildPlan(list, s, new Set(["k1"]), links);
    expect(p.pages.map((x) => x.crop)).toEqual([false, true]);
    expect(p.crop?.top).toBeCloseTo(28.35, 1);
    expect(p.watermark?.text).toBe("BROUILLON");
    expect(p.watermark?.opacity).toBeCloseTo(0.18);
    expect(p.header).toBeNull();
    // Le lien tracé sans adresse n'est pas envoyé.
    expect(p.pages[0].links).toEqual([{ op: "delete", index: 2 }]);
    expect(p.pages[1].rot).toBe(1);
  });
});
