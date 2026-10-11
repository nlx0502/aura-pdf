import React from "react";
import ReactDOM from "react-dom/client";
import "./i18n";
import "./styles/global.css";
import "./components/components.css";
import "./shell/shell.css";
import App from "./App";
import { useEdit } from "./stores/edit";
import { useApp } from "./stores/app";

// En développement : accès à l'état pour les scripts de capture (scripts/shot.mjs).
if (import.meta.env.DEV) {
  (window as unknown as { __aura: typeof useApp }).__aura = useApp;
  (window as unknown as { __auraEdit: typeof useEdit }).__auraEdit = useEdit;
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
