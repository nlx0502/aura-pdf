/**
 * Accès typé au cœur Rust (commandes Tauri).
 */
import { invoke } from "@tauri-apps/api/core";

export interface PageSize {
  w: number;
  h: number;
}

export interface OutlineItem {
  title: string;
  depth: number;
  page: number | null;
}

export interface AttachmentInfo {
  index: number;
  name: string;
  size: number;
}

export interface DocMetadata {
  title?: string;
  author?: string;
  subject?: string;
  keywords?: string;
  creator?: string;
  producer?: string;
  created?: string;
  modified?: string;
}

export interface Permissions {
  print: boolean;
  copy: boolean;
  edit: boolean;
  annotate: boolean;
  forms: boolean;
  /** Insérer, faire pivoter, supprimer ou extraire des pages. */
  extract: boolean;
}

export const ALL_PERMISSIONS: Permissions = { print: true, copy: true, edit: true, annotate: true, forms: true, extract: true };

/** Droits d'un document (les anciennes réponses sans droits valent « tous »). */
export const perms = (info: { permissions?: Permissions }): Permissions => info.permissions ?? ALL_PERMISSIONS;

export interface DocInfo {
  id: number;
  path: string;
  name: string;
  fileSize: number;
  pageCount: number;
  pages: PageSize[];
  version: string;
  metadata: DocMetadata;
  outline: OutlineItem[];
  attachments: AttachmentInfo[];
  hasForm: boolean;
  signatureCount: number;
  encrypted: boolean;
  /** Droits de l'utilisateur (tous si le document n'est pas chiffré). */
  permissions: Permissions;
  scanned: boolean;
  tagged: boolean;
}

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface SearchHit {
  page: number;
  rects: Rect[];
  before: string;
  matched: string;
  after: string;
}

export interface TextWord {
  t: string;
  x: number;
  y: number;
  w: number;
  h: number;
  sp: boolean;
  nl: boolean;
}

export interface PageLink {
  rect: Rect;
  page: number | null;
  uri: string | null;
}

export interface UserMark {
  title: string;
  page: number;
  color: string;
}

export interface RecentEntry {
  path: string;
  name: string;
  pageCount: number;
  fileSize: number;
  lastPage: number;
  openedAt: string;
  favorite: boolean;
  signed: boolean;
  form: boolean;
  scanned: boolean;
  annotated: boolean;
  marks: UserMark[];
  folder: string;
  place: "docs" | "desk" | "dl" | null;
  exists: boolean;
}

export interface FileEntry {
  path: string;
  name: string;
  folder: string;
  fileSize: number;
  modified: string | null;
}

export interface AppErrorPayload {
  code: string;
  message: string;
}

export type CreateSpec =
  | { kind: "blank"; format: PaperFormat; landscape: boolean; pages: number; style: "blank" | "lines" | "grid" | "dots" }
  | { kind: "images"; paths: string[]; format: PaperFormat; landscape: boolean; onePerPage: boolean }
  | { kind: "scans"; paths: string[] };

export type PaperFormat = "a4" | "a5" | "a3" | "letter";

/** Normalise une erreur reçue du cœur Rust. */
export function asAppError(e: unknown): AppErrorPayload {
  if (e && typeof e === "object" && "message" in e) {
    const o = e as { code?: string; message: string };
    return { code: o.code ?? "internal", message: o.message };
  }
  return { code: "internal", message: String(e) };
}

export const api = {
  openDocument: (path: string, password?: string) => invoke<DocInfo>("open_document", { path, password }),
  closeDocument: (doc: number) => invoke<void>("close_document", { doc }),
  setViewGen: (doc: number, gen: number) => invoke<void>("set_view_gen", { doc, gen }),
  searchStart: (doc: number, query: string, token: number) => invoke<void>("search_start", { doc, query, token }),
  searchCancel: (doc: number) => invoke<void>("search_cancel", { doc }),
  pageText: (doc: number, page: number) => invoke<TextWord[]>("page_text", { doc, page }),
  pageLinks: (doc: number, page: number) => invoke<PageLink[]>("page_links", { doc, page }),
  saveAttachment: (doc: number, index: number, dest: string) => invoke<void>("save_attachment", { doc, index, dest }),
  createPdf: (spec: CreateSpec, out: string) => invoke<void>("create_pdf", { spec, out }),
  recentsList: () => invoke<RecentEntry[]>("recents_list"),
  recentsTouch: (info: { path: string; pageCount: number; fileSize: number; signed: boolean; form: boolean; scanned: boolean }) =>
    invoke<number>("recents_touch", { info }),
  recentsSetPage: (path: string, page: number) => invoke<void>("recents_set_page", { path, page }),
  recentsSetFavorite: (path: string, favorite: boolean) => invoke<void>("recents_set_favorite", { path, favorite }),
  recentsSetMarks: (path: string, marks: UserMark[]) => invoke<void>("recents_set_marks", { path, marks }),
  recentsRemove: (path: string) => invoke<void>("recents_remove", { path }),
  placesList: (place: string) => invoke<FileEntry[]>("places_list", { place }),
  prefsGet: () => invoke<Record<string, unknown>>("prefs_get"),
  prefsSet: (key: string, value: unknown) => invoke<void>("prefs_set", { key, value }),
  takeStartupFiles: () => invoke<string[]>("take_startup_files"),
  userInfo: () => invoke<{ displayName: string; documentsDir: string | null }>("user_info"),
  fileExists: (path: string) => invoke<boolean>("file_exists", { path }),
};
