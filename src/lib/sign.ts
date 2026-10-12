/**
 * Module Signer : types échangés avec le cœur Rust et fonctions pures.
 */
import { invoke } from "@tauri-apps/api/core";
import type { DocInfo, Rect } from "./api";

export type SigStatus = "valid" | "untrusted" | "invalid" | "visual" | "unknown";

export interface SigReport {
  field: string;
  kind: "digital" | "visual";
  signer: string;
  issuer: string;
  status: SigStatus;
  messages: string[];
  date: string | null;
  timestamp: string | null;
  reason: string | null;
  location: string | null;
  modifiedAfter: boolean;
  page: number | null;
  rect: Rect | null;
}

export interface EmptyField {
  field: string;
  page: number;
  rect: Rect;
}

export interface SignReport {
  signatures: SigReport[];
  empty: EmptyField[];
}

export interface CertInfo {
  name: string;
  issuer: string;
  organization: string | null;
  notAfter: number;
  selfSigned: boolean;
  expired: boolean;
}

export type Visual = { kind: "draw"; strokes: [number, number][][] } | { kind: "type"; text: string; font: SigFont } | { kind: "image"; path: string };

export type SigFont = "caveat" | "apple" | "delafield";

export interface SignRequest {
  page: number;
  rect: Rect;
  visual: { visual: Visual; color: string; caption: string[] };
  field: string | null;
  cert: { path: string; password: string; tsa: string | null; reason: string; location: string } | null;
  author: string;
}

export const signApi = {
  report: (path: string) => invoke<SignReport>("sign_report", { path }),
  certInspect: (path: string, password: string) => invoke<CertInfo>("cert_inspect", { path, password }),
  certCreate: (name: string, email: string, organization: string, password: string, out: string) => invoke<CertInfo>("cert_create", { name, email, organization, password, out }),
  start: (job: number, path: string, req: SignRequest) => invoke<void>("sign_start", { job, path, req }),
  reload: (doc: number) => invoke<DocInfo>("reload_document", { doc }),
};

/** Polices manuscrites fournies avec l'application. */
export const SIG_FONTS: { id: SigFont; family: string; css: string }[] = [
  { id: "caveat", family: "Caveat", css: "500 34px/1 Caveat, cursive" },
  { id: "apple", family: "Homemade Apple", css: "400 22px/1 'Homemade Apple', cursive" },
  { id: "delafield", family: "Mrs Saint Delafield", css: "400 38px/1 'Mrs Saint Delafield', cursive" },
];

export const INKS = [
  { name: "encre bleue", hex: "#1F2A6B" },
  { name: "noir", hex: "#1A1B1E" },
  { name: "bleu", hex: "#2563C9" },
];

export const DEFAULT_TSA = "http://timestamp.digicert.com";

/** Tracé SVG lissé (courbes passant par les milieux des segments, comme dans le PDF). */
export function strokePath(strokes: [number, number][][], scale = 1, dx = 0, dy = 0): string {
  const t = (p: [number, number]) => `${(p[0] * scale + dx).toFixed(1)} ${(p[1] * scale + dy).toFixed(1)}`;
  return strokes
    .filter((s) => s.length)
    .map((s) => {
      if (s.length < 3) return `M${t(s[0])} L${t(s[s.length - 1])}`;
      let d = `M${t(s[0])}`;
      for (let i = 1; i < s.length - 1; i++) {
        const mid: [number, number] = [(s[i][0] + s[i + 1][0]) / 2, (s[i][1] + s[i + 1][1]) / 2];
        d += ` Q${t(s[i])} ${t(mid)}`;
      }
      return `${d} L${t(s[s.length - 1])}`;
    })
    .join(" ");
}

/** Boîte englobante des traits (null si vide). */
export function strokesBounds(strokes: [number, number][][]): { x: number; y: number; w: number; h: number } | null {
  const pts = strokes.flat();
  if (!pts.length) return null;
  const xs = pts.map((p) => p[0]);
  const ys = pts.map((p) => p[1]);
  const x = Math.min(...xs);
  const y = Math.min(...ys);
  return { x, y, w: Math.max(1, Math.max(...xs) - x), h: Math.max(1, Math.max(...ys) - y) };
}

/** Le document est-il entièrement signé avec des signatures intactes ? */
export function summary(r: SignReport | null) {
  if (!r) return { signed: 0, total: 0, invalid: 0, untrusted: 0 };
  const digital = r.signatures.filter((s) => s.kind === "digital");
  return {
    signed: r.signatures.length,
    total: r.signatures.length + r.empty.length,
    invalid: digital.filter((s) => s.status === "invalid").length,
    untrusted: digital.filter((s) => s.status === "untrusted").length,
  };
}
