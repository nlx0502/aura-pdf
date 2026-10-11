/** Boîtes de dialogue de fichiers (natives Windows). */
import { open, save } from "@tauri-apps/plugin-dialog";
import i18n from "../i18n";

const OPENABLE = ["pdf", "png", "jpg", "jpeg", "gif", "bmp", "tif", "tiff", "webp", "doc", "docx"];
export const IMAGE_EXT = ["png", "jpg", "jpeg", "gif", "bmp", "tif", "tiff", "webp"];

/** Dialogue « Ouvrir » (plusieurs fichiers possibles). */
export async function pickFilesToOpen(): Promise<string[]> {
  const res = await open({
    multiple: true,
    directory: false,
    title: i18n.t("home.openFile"),
    filters: [
      { name: "Documents pris en charge", extensions: OPENABLE },
      { name: "PDF", extensions: ["pdf"] },
    ],
  });
  if (!res) return [];
  return Array.isArray(res) ? res : [res];
}

export async function pickImages(): Promise<string[]> {
  const res = await open({
    multiple: true,
    directory: false,
    title: i18n.t("createDlg.imagesFilter"),
    filters: [{ name: i18n.t("createDlg.imagesFilter"), extensions: IMAGE_EXT }],
  });
  if (!res) return [];
  return Array.isArray(res) ? res : [res];
}

/** Un ou plusieurs PDF (insertion de pages). */
export async function pickPdfs(title: string): Promise<string[]> {
  const res = await open({ multiple: true, directory: false, title, filters: [{ name: "PDF", extensions: ["pdf"] }] });
  if (!res) return [];
  return Array.isArray(res) ? res : [res];
}

/** Une image (contenu d'une page). */
export async function pickImage(title: string): Promise<string | null> {
  const res = await open({ multiple: false, directory: false, title, filters: [{ name: i18n.t("createDlg.imagesFilter"), extensions: IMAGE_EXT }] });
  return typeof res === "string" ? res : null;
}

/** Dialogue « Enregistrer sous » pour un PDF. */
export async function savePdfAs(title: string, defaultPath: string): Promise<string | null> {
  const res = await save({ title, defaultPath, filters: [{ name: "PDF", extensions: ["pdf"] }] });
  if (!res) return null;
  return /\.pdf$/i.test(res) ? res : `${res}.pdf`;
}
