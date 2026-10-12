import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { save } from "@tauri-apps/plugin-dialog";
import { Button, Chip, Icons, Modal, ModalHeader, ProgressBar, Segmented, Stepper, SwitchRow, TextField, toast } from "../../components";
import { useApp } from "../../stores/app";
import { api, asAppError, type PaperFormat } from "../../lib/api";
import { pickImages } from "../../lib/files";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { convertApi, runJob, type ConvertInfo } from "../../lib/convert";
import { localImageUrl } from "../../lib/render";
import "./create.css";

type Source = "blank" | "image" | "doc" | "scan" | "clip";
type Style = "blank" | "lines" | "grid" | "dots";

const SOURCES: { id: Source; icon: keyof typeof Icons }[] = [
  { id: "blank", icon: "plus" },
  { id: "image", icon: "image" },
  { id: "doc", icon: "fileText" },
  { id: "scan", icon: "scan" },
  { id: "clip", icon: "clipboard" },
];

const FORMATS: { id: PaperFormat; label: string; r: number }[] = [
  { id: "a4", label: "A4", r: 1.414 },
  { id: "a5", label: "A5", r: 1.414 },
  { id: "letter", label: "letter", r: 1.294 },
];

const PATTERNS: Record<Style, [string, string]> = {
  blank: ["none", "auto"],
  lines: ["linear-gradient(transparent 92%, #CFD8EA 92%)", "100% 14px"],
  grid: ["linear-gradient(#E1E0DA 1px, transparent 1px), linear-gradient(90deg, #E1E0DA 1px, transparent 1px)", "12px 12px"],
  dots: ["radial-gradient(#C9C7BE 1.2px, transparent 1.4px)", "12px 12px"],
};

function ensurePdf(name: string) {
  const n = name.trim() || "Document";
  return /\.pdf$/i.test(n) ? n : `${n}.pdf`;
}

const OFFICE_EXT = ["docx", "doc", "rtf", "odt", "xlsx", "xls", "ods", "csv", "pptx", "ppt", "odp"];

const baseName = (p: string) => p.split(/[\\/]/).pop() ?? p;

/** Fenêtre « Créer un PDF » : vierge, images, document Office, scanner, presse-papiers. */
export function CreateDialog() {
  const { t } = useTranslation();
  const create = useApp((s) => s.create);
  const setApp = useApp((s) => s.set);
  const openPath = useApp((s) => s.openPath);

  const [source, setSource] = useState<Source>(create.source);
  const [name, setName] = useState(t("createDlg.defaultName"));
  const [style, setStyle] = useState<Style>("lines");
  const [pages, setPages] = useState(1);
  const [format, setFormat] = useState<PaperFormat>("a4");
  const [land, setLand] = useState(false);
  const [images, setImages] = useState<string[]>([]);
  const [onePer, setOnePer] = useState(true);
  const [building, setBuilding] = useState(false);
  const [pct, setPct] = useState(0);
  const [built, setBuilt] = useState<string | null>(null);
  const [docPath, setDocPath] = useState<string | null>(null);
  const [scans, setScans] = useState<string[]>([]);
  const [scanning, setScanning] = useState(false);
  const [ocr, setOcr] = useState(true);
  const [office, setOffice] = useState<ConvertInfo | null>(null);
  const timer = useRef<number>(0);

  // Réinitialiser à chaque ouverture.
  useEffect(() => {
    if (!create.open) return;
    setSource(create.source);
    setImages(create.images);
    setName(create.source === "image" ? t("createDlg.imagesName") : t("createDlg.defaultName"));
    setBuilt(null);
    setBuilding(false);
    setPct(0);
    setDocPath(null);
    setScans([]);
    void convertApi.info().then(setOffice).catch(() => {});
  }, [create.open, create.source, create.images, t]);

  useEffect(() => () => window.clearInterval(timer.current), []);

  const close = () => {
    if (building) return;
    setApp({ create: { ...create, open: false } });
  };

  const pick = (s: Source) => {
    setSource(s);
    setBuilt(null);
    if (s === "image" && name === t("createDlg.defaultName")) setName(t("createDlg.imagesName"));
    if (s !== "image" && name === t("createDlg.imagesName")) setName(t("createDlg.defaultName"));
  };

  const cantBuild =
    building || !!built || !name.trim() || (source === "image" && images.length === 0) || (source === "doc" && !docPath) || (source === "scan" && scans.length === 0);

  const pickDoc = async () => {
    const p = await openDialog({ multiple: false, directory: false, title: t("createDlg.pickDoc"), filters: [{ name: t("createDlg.officeFilter"), extensions: OFFICE_EXT }] });
    if (typeof p !== "string") return;
    setDocPath(p);
    setName(`${baseName(p).replace(/\.[^.]+$/, "")}.pdf`);
  };

  const scan = async () => {
    setScanning(true);
    try {
      const p = await convertApi.scan();
      if (p) setScans((cur) => [...cur, p]);
    } catch (e) {
      toast.error(asAppError(e).message);
    } finally {
      setScanning(false);
    }
  };

  const build = async () => {
    const user = await api.userInfo().catch(() => ({ documentsDir: null }));
    const fileName = ensurePdf(name);
    const out = await save({
      defaultPath: user.documentsDir ? `${user.documentsDir}\\${fileName}` : fileName,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (!out) return;
    setBuilding(true);
    setPct(0);
    window.clearInterval(timer.current);
    // Progression visuelle pendant la génération (le cœur ne publie pas d'étapes pour une création si courte).
    timer.current = window.setInterval(() => setPct((p) => Math.min(92, p + 7)), 70);
    try {
      const langs = ["fra", "eng"];
      if (source === "blank") {
        await api.createPdf({ kind: "blank", format, landscape: land, pages, style }, out);
      } else if (source === "image") {
        const spec = { kind: "images" as const, paths: images, format, landscape: land, onePerPage: onePer };
        if (ocr) await runJob((job) => convertApi.createOcr(job, spec, out, langs));
        else await api.createPdf(spec, out);
      } else if (source === "scan") {
        const spec = { kind: "scans" as const, paths: scans };
        if (ocr) await runJob((job) => convertApi.createOcr(job, spec, out, langs));
        else await api.createPdf(spec, out);
      } else if (source === "doc" && docPath) {
        await runJob((job) => convertApi.document(job, docPath, out));
      } else if (source === "clip") {
        await convertApi.clipboard(out);
      }
      window.clearInterval(timer.current);
      setPct(100);
      setBuilt(out);
    } catch (e) {
      window.clearInterval(timer.current);
      toast.error(asAppError(e).message);
    } finally {
      setBuilding(false);
    }
  };

  const fmt = FORMATS.find((f) => f.id === format)!;
  const long = 300;
  const short = Math.round(long / fmt.r);
  const pat = PATTERNS[source === "blank" ? style : "blank"];
  const shownImages = source === "scan" ? scans.slice(-1) : onePer ? images.slice(0, 1) : images.slice(0, 4);
  const pageCountLabel =
    source === "blank"
      ? `1 / ${pages}`
      : source === "image"
        ? `1 / ${onePer ? Math.max(1, images.length) : Math.max(1, Math.ceil(images.length / 4))}`
        : source === "scan"
          ? `${Math.max(1, scans.length)} / ${Math.max(1, scans.length)}`
          : "1 / 1";

  return (
    <Modal open={create.open} onClose={close} label={t("createDlg.title")} width={900} locked={building}>
      <ModalHeader title={t("createDlg.title")} subtitle={t(`createDlg.hints.${source}`)} onClose={close} />
      <div className="cdlg">
        <div className="cdlg__sources" role="radiogroup" aria-label={t("createDlg.title")}>
          {SOURCES.map((s) => {
            const I = Icons[s.icon];
            return (
              <button key={s.id} type="button" className="src" role="radio" aria-checked={source === s.id} aria-pressed={source === s.id} onClick={() => pick(s.id)}>
                <span className="src__tile">
                  <I />
                </span>
                {t(`createDlg.sources.${s.id}`)}
              </button>
            );
          })}
        </div>

        <div className="cdlg__preview">
          <div
            className="sheet"
            style={{
              width: land ? long : short,
              height: land ? short : long,
              backgroundImage: pat[0],
              backgroundSize: pat[1],
            }}
          >
            {(source === "image" || (source === "scan" && scans.length > 0)) && (
              <div className="sheet__imgs" style={{ gridTemplateColumns: `repeat(${shownImages.length > 1 ? 2 : 1}, minmax(0, 1fr))` }}>
                {shownImages.map((p, i) => (
                  <img key={p} src={localImageUrl(p)} alt="" className="sheet__img" style={{ animationDelay: `${i * 60}ms` }} />
                ))}
                {!shownImages.length && <span className="sheet__none">{t("createDlg.noImages")}</span>}
              </div>
            )}
            {(source === "doc" || (source === "scan" && scans.length === 0) || source === "clip") && (
              <div className={`sheet__mock ${source === "scan" ? "sheet__mock--scan" : ""}`}>
                <span className="ln" style={{ width: "40%", height: 3, background: source === "doc" ? "#2563C9" : "#57554F" }} />
                <span className="ln" style={{ width: "76%", height: 7, background: "#2A2B30" }} />
                <span className="ln" style={{ width: "100%", marginTop: 8, height: 3 }} />
                <span className="ln" style={{ width: "92%", height: 3 }} />
                <span className="ln" style={{ width: "96%", height: 3 }} />
                <span className="ln" style={{ width: "60%", height: 3 }} />
              </div>
            )}
            <span className="sheet__count tnum">{pageCountLabel}</span>
          </div>
        </div>

        <div className="cdlg__settings">
          <label className="field-label">
            <span>{t("createDlg.fileName")}</span>
            <TextField data-autofocus value={name} onChange={(e) => setName(e.target.value)} />
          </label>

          {source === "blank" && (
            <div className="cdlg__group rise-sm">
              <div className="cdlg__field">
                <span className="muted">{t("createDlg.background")}</span>
                <div className="cdlg__chips">
                  {(["blank", "lines", "grid", "dots"] as Style[]).map((s) => (
                    <Chip key={s} dialog pressed={style === s} onClick={() => setStyle(s)}>
                      {t(`createDlg.styles.${s}`)}
                    </Chip>
                  ))}
                </div>
              </div>
              <div className="row-setting">
                <span>{t("createDlg.pageCount")}</span>
                <Stepper value={pages} min={1} max={500} onChange={setPages} decLabel={t("createDlg.fewer")} incLabel={t("createDlg.more")} />
              </div>
            </div>
          )}

          {source === "image" && (
            <div className="cdlg__group rise-sm">
              <div style={{ display: "flex", gap: 8 }}>
                <Button size="lg" icon={<Icons.plus />} style={{ flex: 1 }} onClick={() => void pickImages().then((p) => setImages((cur) => [...cur, ...p.filter((x) => !cur.includes(x))]))}>
                  {t("createDlg.addImages", { count: images.length })}
                </Button>
                {images.length > 0 && (
                  <Button size="lg" variant="ghost" onClick={() => setImages([])}>
                    {t("createDlg.clearImages")}
                  </Button>
                )}
              </div>
              {images.length > 0 && (
                <ul className="cdlg__files">
                  {images.map((p) => (
                    <li key={p} className="ellipsis">
                      {p.split(/[\\/]/).pop()}
                    </li>
                  ))}
                </ul>
              )}
              <SwitchRow label={t("createDlg.onePerPage")} checked={onePer} onChange={setOnePer} />
              <SwitchRow label={t("createDlg.ocr")} checked={ocr} onChange={setOcr} />
            </div>
          )}

          {source === "scan" && (
            <div className="cdlg__group rise-sm">
              <div style={{ display: "flex", gap: 8 }}>
                <Button size="lg" icon={<Icons.scan />} style={{ flex: 1 }} onClick={() => void scan()} disabled={scanning || building}>
                  {scanning ? t("createDlg.scanning") : scans.length ? t("createDlg.scanMore", { count: scans.length }) : t("createDlg.scanFirst")}
                </Button>
                {scans.length > 0 && (
                  <Button size="lg" variant="ghost" onClick={() => setScans([])}>
                    {t("createDlg.clearImages")}
                  </Button>
                )}
              </div>
              <SwitchRow label={t("createDlg.ocr")} checked={ocr} onChange={setOcr} />
            </div>
          )}

          {source === "doc" && (
            <div className="cdlg__group rise-sm">
              <Button size="lg" icon={<Icons.fileText />} onClick={() => void pickDoc()}>
                {docPath ? baseName(docPath) : t("createDlg.pickDoc")}
              </Button>
              <p className="cdlg__later">
                <Icons.info />
                {office?.word || office?.excel || office?.powerpoint ? t("createDlg.officeOn") : t("createDlg.officeOff")}
              </p>
            </div>
          )}

          {source === "clip" && (
            <p className="cdlg__later rise-sm">
              <Icons.info />
              {t("createDlg.clipHint")}
            </p>
          )}

          {source !== "doc" && source !== "clip" && source !== "scan" && (
            <div className="cdlg__field">
              <span className="muted">{t("createDlg.format")}</span>
              <div className="cdlg__chips" style={{ alignItems: "center" }}>
                {FORMATS.map((f) => (
                  <Chip key={f.id} dialog pressed={format === f.id} onClick={() => setFormat(f.id)}>
                    {f.id === "letter" ? t("createDlg.letter") : f.label}
                  </Chip>
                ))}
                <span style={{ flex: 1 }} />
                <Segmented
                  label={t("createDlg.orientation")}
                  value={land ? "land" : "port"}
                  onChange={(v) => setLand(v === "land")}
                  itemWidth={40}
                  options={[
                    { value: "port", ariaLabel: t("createDlg.portrait"), title: t("createDlg.portrait"), icon: <span className="orient orient--p" /> },
                    { value: "land", ariaLabel: t("createDlg.landscape"), title: t("createDlg.landscape"), icon: <span className="orient orient--l" /> },
                  ]}
                />
              </div>
            </div>
          )}

          <div className="cdlg__foot">
            {building && (
              <div className="rise-sm" style={{ display: "flex", flexDirection: "column", gap: 7 }}>
                <span className="muted tnum">{t("createDlg.building", { pct })}</span>
                <ProgressBar value={pct} />
              </div>
            )}
            {built && (
              <div className="cdlg__built rise-sm">
                <span className="cdlg__ok">
                  <Icons.check strokeWidth={2.2} />
                </span>
                <span style={{ flex: 1, fontSize: 13 }}>{t("createDlg.built", { name: ensurePdf(name) })}</span>
                <Button
                  variant="primary"
                  size="sm"
                  onClick={() => {
                    setApp({ create: { ...create, open: false } });
                    void openPath(built);
                  }}
                >
                  {t("common.open")}
                </Button>
              </div>
            )}
            <div style={{ display: "flex", gap: 8, justifyContent: "flex-end" }}>
              <Button variant="ghost" onClick={close} disabled={building}>
                {t("common.cancel")}
              </Button>
              <Button variant="primary" onClick={() => void build()} disabled={cantBuild}>
                {t("createDlg.submit")}
              </Button>
            </div>
          </div>
        </div>
      </div>
    </Modal>
  );
}
