import { describe, expect, it } from "vitest";
import { captionPos, defaultStyle, fromIsoDate, placeCaption, progress, stateName, toIsoDate, validEmail, type FormField } from "./forms";

const f = (kind: FormField["kind"], value: string, required = false): FormField => ({ name: kind + value, label: "", kind, value, options: [], required, readonly: false, multiline: false, maxLen: null, placeholder: "", style: defaultStyle(kind), widgets: [], caption: null });

describe("formulaires", () => {
  it("progression", () => {
    const p = progress([f("text", "Camille", true), f("email", "", true), f("check", ""), f("button", ""), f("sig", "signed")]);
    expect(p).toEqual({ total: 4, filled: 2, missing: 1, pct: 50 });
    expect(progress([]).pct).toBe(0);
  });

  it("dates", () => {
    expect(toIsoDate("14/03/1990")).toBe("1990-03-14");
    expect(toIsoDate("4.3.1990")).toBe("1990-03-04");
    expect(toIsoDate("demain")).toBe("");
    expect(fromIsoDate("1990-03-14")).toBe("14/03/1990");
  });

  it("e-mail", () => {
    expect(validEmail("")).toBe(true);
    expect(validEmail("camille@exemple.fr")).toBe(true);
    expect(validEmail("camille@")).toBe(false);
  });

  it("libellés visibles et options", () => {
    const r = { x: 100, y: 200, w: 120, h: 20 };
    expect(captionPos(placeCaption("Nom", 0, r, "above"), r)).toBe("above");
    expect(captionPos(placeCaption("Nom", 0, r, "left"), r)).toBe("left");
    expect(captionPos(placeCaption("Nom", 0, r, "right"), r)).toBe("right");
    expect(placeCaption("Nom", 0, r, "above").rect.y).toBeLessThan(200);
    expect(stateName("Téléphone portable", 0)).toBe("Telephoneportable");
    expect(stateName("—", 2)).toBe("Choix3");
  });
});
