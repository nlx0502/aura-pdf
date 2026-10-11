import { describe, expect, it } from "vitest";
import {
  anchorAt,
  anchorPoint,
  computeLayout,
  currentPageAt,
  fitWidthZoom,
  nextZoom,
  PAD_TOP,
  rotateRect,
  visiblePages,
} from "./layout";

const A4 = { w: 595, h: 842 };
const pages = Array.from({ length: 600 }, () => A4);

describe("computeLayout", () => {
  it("empile les pages en continu", () => {
    const l = computeLayout({ pages, zoom: 100, rot: 0, mode: "continu", current: 0, viewportW: 1000 });
    expect(l.shown.length).toBe(600);
    expect(l.boxes[0].y).toBe(PAD_TOP);
    expect(l.boxes[1].y).toBeGreaterThan(l.boxes[0].y + l.boxes[0].h);
  });

  it("dispose deux pages par rangée en mode double", () => {
    const l = computeLayout({ pages: pages.slice(0, 5), zoom: 50, rot: 0, mode: "double", current: 0, viewportW: 800 });
    expect(l.boxes[0].y).toBe(l.boxes[1].y);
    expect(l.boxes[2].y).toBeGreaterThan(l.boxes[0].y);
    expect(l.shown).toEqual([0, 1, 2, 3, 4]);
  });

  it("n'affiche que la page courante en mode page", () => {
    const l = computeLayout({ pages: pages.slice(0, 10), zoom: 100, rot: 0, mode: "page", current: 4, viewportW: 1000 });
    expect(l.shown).toEqual([4]);
  });

  it("intervertit largeur et hauteur après un quart de tour", () => {
    const l = computeLayout({ pages: [A4], zoom: 100, rot: 1, mode: "continu", current: 0, viewportW: 1000 });
    expect(l.boxes[0].w).toBeGreaterThan(l.boxes[0].h);
  });
});

describe("visibilité et ancres", () => {
  const l = computeLayout({ pages, zoom: 100, rot: 0, mode: "continu", current: 0, viewportW: 1000 });

  it("trouve les pages visibles par dichotomie sur 600 pages", () => {
    const top = l.boxes[300].y + 10;
    const vis = visiblePages(l, top, top + 800);
    expect(vis[0]).toBe(300);
    expect(vis.length).toBeLessThanOrEqual(3);
  });

  it("page courante", () => {
    expect(currentPageAt(l, l.boxes[42].y, 900)).toBe(42);
  });

  it("une ancre survit à un changement de zoom", () => {
    const cy = l.boxes[120].y + l.boxes[120].h * 0.25;
    const a = anchorAt(l, 500, cy);
    expect(a.page).toBe(120);
    const l2 = computeLayout({ pages, zoom: 200, rot: 0, mode: "continu", current: 0, viewportW: 1000 });
    const p = anchorPoint(l2, a);
    expect(p.y).toBeCloseTo(l2.boxes[120].y + l2.boxes[120].h * 0.25, 3);
  });
});

describe("zoom", () => {
  it("ajuste à la largeur", () => {
    const z = fitWidthZoom([A4], 0, "continu", 1000);
    expect(z).toBeGreaterThan(100);
    expect(z).toBeLessThan(130);
  });
  it("paliers", () => {
    expect(nextZoom(100, 1)).toBe(110);
    expect(nextZoom(100, -1)).toBe(90);
    expect(nextZoom(103, -1)).toBe(100);
  });
});

describe("rotateRect", () => {
  it("un tour complet revient au point de départ", () => {
    const r = { x: 10, y: 20, w: 30, h: 5 };
    let cur = r;
    let w = 100;
    let h = 200;
    for (let i = 0; i < 4; i++) {
      cur = rotateRect(cur, w, h, 1);
      [w, h] = [h, w];
    }
    expect(cur).toEqual(r);
  });
});
