import { describe, expect, it } from "vitest";
import type { TextWord } from "./api";
import { arrowHead, blankAnnot, bounds, nearestWord, quoteFor, simplify, translate, wordRects } from "./annot";

const w = (t: string, x: number, y: number): TextWord => ({ t, x, y, w: t.length * 6, h: 12, sp: true, nl: false });
const words = [w("Les", 10, 10), w("annexes", 40, 10), w("techniques", 100, 10), w("restent", 10, 30), w("la", 60, 30)];

describe("sélection de mots", () => {
  it("mot le plus proche", () => {
    expect(nearestWord(words, 45, 15)).toBe(1);
    expect(nearestWord(words, 15, 33)).toBe(3);
    expect(nearestWord(words, 900, 900)).toBe(-1);
  });
  it("un rectangle par ligne, sens indifférent", () => {
    const r = wordRects(words, 4, 1);
    expect(r).toHaveLength(2);
    expect(r[0].x).toBe(40);
    expect(r[1].y).toBe(30);
  });
  it("citation", () => {
    expect(quoteFor(words, wordRects(words, 1, 2))).toBe("annexes techniques");
  });
});

describe("géométrie", () => {
  it("déplacement et boîte", () => {
    const a = blankAnnot({ page: 0, kind: "ink", ink: [[[0, 0], [10, 20]]] });
    const moved = { ...a, ...translate(a, 5, 5) };
    expect(bounds(moved)).toEqual({ x: 5, y: 5, w: 10, h: 20 });
  });
  it("simplification d'un tracé rectiligne", () => {
    const pts = Array.from({ length: 50 }, (_, i) => [i, i] as [number, number]);
    expect(simplify(pts)).toHaveLength(2);
  });
  it("pointe de flèche symétrique", () => {
    const h = arrowHead([0, 0, 100, 0], 2);
    expect(h[1]).toEqual([100, 0]);
    expect(h[0][1]).toBeCloseTo(-h[2][1]);
  });
});
