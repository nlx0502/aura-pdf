/** Détails propres au système (libellés de raccourcis, etc.). */

export const IS_MAC = typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);

/** Préfixe des raccourcis : « Ctrl+ » sous Windows, « ⌘ » sous macOS. */
export const MOD = IS_MAC ? "⌘" : "Ctrl+";

/** Libellé d'un raccourci avec la touche de commande : `shortcut("P")` → « Ctrl+P ». */
export function shortcut(key: string): string {
  return `${MOD}${key}`;
}

/** Vrai si l'événement clavier utilise la touche de commande du système. */
export function isMod(e: KeyboardEvent | React.KeyboardEvent): boolean {
  return IS_MAC ? e.metaKey : e.ctrlKey;
}

/** Vrai si la cible est un champ de saisie (les raccourcis à une lettre sont alors ignorés). */
export function isTyping(e: KeyboardEvent): boolean {
  const t = e.target as HTMLElement | null;
  if (!t) return false;
  return t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.tagName === "SELECT" || t.isContentEditable;
}
