/**
 * Valeurs de thème utiles côté TypeScript (calculs d'animation, canvas).
 * Elles doivent rester alignées sur `tokens.css`.
 */
export const motion = {
  ease: "cubic-bezier(.2,.8,.2,1)",
  fast: 150,
  base: 200,
  mid: 300,
  slow: 450,
} as const;

export const colors = {
  accent: "#6E6AF6",
  accentText: "#A9A6FF",
  success: "#4FCB91",
  warning: "#F2B84B",
  danger: "#EF6B6B",
  paper: "#FBFAF7",
  paperDark: "#1B1C21",
} as const;

/** Couleurs proposées pour les signets de l'utilisateur. */
export const markColors = ["#F2B84B", "#6E6AF6", "#4FCB91", "#EF6B6B", "#5BC0EB"] as const;

/** Points PDF → pixels CSS à 100 % (96 ppp). */
export const PT_TO_PX = 96 / 72;
