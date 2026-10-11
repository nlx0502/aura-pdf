import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import fr from "./fr";

/** Langues disponibles. Ajouter ici `en`, `es`, etc. */
export const resources = {
  fr: { translation: fr },
} as const;

void i18n.use(initReactI18next).init({
  resources,
  lng: "fr",
  fallbackLng: "fr",
  interpolation: { escapeValue: false },
  returnNull: false,
});

export default i18n;
