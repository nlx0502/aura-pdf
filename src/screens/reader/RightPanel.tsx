import { useTranslation } from "react-i18next";
import { ProgressBar } from "../../components";
import { formatPdfDate, formatSize, formatWhenShort, readingPct } from "../../lib/format";
import { fileThumbUrl } from "../../lib/render";
import { useApp, type DocTab } from "../../stores/app";
import { gotoHit } from "./search";

/** Panneau droit : résultats de recherche, documents récents, propriétés. */
export function RightPanel({ tab }: { tab: DocTab }) {
  const { t } = useTranslation();
  const recents = useApp((s) => s.recents);
  const openPath = useApp((s) => s.openPath);
  const s = tab.search;
  const searching = s.query.trim().length >= 2;
  const info = tab.info;
  const md = info.metadata;

  return (
    <aside className="panel rpanel rpanel--right slide-in" aria-label={t("reader.info")}>
      {searching && (
        <section className="rsec">
          <div className="rpanel__head">
            <h2 className="title">{t("reader.results")}</h2>
            <span className="muted tnum">{t("reader.results", { count: s.hits.length })}</span>
          </div>
          {!s.done && (
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
              <span className="muted tnum" style={{ fontSize: 11.5 }}>
                {t("reader.searching", { pct: Math.round((s.scanned / Math.max(1, info.pageCount)) * 100) })}
              </span>
              <ProgressBar thin value={(s.scanned / Math.max(1, info.pageCount)) * 100} />
            </div>
          )}
          <div className="results">
            {s.hits.slice(0, 300).map((h, i) => (
              <button
                key={i}
                type="button"
                className={`result rise-sm ${i === s.active ? "is-active" : ""}`}
                style={{ animationDelay: `${Math.min(i, 12) * 70}ms` }}
                onClick={() => gotoHit(tab.key, i)}
              >
                <span className="result__loc">{t("reader.resultLoc", { page: h.page + 1 })}</span>
                <span className="result__text">
                  {h.before}
                  <mark>{h.matched}</mark>
                  {h.after}
                </span>
              </button>
            ))}
          </div>
          {s.done && s.hits.length === 0 && (
            <p className="muted rise-sm" style={{ margin: 0 }}>
              {t("reader.noHits")}
            </p>
          )}
        </section>
      )}

      <section className="rsec" style={{ gap: 6 }}>
        <h2 className="title" style={{ marginBottom: 6 }}>
          {t("reader.recents")}
        </h2>
        {recents
          .filter((r) => r.exists)
          .slice(0, 4)
          .map((r) => {
            const isThis = r.path.toLowerCase() === info.path.toLowerCase();
            const page = isThis ? tab.page : r.lastPage;
            return (
              <button key={r.path} type="button" className="rrowbtn recent" onClick={() => void openPath(r.path)}>
                <span className="recent__thumb">
                  <img
                    src={fileThumbUrl(r.path, 96)}
                    alt=""
                    loading="lazy"
                    draggable={false}
                    onError={(e) => (e.currentTarget.style.visibility = "hidden")}
                  />
                </span>
                <span className="recent__main">
                  <span className="recent__line">
                    <span className="ellipsis">{r.name}</span>
                    <span className="muted" style={{ flex: "none" }}>
                      {isThis ? t("reader.now") : formatWhenShort(new Date(r.openedAt))}
                    </span>
                  </span>
                  <span className="recent__line" style={{ alignItems: "center" }}>
                    <span className="bar bar--thin" style={{ flex: 1 }}>
                      <span className="bar__fill" style={{ width: `${readingPct(page, r.pageCount)}%`, opacity: 0.7 }} />
                    </span>
                    <span className="muted tnum" style={{ fontSize: 11.5 }}>
                      {page + 1} / {r.pageCount}
                    </span>
                  </span>
                </span>
              </button>
            );
          })}
      </section>

      <section className="rsec" style={{ gap: 12 }}>
        <h2 className="title">{t("reader.properties")}</h2>
        <dl className="props">
          {md.title && (
            <>
              <dt>{t("reader.props.title")}</dt>
              <dd>{md.title}</dd>
            </>
          )}
          {md.author && (
            <>
              <dt>{t("reader.props.author")}</dt>
              <dd>{md.author}</dd>
            </>
          )}
          <dt>{t("reader.props.pages")}</dt>
          <dd className="tnum">{info.pageCount}</dd>
          <dt>{t("reader.props.size")}</dt>
          <dd className="tnum">{formatSize(info.fileSize)}</dd>
          <dt>{t("reader.props.version")}</dt>
          <dd>{info.version}</dd>
          <dt>{t("reader.props.attachments")}</dt>
          <dd className="tnum">{info.attachments.length}</dd>
          {formatPdfDate(md.created) && (
            <>
              <dt>{t("reader.props.created")}</dt>
              <dd>{formatPdfDate(md.created)}</dd>
            </>
          )}
          {formatPdfDate(md.modified) && (
            <>
              <dt>{t("reader.props.modified")}</dt>
              <dd>{formatPdfDate(md.modified)}</dd>
            </>
          )}
          {md.producer && (
            <>
              <dt>{t("reader.props.producer")}</dt>
              <dd className="ellipsis" title={md.producer}>
                {md.producer}
              </dd>
            </>
          )}
          <dt>{t("reader.props.protection")}</dt>
          <dd>{info.encrypted ? t("reader.props.encrypted") : t("reader.props.none")}</dd>
          {info.hasForm && (
            <>
              <dt>{t("reader.props.form")}</dt>
              <dd>{t("reader.props.yes")}</dd>
            </>
          )}
          {info.signatureCount > 0 && (
            <>
              <dt>{t("reader.props.signatures")}</dt>
              <dd className="tnum">{info.signatureCount}</dd>
            </>
          )}
        </dl>
      </section>
    </aside>
  );
}
