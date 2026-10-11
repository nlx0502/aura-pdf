import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { create } from "zustand";
import { Button, Icons, Modal, ModalHeader } from "../components";
import { dirtyTabs as dirtyAnnotTabs, useAnnots } from "../stores/annot";
import { dirtyEditTabs, useEdit } from "../stores/edit";
import { useApp } from "../stores/app";

type Pending = { kind: "tab"; key: string } | { kind: "quit" } | null;
const usePending = create<{ pending: Pending }>(() => ({ pending: null }));

/** Onglets dont les annotations ou les modifications ne sont pas enregistrées. */
function dirtyTabs(): string[] {
  return Array.from(new Set([...dirtyAnnotTabs(), ...dirtyEditTabs()]));
}

function isDirty(key: string) {
  return !!useAnnots.getState().docs[key]?.dirty || !!useEdit.getState().docs[key]?.dirty;
}

/** Enregistre tout ce qui est en attente pour un onglet (l'édition enregistre aussi les annotations). */
export async function saveTab(key: string): Promise<boolean> {
  if (useEdit.getState().docs[key]?.dirty) return useEdit.getState().save(key);
  if (useAnnots.getState().docs[key]?.dirty) return useAnnots.getState().save(key);
  return true;
}

function discardTab(key: string) {
  useAnnots.getState().discard(key);
  useEdit.getState().discard(key);
}

/** Ferme un onglet, en proposant d'enregistrer les modifications si besoin. */
export function requestCloseTab(key: string) {
  if (isDirty(key)) {
    usePending.setState({ pending: { kind: "tab", key } });
    return;
  }
  discardTab(key);
  useApp.getState().closeTab(key);
}

/** Intercepte la fermeture de la fenêtre tant que des annotations ne sont pas enregistrées. */
export function useQuitGuard() {
  useEffect(() => {
    let un: (() => void) | undefined;
    let disposed = false;
    void getCurrentWindow()
      .onCloseRequested((e) => {
        if (dirtyTabs().length) {
          e.preventDefault();
          usePending.setState({ pending: { kind: "quit" } });
        }
      })
      .then((u) => (disposed ? u() : (un = u)))
      .catch(() => {});
    return () => {
      disposed = true;
      un?.();
    };
  }, []);
}

export function UnsavedDialog() {
  const { t } = useTranslation();
  const pending = usePending((s) => s.pending);
  const tabs = useApp((s) => s.tabs);
  const [busy, setBusy] = useState(false);
  const close = () => usePending.setState({ pending: null });
  if (!pending) return null;

  const keys = pending.kind === "tab" ? [pending.key] : dirtyTabs();
  const name = pending.kind === "tab" ? (tabs.find((x) => x.key === pending.key)?.info.name ?? "") : "";
  const finish = () => {
    close();
    if (pending.kind === "tab") {
      discardTab(pending.key);
      useApp.getState().closeTab(pending.key);
    } else {
      void getCurrentWindow().destroy();
    }
  };
  const saveAll = async () => {
    setBusy(true);
    let ok = true;
    for (const k of keys) ok = (await saveTab(k)) && ok;
    setBusy(false);
    if (ok) finish();
  };

  return (
    <Modal open onClose={close} label={t("annot.confirmTitle")} width={460} locked={busy}>
      <ModalHeader icon={<Icons.save />} title={t("annot.confirmTitle")} onClose={busy ? undefined : close} />
      <div className="dialog__body">
        <p className="muted" style={{ fontSize: 13.5 }}>
          {pending.kind === "tab" ? t("annot.confirmBody", { name }) : t("annot.confirmQuit")}
        </p>
      </div>
      <div className="dialog__foot">
        <Button variant="ghost" onClick={close} disabled={busy}>
          {t("common.cancel")}
        </Button>
        <Button variant="danger" onClick={finish} disabled={busy}>
          {t("annot.confirmDiscard")}
        </Button>
        <Button variant="primary" onClick={() => void saveAll()} disabled={busy} data-autofocus>
          {t("annot.confirmSave")}
        </Button>
      </div>
    </Modal>
  );
}
