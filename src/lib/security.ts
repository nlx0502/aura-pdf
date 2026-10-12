/**
 * Module Sécurité : état de protection d'un document, application (mot de
 * passe, chiffrement, permissions, métadonnées, PDF/A), contrôle PDF/A.
 */
import { invoke } from "@tauri-apps/api/core";
import type { DocInfo, Permissions } from "./api";

export type Algorithm = "none" | "rc4-40" | "rc4-128" | "aes-128" | "aes-256";
export type PdfaLevel = "1b" | "2b" | "3b";

export interface Meta {
  title: string;
  author: string;
  subject: string;
  keywords: string;
}

export interface SecurityInfo {
  algorithm: Algorithm;
  openPassword: boolean;
  encryptMetadata: boolean;
  permissions: Permissions;
  metadata: Meta;
  pdfa: string | null;
  owner: boolean;
}

export interface SecuritySpec {
  protect: boolean;
  userPassword: string;
  ownerPassword: string;
  aes256: boolean;
  encryptMetadata: boolean;
  permissions: Permissions;
  metadata: Meta;
  pdfa: PdfaLevel | null;
}

export type CheckId = "fonts" | "colors" | "metadata" | "dynamic" | "encryption" | "attachments" | "transparency";
export interface PdfaCheck {
  id: CheckId;
  ok: boolean;
  count: number;
  /** Corrigé automatiquement par la conversion. */
  fixable: boolean;
}

export const securityApi = {
  info: (doc: number) => invoke<SecurityInfo>("security_info", { doc }),
  pdfaCheck: (doc: number, level: PdfaLevel) => invoke<PdfaCheck[]>("pdfa_check", { doc, level }),
  apply: (job: number, doc: number, spec: SecuritySpec) => invoke<void>("security_apply", { job, doc, spec }),
  reload: (doc: number, password: string | null) => invoke<DocInfo>("security_reload", { doc, password }),
};

export const PERM_KEYS: (keyof Permissions)[] = ["print", "copy", "edit", "annotate", "forms", "extract"];

/** Robustesse d'un mot de passe, de 0 (vide ou très faible) à 4 (excellent). */
export function strength(pw: string): number {
  if (!pw) return 0;
  let s = 0;
  if (pw.length >= 8) s++;
  if (/[a-z]/.test(pw) && /[A-Z]/.test(pw)) s++;
  if (/\d/.test(pw)) s++;
  if (/[^A-Za-z0-9]/.test(pw)) s++;
  if (pw.length >= 14) s = Math.min(4, s + 1);
  // Répétitions et suites évidentes : plafonnées.
  if (/^(.)\1+$/.test(pw) || /^(?:0123|1234|abcd|azer|qwer|motdepasse|password)/i.test(pw)) s = Math.min(s, 1);
  return s;
}

export const STRENGTH_COLORS = ["#EF6B6B", "#EF6B6B", "#F2B84B", "#8B87FA", "#4FCB91"];

/** Protection demandée incompatible avec l'archivage (PDF/A interdit le chiffrement). */
export function conflict(spec: Pick<SecuritySpec, "protect" | "permissions" | "pdfa">): boolean {
  return !!spec.pdfa && (spec.protect || PERM_KEYS.some((k) => !spec.permissions[k]));
}

export const countPerms = (p: Permissions) => PERM_KEYS.filter((k) => p[k]).length;
