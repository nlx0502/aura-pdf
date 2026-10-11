import { describe, expect, it } from "vitest";
import { formatPdfDate, formatSize, formatWhen, groupOf, readingPct, stripExt } from "./format";

const nbsp = (s: string) => s.replace(/[  ]/g, " ");

describe("formatSize", () => {
  it("affiche les unités françaises", () => {
    expect(nbsp(formatSize(2.4 * 1024 * 1024))).toBe("2,4 Mo");
    expect(nbsp(formatSize(380 * 1024))).toBe("380 Ko");
    expect(formatSize(0)).toBe("—");
  });
});

describe("groupOf", () => {
  const now = new Date(2026, 9, 10, 15, 0); // samedi 10 octobre 2026
  it("regroupe par période", () => {
    expect(groupOf(new Date(2026, 9, 10, 8, 0), now)).toBe("today");
    expect(groupOf(new Date(2026, 9, 9, 23, 59), now)).toBe("yesterday");
    expect(groupOf(new Date(2026, 9, 5, 12, 0), now)).toBe("week");
    expect(groupOf(new Date(2026, 8, 28, 12, 0), now)).toBe("older");
  });
});

describe("formatWhen", () => {
  const now = new Date(2026, 9, 10, 15, 0);
  it("formulations relatives", () => {
    expect(formatWhen(new Date(2026, 9, 10, 14, 55), now)).toBe("Il y a 5 min");
    expect(formatWhen(new Date(2026, 9, 10, 13, 0), now)).toBe("Il y a 2 h");
    expect(formatWhen(new Date(2026, 9, 9, 17, 40), now)).toBe("Hier, 17:40");
    expect(formatWhen(new Date(2026, 9, 5, 10, 0), now)).toBe("Lundi");
  });
});

describe("divers", () => {
  it("date PDF", () => {
    expect(formatPdfDate("D:20250114093000+01'00'")).toBe("14 janvier 2025");
    expect(formatPdfDate("n'importe quoi")).toBeNull();
  });
  it("progression de lecture", () => {
    expect(readingPct(1, 14)).toBe(14);
    expect(readingPct(13, 14)).toBe(100);
  });
  it("extension", () => {
    expect(stripExt("Rapport Q1.pdf")).toBe("Rapport Q1");
  });
});
