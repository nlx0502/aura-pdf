import React from "react";
import ReactDOM from "react-dom/client";
import "./i18n";
import "./styles/global.css";
import "./components/components.css";
import "./shell/shell.css";
import App from "./App";
import { useEdit } from "./stores/edit";
import { useSign } from "./stores/sign";
import { useForms } from "./stores/forms";
import { useConvert } from "./stores/convert";
import { useCompressTool, useMergeTool } from "./stores/tools";
import { useApp } from "./stores/app";
import { IS_MAC } from "./lib/platform";

// Mac : barre de titre transparente, feux tricolores natifs (voir shell.css).
if (IS_MAC) document.documentElement.classList.add("is-mac");

// En développement : accès à l'état pour les scripts de capture (scripts/shot.mjs).
if (import.meta.env.DEV) {
  (window as unknown as { __aura: typeof useApp }).__aura = useApp;
  (window as unknown as { __auraEdit: typeof useEdit }).__auraEdit = useEdit;
  (window as unknown as { __auraTools: unknown }).__auraTools = { merge: useMergeTool, compress: useCompressTool };
  (window as unknown as { __auraSign: typeof useSign }).__auraSign = useSign;
  (window as unknown as { __auraForms: typeof useForms }).__auraForms = useForms;
  (window as unknown as { __auraConvert: typeof useConvert }).__auraConvert = useConvert;
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
