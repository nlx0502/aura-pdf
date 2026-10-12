import { describe, expect, it } from "vitest";
import { ALL_PERMISSIONS } from "./api";
import { conflict, countPerms, strength } from "./security";

describe("sécurité", () => {
  it("robustesse du mot de passe", () => {
    expect(strength("")).toBe(0);
    expect(strength("abc")).toBe(0);
    expect(strength("aaaaaaaaaaaaaaaa")).toBeLessThanOrEqual(1);
    expect(strength("12345678")).toBeLessThanOrEqual(1);
    expect(strength("Aura2025")).toBe(3);
    expect(strength("Aura-2025")).toBe(4);
    expect(strength("correct horse battery staple")).toBeGreaterThanOrEqual(3);
  });

  it("PDF/A incompatible avec le chiffrement", () => {
    const p = ALL_PERMISSIONS;
    expect(conflict({ protect: false, permissions: p, pdfa: "2b" })).toBe(false);
    expect(conflict({ protect: true, permissions: p, pdfa: "2b" })).toBe(true);
    expect(conflict({ protect: false, permissions: { ...p, copy: false }, pdfa: "1b" })).toBe(true);
    expect(conflict({ protect: true, permissions: p, pdfa: null })).toBe(false);
    expect(countPerms({ ...p, copy: false, print: false })).toBe(4);
  });
});
