import { describe, expect, it } from "vitest";
import { estimate, parseRanges, partsFromCuts, PRESETS, type Analysis } from "./tools";

describe("outils", () => {
  it("plages de pages", () => {
    expect(parseRanges("1-4, 5-10, 11-14", 14)).toEqual({ parts: [[1, 4], [5, 10], [11, 14]], bad: false });
    expect(parseRanges("3; 7–9", 10)).toEqual({ parts: [[3, 3], [7, 9]], bad: false });
    expect(parseRanges("1-20", 14).bad).toBe(true);
    expect(parseRanges("5-2", 14).bad).toBe(true);
    expect(parseRanges("", 14).bad).toBe(true);
    expect(parseRanges("a-b", 14).bad).toBe(true);
  });

  it("coupures par signets", () => {
    expect(partsFromCuts([4, 9], 14)).toEqual([[1, 3], [4, 8], [9, 14]]);
    expect(partsFromCuts([9, 4, 4, 1, 99], 14)).toEqual([[1, 3], [4, 8], [9, 14]]);
    expect(partsFromCuts([], 5)).toEqual([[1, 5]]);
  });

  it("estimation de la compression", () => {
    const a: Analysis = {
      total: 10_000_000,
      pages: 10,
      images: 8_000_000,
      fonts: 1_000_000,
      content: 900_000,
      meta: 100_000,
      thumbs: 50_000,
      attach: 0,
      uncompressed: 200_000,
      objects: 500,
      imageList: [{ bytes: 8_000_000, w: 4000, h: 3000, comps: 3, jpeg: true, ppi: 600, fixed: false }],
      fontList: [{ bytes: 1_000_000, standard: false, subsettable: true }],
    };
    const light = estimate(a, PRESETS.light);
    const rec = estimate(a, PRESETS.rec);
    const strong = estimate(a, PRESETS.strong);
    // Plus le niveau est fort, plus le fichier est petit ; jamais plus gros que l'original.
    expect(light.total).toBeLessThan(a.total);
    expect(rec.total).toBeLessThan(light.total);
    expect(strong.total).toBeLessThan(rec.total);
    expect(rec.fonts).toBeCloseTo(350_000, -3);
    // Image déjà sous la résolution visée : seule la qualité joue.
    const small = estimate({ ...a, imageList: [{ ...a.imageList[0], ppi: 100 }] }, PRESETS.light);
    expect(small.images).toBeLessThanOrEqual(8_000_000);
  });
});
