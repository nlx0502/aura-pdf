import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button, Icons, IconButton, Modal, ModalHeader, Segmented, SwitchRow, TextField } from "../components";
import { useApp, type AnimPref } from "../stores/app";

/** Demande du mot de passe d'un document protégé. */
export function PasswordDialog() {
  const { t } = useTranslation();
  const prompt = useApp((s) => s.password);
  const set = useApp((s) => s.set);
  const openPath = useApp((s) => s.openPath);
  const [pw, setPw] = useState("");
  const [show, setShow] = useState(false);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    setPw("");
    setBusy(false);
  }, [prompt?.path, prompt?.error]);

  const close = () => set({ password: null });
  const submit = async () => {
    if (!prompt || !pw) return;
    setBusy(true);
    await openPath(prompt.path, pw);
    setBusy(false);
  };

  return (
    <Modal open={!!prompt} onClose={close} label={t("password.title")} width={440}>
      <ModalHeader icon={<Icons.lock />} title={t("password.title")} subtitle={prompt ? t("password.hint", { name: prompt.name }) : ""} onClose={close} />
      <form
        className="dialog__body"
        onSubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
      >
        <label className="field-label">
          <span>{t("password.label")}</span>
          <TextField
            data-autofocus
            type={show ? "text" : "password"}
            value={pw}
            invalid={!!prompt?.error}
            onChange={(e) => setPw(e.target.value)}
            autoComplete="off"
            suffix={
              <IconButton
                size="sm"
                tip={false}
                label={show ? t("password.hide") : t("password.show")}
                icon={show ? <Icons.eyeOff /> : <Icons.eye />}
                onClick={() => setShow(!show)}
              />
            }
          />
        </label>
        {prompt?.error && (
          <span className="muted rise-sm" style={{ color: "var(--warning-text)" }} role="alert">
            {prompt.error}
          </span>
        )}
        <div style={{ display: "flex", gap: 8, justifyContent: "flex-end" }}>
          <Button variant="ghost" onClick={close}>
            {t("common.cancel")}
          </Button>
          <Button variant="primary" type="submit" disabled={!pw || busy}>
            {t("password.submit")}
          </Button>
        </div>
      </form>
    </Modal>
  );
}

/** Paramètres : animations, prénom, mode sombre par défaut, à propos. */
export function SettingsDialog() {
  const { t } = useTranslation();
  const open = useApp((s) => s.settingsOpen);
  const prefs = useApp((s) => s.prefs);
  const setPref = useApp((s) => s.setPref);
  const set = useApp((s) => s.set);
  const systemName = useApp((s) => s.systemName);
  const close = () => set({ settingsOpen: false });
  return (
    <Modal open={open} onClose={close} label={t("settings.title")} width={500}>
      <ModalHeader icon={<Icons.settings />} title={t("settings.title")} onClose={close} />
      <div className="dialog__body">
        <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
          <div className="row-setting">
            <span>{t("settings.animations")}</span>
            <Segmented<AnimPref>
              label={t("settings.animations")}
              value={prefs.animations}
              onChange={(v) => setPref("animations", v)}
              options={[
                { value: "system", label: t("settings.animSystem") },
                { value: "on", label: t("settings.animOn") },
                { value: "off", label: t("settings.animOff") },
              ]}
            />
          </div>
          <span className="muted">{t("settings.animHint")}</span>
        </div>
        <label className="field-label">
          <span>{t("settings.name")}</span>
          <TextField value={prefs.userName} placeholder={systemName} onChange={(e) => setPref("userName", e.target.value)} />
        </label>
        <SwitchRow label={t("settings.darkPaper")} checked={prefs.darkPaper} onChange={(v) => setPref("darkPaper", v)} />
        <div className="row-setting">
          <span>{t("settings.language")}</span>
          <span style={{ color: "var(--text)" }}>{t("settings.french")}</span>
        </div>
        <div style={{ borderTop: "1px solid var(--border)", paddingTop: 14, display: "flex", flexDirection: "column", gap: 6 }}>
          <span className="title">{t("settings.about")}</span>
          <span className="muted">{t("settings.version", { v: __APP_VERSION__ })}</span>
          <span className="muted">{t("settings.offline")}</span>
        </div>
      </div>
    </Modal>
  );
}
