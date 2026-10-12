/**
 * Module Sécurité (maquette Security.dc.html) : mot de passe, chiffrement,
 * permissions, métadonnées, archivage PDF/A, récapitulatif et application.
 */
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button, Chip, Icons, Segmented, Switch, TextField, ToastHost, toast } from "../../components";
import { ALL_PERMISSIONS, asAppError, type Permissions } from "../../lib/api";
import { formatSize } from "../../lib/format";
import { dropDocument } from "../../lib/render";
import { dropPageData } from "../../lib/pagedata";
import { conflict, countPerms, PERM_KEYS, securityApi, strength, STRENGTH_COLORS, type Meta, type PdfaCheck, type PdfaLevel, type SecurityInfo, type SecuritySpec } from "../../lib/security";
import { signApi } from "../../lib/sign";
import { newJob } from "../../lib/tools";
import { useActiveTab, useApp, type DocTab } from "../../stores/app";
import { useAnnots } from "../../stores/annot";
import { useEdit } from "../../stores/edit";
import { useForms } from "../../stores/forms";
import { saveTab } from "../../shell/Unsaved";
import "../tools/tools.css";
import "./security.css";

const EYE = "M1.5 8s2.5-4.5 6.5-4.5S14.5 8 14.5 8 12 12.5 8 12.5 1.5 8 1.5 8z M8 6a2 2 0 1 0 0 4 2 2 0 0 0 0-4z";
const EYE_OFF = "M2.5 2.5l11 11 M6.5 4c.5-.3 1-.5 1.5-.5 4 0 6.5 4.5 6.5 4.5s-.6 1.1-1.7 2.2 M4.2 5.2C2.5 6.4 1.5 8 1.5 8s2.5 4.5 6.5 4.5c1 0 1.9-.3 2.7-.7";
const D_OK = "M3.5 8.5l3 3 6-7";
const LEVELS: PdfaLevel[] = ["1b", "2b", "3b"];
const META_KEYS: (keyof Meta)[] = ["title", "author", "subject", "keywords"];

type Phase = { kind: "idle" } | { kind: "applying"; pct: number } | { kind: "done"; protectedDoc: boolean };

export function Security() {
  const tab = useActiveTab();
  if (!tab) return <div className="workspace" />;
  return <SecurityInner key={`${tab.key}:${tab.info.id}`} tab={tab} />;
}

function initialSpec(info: SecurityInfo): SecuritySpec {
  return {
    protect: info.openPassword,
    userPassword: "",
    ownerPassword: "",
    aes256: info.algorithm !== "aes-128",
    encryptMetadata: info.encryptMetadata,
    permissions: { ...info.permissions },
    metadata: { ...info.metadata },
    pdfa: LEVELS.includes(info.pdfa as PdfaLevel) ? (info.pdfa as PdfaLevel) : null,
  };
}

function SecurityInner({ tab }: { tab: DocTab }) {
  const { t } = useTranslation();
  const [info, setInfo] = useState<SecurityInfo | null>(null);
  const [spec, setSpec] = useState<SecuritySpec | null>(null);
  const [show, setShow] = useState(false);
  const [phase, setPhase] = useState<Phase>({ kind: "idle" });
  const [checks, setChecks] = useState<PdfaCheck[] | null>(null);
  const [checkStep, setCheckStep] = useState(-1);
  const [unlockPw, setUnlockPw] = useState("");
  const [unlockBad, setUnlockBad] = useState(false);
  const timer = useRef(0);

  const load = async () => {
    try {
      const i = await securityApi.info(tab.info.id);
      setInfo(i);
      setSpec(initialSpec(i));
    } catch (e) {
      toast.error(asAppError(e).message);
    }
  };
  useEffect(() => {
    void load();
    return () => window.clearInterval(timer.current);
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  const up = (p: Partial<SecuritySpec>) => {
    setSpec((s) => (s ? { ...s, ...p } : s));
    if (phase.kind === "done") setPhase({ kind: "idle" });
  };

  // Le contrôle PDF/A se refait quand le niveau change.
  useEffect(() => {
    setChecks(null);
    setCheckStep(-1);
    window.clearInterval(timer.current);
  }, [spec?.pdfa]);

  if (!info || !spec) return <div className="workspace" />;

  const restricted = info.algorithm !== "none" && !info.owner && countPerms(info.permissions) < 6;
  const busy = phase.kind === "applying";
  const keepPw = info.openPassword && !info.owner;
  const score = strength(spec.userPassword);
  const strengthLabels = t("security.strength", { returnObjects: true }) as string[];
  const permCount = countPerms(spec.permissions);
  const clash = conflict(spec);
  const encrypting = !spec.pdfa && (spec.protect || permCount < 6);
  const metaSet = META_KEYS.some((k) => spec.metadata[k].trim());

  const runCheck = async () => {
    if (!spec.pdfa) return;
    window.clearInterval(timer.current);
    setCheckStep(0);
    try {
      const res = await securityApi.pdfaCheck(tab.info.id, spec.pdfa);
      setChecks(res);
      // Les points se valident l'un après l'autre (maquette).
      let i = 0;
      timer.current = window.setInterval(() => {
        i += 1;
        setCheckStep(i);
        if (i >= res.length) window.clearInterval(timer.current);
      }, 380);
    } catch (e) {
      setCheckStep(-1);
      toast.error(asAppError(e).message);
    }
  };

  const unlock = async () => {
    try {
      const fresh = await securityApi.reload(tab.info.id, unlockPw);
      const i = await securityApi.info(fresh.id);
      if (!i.owner) {
        setUnlockBad(true);
        return;
      }
      dropDocument(fresh.id);
      dropPageData(fresh.id);
      useApp.getState().updateTab(tab.key, (tb) => ({ info: fresh, viewRev: (tb.viewRev ?? 0) + 1 }));
    } catch {
      setUnlockBad(true);
    }
  };

  const apply = async () => {
    if (busy) return;
    if (spec.protect && !spec.pdfa && !spec.userPassword && !keepPw) {
      toast.error(t("security.needPassword"));
      return;
    }
    // Les modifications en attente sont d'abord enregistrées : la sécurité s'applique au fichier.
    if (!(await saveTab(tab.key))) return;
    const cur = useApp.getState().tabs.find((x) => x.key === tab.key)?.info ?? tab.info;
    // PDF/A : aucune protection (le chiffrement y est interdit).
    const sent: SecuritySpec = spec.pdfa ? { ...spec, protect: false, permissions: { ...ALL_PERMISSIONS } } : spec;
    const t0 = performance.now();
    const job = newJob({
      progress: (p) => setPhase({ kind: "applying", pct: Math.round(((p.step + 1) / 4) * 90) }),
      done: async (d) => {
        if (!d.ok) {
          setPhase({ kind: "idle" });
          toast.error(d.message ?? t("security.failed"));
          return;
        }
        // Le document est rouvert : avec le mot de passe propriétaire s'il est connu
        // (l'auteur garde tous ses droits), sinon avec le mot de passe d'ouverture.
        const encrypted = !sent.pdfa && (sent.protect || countPerms(sent.permissions) < 6);
        try {
          let fresh;
          if (!encrypted) fresh = await securityApi.reload(cur.id, null);
          else if (sent.ownerPassword) fresh = await securityApi.reload(cur.id, sent.ownerPassword);
          else if (sent.protect && sent.userPassword) fresh = await securityApi.reload(cur.id, sent.userPassword);
          else if (sent.protect) fresh = await signApi.reload(cur.id);
          else fresh = await securityApi.reload(cur.id, null);
          dropDocument(fresh.id);
          dropPageData(fresh.id);
          useAnnots.getState().discard(tab.key);
          useEdit.getState().discard(tab.key);
          useForms.getState().discard(tab.key);
          useApp.getState().updateTab(tab.key, (tb) => ({ info: fresh, viewRev: (tb.viewRev ?? 0) + 1 }));
        } catch (e) {
          toast.error(asAppError(e).message);
        }
        // Une application trop rapide reste lisible.
        await new Promise((r) => setTimeout(r, Math.max(0, 700 - (performance.now() - t0))));
        setPhase({ kind: "done", protectedDoc: encrypted });
        // Le fichier a changé : le contrôle PDF/A est à refaire.
        window.clearInterval(timer.current);
        setChecks(null);
        setCheckStep(-1);
        try {
          const i = await securityApi.info(cur.id);
          setInfo(i);
          setSpec((s) => (s ? { ...initialSpec(i), userPassword: "", ownerPassword: "", aes256: s.aes256 } : s));
        } catch {
          /* état rafraîchi au prochain affichage */
        }
      },
    });
    setPhase({ kind: "applying", pct: 4 });
    try {
      await securityApi.apply(job, cur.id, sent);
    } catch (e) {
      setPhase({ kind: "idle" });
      toast.error(asAppError(e).message);
    }
  };

  const algoLabel = (a: string) => ({ none: t("security.sumEncNone"), "rc4-40": "RC4 40 bits", "rc4-128": "RC4 128 bits", "aes-128": "AES-128", "aes-256": "AES-256" })[a] ?? a;
  const summary = [
    { k: t("security.sumOpen"), v: spec.protect && !spec.pdfa ? t("security.sumOpenPw") : t("security.sumOpenFree"), dot: spec.protect && !spec.pdfa ? "var(--success)" : "#5E616A" },
    { k: t("security.sumEnc"), v: encrypting ? (spec.aes256 ? "AES-256" : "AES-128") : t("security.sumEncNone"), dot: encrypting ? "var(--success)" : "#5E616A" },
    { k: t("security.sumPerms"), v: `${spec.pdfa ? 6 : permCount} / 6`, dot: !spec.pdfa && permCount < 6 ? "var(--warning)" : "#5E616A" },
    { k: t("security.sumMeta"), v: metaSet ? t("security.sumMetaSet") : t("security.sumMetaEmpty"), dot: "#7FCDEE" },
    { k: t("security.sumPdfa"), v: spec.pdfa ? `PDF/A-${spec.pdfa}` : t("security.sumPdfaNone"), dot: spec.pdfa ? "#CDA8F7" : "#5E616A" },
  ];

  const checkDone = checks && checkStep >= checks.length;
  const bad = checks?.filter((c) => !c.ok && !c.fixable) ?? [];
  const pending = checks?.filter((c) => !c.ok && c.fixable) ?? [];
  const checkLabel = checkStep >= 0 && !checkDone ? t("security.checking") : checkDone ? (bad.length ? t("security.notConforming") : pending.length ? t("security.conformingAfter") : t("security.conforming", { level: spec.pdfa })) : t("security.checkBtn");
  const shownChecks: { id: string; label: string; state: "done" | "active" | "todo"; tone: "ok" | "fix" | "bad" | "none"; note?: string }[] = (
    checks ?? (["fonts", "colors", "metadata", "dynamic"] as const).map((id) => ({ id, ok: false, count: 0, fixable: false }))
  ).map((c, i) => ({
    id: c.id,
    label: t(`security.check.${c.id}`),
    state: checks && checkStep > i ? "done" : checkStep === i ? "active" : "todo",
    tone: !checks || checkStep <= i ? "none" : c.ok ? "ok" : c.fixable ? "fix" : "bad",
    note: !checks || checkStep <= i || c.ok ? undefined : c.fixable ? t("security.fixed") : c.id === "fonts" ? t("security.fontsMissing", { count: c.count }) : t("security.notFixable", { count: c.count }),
  }));

  return (
    <div className="workspace securityw">
      <main className="panel sec__main">
        <header className="sec__head">
          <h1>{t("security.title")}</h1>
          <span className="muted tnum">
            {tab.info.name} · {formatSize(tab.info.fileSize)} · {t("tool.pages", { count: tab.info.pageCount })}
            {info.algorithm !== "none" ? ` · ${t("security.current")} : ${algoLabel(info.algorithm)}` : ""}
          </span>
        </header>
        {restricted && (
          <div className="sec__unlock rise">
            <span className="hic hic--warn">
              <Icons.lock />
            </span>
            <span className="sec__unlock-t">
              <strong>{t("security.restricted")}</strong>
              <span className="muted">{t("security.restrictedSub")}</span>
            </span>
            <TextField
              type="password"
              value={unlockPw}
              invalid={unlockBad}
              placeholder={t("security.ownerShort")}
              aria-label={t("security.ownerPassword")}
              onChange={(e) => {
                setUnlockPw(e.target.value);
                setUnlockBad(false);
              }}
              onKeyDown={(e) => e.key === "Enter" && void unlock()}
            />
            <Button onClick={() => void unlock()} disabled={!unlockPw}>
              {t("security.unlock")}
            </Button>
            {unlockBad && <span className="sec__unlock-bad">{t("security.wrongOwner")}</span>}
          </div>
        )}
        <fieldset className="sec__grid" disabled={busy || restricted}>
          {/* Mot de passe */}
          <section className="scard">
            <div className="scard__head">
              <span className="hic" style={{ background: spec.protect && !spec.pdfa ? "rgba(79,203,145,.16)" : "rgba(255,255,255,.06)", color: spec.protect && !spec.pdfa ? "#4FCB91" : "var(--text-muted)" }}>
                <svg width="20" height="20" aria-hidden="true" style={{ overflow: "visible" }}>
                  <path className="padlock__shackle" d="M6 9.5V7a4 4 0 0 1 8 0v2.5" style={{ transform: `translateY(${spec.protect && !spec.pdfa ? 0 : -3.5}px)` }} />
                  <rect x="4" y="9.5" width="12" height="8.5" rx="2.2" fill="currentColor" opacity=".9" />
                </svg>
              </span>
              <span className="scard__title">
                <h2 className="title">{t("security.password")}</h2>
                <span className="muted">{spec.protect && !spec.pdfa ? t("security.lockOn") : t("security.lockOff")}</span>
              </span>
              <Switch label={t("security.protectLabel")} checked={spec.protect} onChange={(protect) => up({ protect })} />
            </div>
            {spec.protect && (
              <div className="rise scard__body">
                <label className="sfield">
                  <span className="muted">{t("security.userPassword")}</span>
                  <TextField
                    type={show ? "text" : "password"}
                    value={spec.userPassword}
                    onChange={(e) => up({ userPassword: e.target.value })}
                    placeholder={keepPw ? t("security.keepPlaceholder") : t("security.userPlaceholder")}
                    autoComplete="new-password"
                    suffix={
                      <button type="button" className="btn btn--ghost sfield__eye" aria-label={show ? t("security.hide") : t("security.show")} onClick={() => setShow((v) => !v)}>
                        <svg className="i" width="16" height="16" aria-hidden="true">
                          <path d={show ? EYE_OFF : EYE} />
                        </svg>
                      </button>
                    }
                  />
                </label>
                <div className="smeter" aria-hidden="true">
                  {[0, 1, 2, 3].map((i) => (
                    <span key={i} style={{ background: i < score ? STRENGTH_COLORS[score] : "rgba(255,255,255,.08)", transitionDelay: `${i * 60}ms` }} />
                  ))}
                </div>
                <span className="muted smeter__l" style={{ color: spec.userPassword ? STRENGTH_COLORS[score] : undefined }} aria-live="polite">
                  {spec.userPassword ? strengthLabels[score] : keepPw ? t("security.keepHint") : t("security.typePassword")}
                </span>
              </div>
            )}
            <label className="sfield">
              <span className="muted">{t("security.ownerPassword")}</span>
              <TextField type="password" value={spec.ownerPassword} onChange={(e) => up({ ownerPassword: e.target.value })} placeholder={t("security.ownerPlaceholder")} autoComplete="new-password" />
            </label>
            {!spec.ownerPassword && permCount < 6 && !spec.pdfa && <span className="muted sec__hint">{t("security.ownerHint")}</span>}
          </section>

          {/* Chiffrement */}
          <section className="scard" style={{ animationDelay: ".05s" }}>
            <div className="scard__head">
              <span className="hic" style={{ background: "rgba(110,106,246,.16)", color: "#A9A6FF" }}>
                <svg className="i" width="16" height="16" aria-hidden="true">
                  <path d="M5 7.5a3 3 0 1 1 3 3 M8 10.5v3.5 M8 12h2 M8 14h1.5" />
                </svg>
              </span>
              <span className="scard__title">
                <h2 className="title">{t("security.encryption")}</h2>
                <span className="muted">{encrypting ? (spec.aes256 ? t("security.aes256Hint") : t("security.aes128Hint")) : t("security.encNotUsed")}</span>
              </span>
            </div>
            <Segmented
              block
              label={t("security.algorithm")}
              value={spec.aes256 ? "256" : "128"}
              options={[
                { value: "128", label: "AES-128" },
                { value: "256", label: "AES-256" },
              ]}
              onChange={(v) => up({ aes256: v === "256" })}
            />
            <div className="tool__row">
              <span>{t("security.encryptMeta")}</span>
              <Switch label={t("security.encryptMeta")} checked={spec.encryptMetadata} onChange={(encryptMetadata) => up({ encryptMetadata })} />
            </div>
          </section>

          {/* Permissions */}
          <section className="scard" style={{ animationDelay: ".1s" }}>
            <div className="scard__head">
              <span className="hic" style={{ background: "rgba(242,184,75,.14)", color: "#F2C064" }}>
                <svg className="i" width="16" height="16" aria-hidden="true">
                  <path d="M8 1.5l5 2v4c0 3.2-2.2 5.6-5 7-2.8-1.4-5-3.8-5-7v-4z" />
                </svg>
              </span>
              <span className="scard__title">
                <h2 className="title">{t("security.permissions")}</h2>
                <span className="muted tnum">{t("security.permCount", { count: permCount })}</span>
              </span>
            </div>
            <div className="sperms">
              {PERM_KEYS.map((k) => (
                <div key={k} className={`sperm ${spec.permissions[k] ? "" : "sperm--off"}`}>
                  <span>{t(`security.perm.${k}`)}</span>
                  <Switch label={t(`security.perm.${k}`)} checked={spec.permissions[k]} onChange={(v) => up({ permissions: { ...spec.permissions, [k]: v } as Permissions })} />
                </div>
              ))}
            </div>
          </section>

          {/* Métadonnées */}
          <section className="scard" style={{ animationDelay: ".15s" }}>
            <div className="scard__head">
              <span className="hic" style={{ background: "rgba(91,192,235,.14)", color: "#7FCDEE" }}>
                <svg className="i" width="16" height="16" aria-hidden="true">
                  <path d="M2.5 4.5h7l4 3.5-4 3.5h-7z M5.5 8h.01" />
                </svg>
              </span>
              <span className="scard__title">
                <h2 className="title">{t("security.metadata")}</h2>
                <span className="muted">{t("security.metadataSub")}</span>
              </span>
              <Button variant="ghost" size="sm" onClick={() => up({ metadata: { title: "", author: "", subject: "", keywords: "" } })}>
                {t("security.clearMeta")}
              </Button>
            </div>
            <div className="smeta">
              {META_KEYS.map((k) => (
                <label key={k} className="sfield">
                  <span className="muted">{t(`security.meta.${k}`)}</span>
                  <TextField value={spec.metadata[k]} placeholder={t("security.empty")} onChange={(e) => up({ metadata: { ...spec.metadata, [k]: e.target.value } })} />
                </label>
              ))}
            </div>
          </section>

          {/* PDF/A */}
          <section className="scard" style={{ animationDelay: ".2s" }}>
            <div className="scard__head">
              <span className="hic" style={{ background: "rgba(196,155,245,.14)", color: "#CDA8F7" }}>
                <svg className="i" width="16" height="16" aria-hidden="true">
                  <path d="M2.5 3.5h11v3h-11z M3.5 6.5v7h9v-7 M6.5 9h3" />
                </svg>
              </span>
              <span className="scard__title">
                <h2 className="title">{t("security.pdfa")}</h2>
                <span className="muted">{t("security.pdfaSub")}</span>
              </span>
            </div>
            <div className="schips">
              <Chip pressed={!spec.pdfa} onClick={() => up({ pdfa: null })}>
                {t("security.pdfaNone")}
              </Chip>
              {LEVELS.map((l) => (
                <Chip key={l} pressed={spec.pdfa === l} onClick={() => up({ pdfa: l })}>
                  PDF/A-{l}
                </Chip>
              ))}
            </div>
            {clash && <span className="sec__warn rise">{t("security.pdfaConflict")}</span>}
            {spec.pdfa && (
              <>
                <div className="schecks">
                  {shownChecks.map((c) => (
                    <div key={c.id} className={`scheck scheck--${c.tone}`}>
                      {c.state === "done" && (
                        <span className={`scheck__ic popin scheck__ic--${c.tone}`}>
                          <svg className="i" width="16" height="16" aria-hidden="true">
                            <path d={c.tone === "bad" ? "M4.5 4.5l7 7 M11.5 4.5l-7 7" : D_OK} />
                          </svg>
                        </span>
                      )}
                      {c.state === "active" && <span className="scheck__spin" aria-hidden="true" />}
                      {c.state === "todo" && <span className="scheck__todo" aria-hidden="true" />}
                      <span className="scheck__l">{c.label}</span>
                      {c.note && <span className="scheck__note">{c.note}</span>}
                    </div>
                  ))}
                </div>
                <Button onClick={() => void runCheck()} disabled={checkStep >= 0 && !checkDone}>
                  {checkLabel}
                </Button>
              </>
            )}
          </section>
        </fieldset>
      </main>

      <aside className="panel sec__side" aria-label={t("security.summary")}>
        <h2 className="title">{t("security.summary")}</h2>
        <div className="ssum">
          {summary.map((s) => (
            <div key={s.k} className="ssum__row">
              <span className="ssum__dot" style={{ background: s.dot }} />
              <span className="ssum__k">{s.k}</span>
              <span className="ssum__v tnum">{s.v}</span>
            </div>
          ))}
        </div>
        <div className="sec__foot">
          {phase.kind === "applying" && (
            <div className="rise sapply">
              <span className="muted tnum">{t("security.applying", { pct: phase.pct })}</span>
              <span className="sapply__bar">
                <span style={{ width: `${phase.pct}%` }} />
              </span>
            </div>
          )}
          {phase.kind === "done" && (
            <div className="rise tool__done">
              <span className="tool__done-ic popin">
                <Icons.check />
              </span>
              <span className="tool__done-t">{phase.protectedDoc ? t("security.applied") : t("security.appliedPlain")}</span>
            </div>
          )}
          <Button variant="primary" size="lg" onClick={() => void apply()} disabled={busy || restricted}>
            {busy ? t("security.applying", { pct: phase.kind === "applying" ? phase.pct : 0 }) : phase.kind === "done" ? t("security.appliedBtn") : t("security.applyBtn")}
          </Button>
        </div>
        <ToastHost only="right" contained />
      </aside>
    </div>
  );
}
