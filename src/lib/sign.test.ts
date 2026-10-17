import { describe, expect, it } from "vitest";
import { strokePath, strokesBounds, summary, type SignReport } from "./sign";

describe("signature", () => {
  it("tracé lissé du pad", () => {
    expect(strokePath([])).toBe("");
    expect(strokePath([[[0, 0], [10, 10]]])).toBe("M0.0 0.0 L10.0 10.0");
    const d = strokePath([[[0, 0], [10, 0], [20, 10], [30, 10]]], 2, 1, 1);
    expect(d.startsWith("M1.0 1.0 Q21.0 1.0 31.0 11.0")).toBe(true);
    expect(d.endsWith("L61.0 21.0")).toBe(true);
  });

  it("boîte englobante", () => {
    expect(strokesBounds([])).toBeNull();
    expect(strokesBounds([[[10, 20], [30, 25]], [[5, 40]]])).toEqual({ x: 5, y: 20, w: 25, h: 20 });
  });

  it("bilan des signatures", () => {
    const sig = (status: "valid" | "untrusted" | "invalid") => ({ field: "", kind: "digital" as const, signer: "", issuer: "", status, messages: [], date: null, timestamp: null, reason: null, location: null, modifiedAfter: false, certified: null, pades: false, page: 0, rect: null });
    const r: SignReport = { signatures: [sig("valid"), sig("untrusted"), sig("invalid")], empty: [{ field: "B", page: 1, rect: { x: 0, y: 0, w: 1, h: 1 } }] };
    expect(summary(r)).toEqual({ signed: 3, total: 4, invalid: 1, untrusted: 1 });
    expect(summary(null)).toEqual({ signed: 0, total: 0, invalid: 0, untrusted: 0 });
  });
});
