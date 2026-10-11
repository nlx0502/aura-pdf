import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button, Chip, IconButton, Icons, Kbd, Segmented, TextField } from "../../components";
import { useApp } from "../../stores/app";
import { useDrag } from "../../stores/drag";
import { api, type FileEntry, type RecentEntry } from "../../lib/api";
import { formatSize, formatWhen, readingPct } from "../../lib/format";
import { filterRecents, groupRecents, tagOf, type RecentFilter, type RecentView } from "../../lib/recents";
import { fileThumbUrl } from "../../lib/render";
import { shortcut } from "../../lib/platform";
import { pickFilesToOpen } from "../../lib/files";
import { Logo } from "../../shell/AppBar";
import { WindowControls } from "../../shell/WindowControls";
import "./home.css";

type PlaceId = "docs" | "desk" | "dl";
type CreateId = "blank" | "image" | "doc" | "scan" | "merge" | "clip";

const CREATES: { id: CreateId; icon: keyof typeof Icons; bg: string; fg: string }[] = [
  { id: "blank", icon: "plus", bg: "var(--accent-soft)", fg: "var(--accent-text)" },
  { id: "image", icon: "image", bg: "var(--pink-soft)", fg: "var(--pink-text)" },
  { id: "doc", icon: "fileText", bg: "var(--info-soft)", fg: "var(--info-text)" },
  { id: "scan", icon: "scan", bg: "var(--warning-soft)", fg: "var(--warning-text)" },
  { id: "merge", icon: "merge", bg: "var(--success-soft)", fg: "var(--success)" },
  { id: "clip", icon: "clipboard", bg: "var(--violet-soft)", fg: "var(--violet-text)" },
];

const TAG_STYLE: Record<string, { fg: string; bg: string }> = {
  signed: { fg: "#6FDCA8", bg: "rgba(79,203,145,.14)" },
  form: { fg: "#7FCDEE", bg: "rgba(91,192,235,.14)" },
  scan: { fg: "#C9CBD1", bg: "rgba(255,255,255,.08)" },
  annotated: { fg: "#F2C064", bg: "rgba(242,184,75,.14)" },
};

/** Élément affichable dans la liste (document récent ou fichier d'un emplacement). */
interface Item {
  path: string;
  name: string;
  folder: string;
  pageCount: number | null;
  fileSize: number;
  lastPage: number | null;
  date: Date;
  favorite: boolean;
  tag: ReturnType<typeof tagOf>;
  exists: boolean;
  recent: boolean;
}

function fromRecent(r: RecentEntry): Item {
  return {
    path: r.path,
    name: r.name,
    folder: r.folder,
    pageCount: r.pageCount,
    fileSize: r.fileSize,
    lastPage: r.lastPage,
    date: new Date(r.openedAt),
    favorite: r.favorite,
    tag: tagOf(r),
    exists: r.exists,
    recent: true,
  };
}

function fromFile(f: FileEntry, recents: RecentEntry[]): Item {
  const r = recents.find((x) => x.path.toLowerCase() === f.path.toLowerCase());
  return r
    ? { ...fromRecent(r), folder: f.folder }
    : {
        path: f.path,
        name: f.name,
        folder: f.folder,
        pageCount: null,
        fileSize: f.fileSize,
        lastPage: null,
        date: f.modified ? new Date(f.modified) : new Date(0),
        favorite: false,
        tag: null,
        exists: true,
        recent: false,
      };
}

export function Home() {
  const { t } = useTranslation();
  const recents = useApp((s) => s.recents);
  const openPath = useApp((s) => s.openPath);
  const openFiles = useApp((s) => s.openFiles);
  const setApp = useApp((s) => s.set);
  const setModule = useApp((s) => s.setModule);
  const refreshRecents = useApp((s) => s.refreshRecents);
  const prefs = useApp((s) => s.prefs);
  const systemName = useApp((s) => s.systemName);

  const [q, setQ] = useState("");
  const [view, setView] = useState<RecentView>("home");
  const [place, setPlace] = useState<PlaceId | null>(null);
  const [filter, setFilter] = useState<RecentFilter>("all");
  const [layout, setLayout] = useState<"list" | "grid">("list");
  const [leaving, setLeaving] = useState<string[]>([]);
  const [placeFiles, setPlaceFiles] = useState<Record<PlaceId, FileEntry[] | null>>({ docs: null, desk: null, dl: null });

  // Emplacements : liste des PDF de chaque dossier (en tâche de fond).
  useEffect(() => {
    (["docs", "desk", "dl"] as PlaceId[]).forEach((p) => {
      void api
        .placesList(p)
        .then((files) => setPlaceFiles((s) => ({ ...s, [p]: files })))
        .catch(() => setPlaceFiles((s) => ({ ...s, [p]: [] })));
    });
  }, []);

  const name = prefs.userName.trim() || systemName;
  const evening = new Date().getHours() >= 18;
  const greeting = name
    ? t(evening ? "home.greetingEvening" : "home.greetingDay", { name })
    : t(evening ? "home.greetingEveningNoName" : "home.greetingDayNoName");

  const items: Item[] = useMemo(() => {
    if (place) {
      const files = placeFiles[place] ?? [];
      const ql = q.trim().toLowerCase();
      return files
        .map((f) => fromFile(f, recents))
        .filter((i) => !ql || i.name.toLowerCase().includes(ql) || i.folder.toLowerCase().includes(ql))
        .filter((i) => filter === "all" || i.tag === (filter === "scan" ? "scan" : filter));
    }
    return filterRecents(recents, { q, view, place: null, filter }).map(fromRecent);
  }, [recents, q, view, place, filter, placeFiles]);

  const groups = useMemo(() => groupRecents(items, (i) => i.date), [items]);
  const resume = recents.find((r) => r.exists);

  const favCount = recents.filter((r) => r.favorite).length;
  const listTitle = place ? t(`home.placeNames.${place}`) : view === "fav" ? t("home.favTitle") : t("home.recentTitle");

  const toggleFav = (it: Item) => {
    if (!it.recent) return;
    void api.recentsSetFavorite(it.path, !it.favorite).then(refreshRecents);
  };
  const remove = (it: Item) => {
    setLeaving((l) => [...l, it.path]);
    window.setTimeout(() => {
      void api.recentsRemove(it.path).then(refreshRecents);
      setLeaving((l) => l.filter((p) => p !== it.path));
    }, 340);
  };

  const onCreate = (id: CreateId) => {
    if (id === "merge") setModule("fusion");
    else setApp({ create: { open: true, source: id, images: [] } });
  };

  return (
    <div className="home">
      <header className="home__bar" data-tauri-drag-region>
        <Logo width={232} />
        <div className="home__search" data-tauri-drag-region>
          <TextField
            pill
            icon={<Icons.search />}
            placeholder={t("home.searchPlaceholder")}
            aria-label={t("home.searchPlaceholder")}
            value={q}
            onChange={(e) => setQ(e.target.value)}
            style={{ maxWidth: 480 }}
          />
        </div>
        <div className="home__actions">
          <IconButton label={t("home.settings")} tipAlign="end" icon={<Icons.settings />} onClick={() => setApp({ settingsOpen: true })} />
          <span className="avatar" aria-hidden="true">
            {(name || "A").charAt(0).toUpperCase()}
          </span>
          <WindowControls />
        </div>
      </header>

      <div className="workspace">
        <aside className="panel home__side">
          <Button variant="primary" size="lg" className="home__create" icon={<Icons.plus />} onClick={() => onCreate("blank")}>
            {t("home.createPdf")}
            <span className="kbd" style={{ marginLeft: "auto" }}>
              {shortcut("N")}
            </span>
          </Button>
          {(
            [
              { id: "home", icon: "home", count: null },
              { id: "recent", icon: "clock", count: recents.length },
              { id: "fav", icon: "star", count: favCount },
            ] as const
          ).map((v) => {
            const I = Icons[v.icon];
            return (
              <button
                key={v.id}
                type="button"
                className="side"
                aria-current={view === v.id && !place}
                onClick={() => {
                  setView(v.id);
                  setPlace(null);
                }}
              >
                <I />
                {t(`home.views.${v.id}`)}
                {v.count !== null && <span className="side__cnt tnum">{v.count}</span>}
              </button>
            );
          })}
          <span className="home__sidehead">{t("home.places")}</span>
          {(["docs", "desk", "dl"] as PlaceId[]).map((p) => (
            <button
              key={p}
              type="button"
              className="side"
              aria-current={place === p}
              onClick={() => {
                setPlace(p);
                setView("recent");
              }}
            >
              <Icons.folder />
              {t(`home.placeNames.${p}`)}
              <span className="side__cnt tnum">{placeFiles[p]?.length ?? ""}</span>
            </button>
          ))}
          <div className="home__shortcuts">
            <span className="home__sidehead" style={{ padding: 0 }}>
              {t("home.shortcuts")}
            </span>
            <span className="home__sc">
              {t("home.openFile")}
              <Kbd>{shortcut("O")}</Kbd>
            </span>
            <span className="home__sc">
              {t("home.createPdf")}
              <Kbd>{shortcut("N")}</Kbd>
            </span>
          </div>
        </aside>

        <main className="panel home__main">
          <div className="home__inner">
            <div className="rise home__hello">
              <h1>{greeting}</h1>
              <span className="muted" style={{ fontSize: 14 }}>
                {t("home.subtitle")}
              </span>
            </div>

            <div className="home__hero">
              <DropZone onOpen={() => void pickFilesToOpen().then(openFiles)} />
              <ResumeCard entry={resume} onOpen={(p) => void openPath(p)} />
            </div>

            <section className="home__section">
              <div className="home__sechead">
                <h2 className="title" style={{ fontSize: 15 }}>
                  {t("home.create")}
                </h2>
                <Button variant="ghost" size="sm" onClick={() => onCreate("blank")}>
                  {t("home.allOptions")}
                </Button>
              </div>
              <div className="home__creates">
                {CREATES.map((c, i) => {
                  const I = Icons[c.icon];
                  return (
                    <button key={c.id} type="button" className="create" style={{ animationDelay: `${60 + i * 40}ms` }} onClick={() => onCreate(c.id)}>
                      <span className="create__tile" style={{ background: c.bg, color: c.fg }}>
                        <I />
                      </span>
                      <span className="create__text">
                        <span className="create__label">{t(`create.${c.id}.label`)}</span>
                        <span className="muted" style={{ fontSize: 12 }}>
                          {t(`create.${c.id}.hint`)}
                        </span>
                      </span>
                    </button>
                  );
                })}
              </div>
            </section>

            <section className="home__section">
              <div className="home__sechead home__sechead--wrap">
                <div style={{ display: "flex", alignItems: "baseline", gap: 10 }}>
                  <h2 className="title" style={{ fontSize: 15 }}>
                    {listTitle}
                  </h2>
                  <span className="muted tnum">{t("common.documents", { count: items.length })}</span>
                </div>
                <div className="home__tools">
                  {(["all", "signed", "form", "scan"] as RecentFilter[]).map((f) => (
                    <Chip key={f} pressed={filter === f} onClick={() => setFilter(f)}>
                      {t(`home.filters.${f}`)}
                    </Chip>
                  ))}
                  <Segmented
                    label={t("home.display")}
                    value={layout}
                    onChange={setLayout}
                    itemWidth={34}
                    options={[
                      { value: "list", icon: <Icons.list />, ariaLabel: t("home.list"), title: t("home.list") },
                      { value: "grid", icon: <Icons.grid />, ariaLabel: t("home.grid"), title: t("home.grid") },
                    ]}
                  />
                </div>
              </div>

              {place && placeFiles[place] === null ? (
                <div className="empty rise">{t("home.placeLoading")}</div>
              ) : items.length === 0 ? (
                <div className="empty rise">{place ? t("home.placeEmpty") : recents.length === 0 ? t("home.emptyFirst") : t("home.empty")}</div>
              ) : layout === "list" ? (
                <div className="home__groups">
                  {(() => {
                    let idx = 0;
                    return groups.map((g) => (
                      <div key={g.group} className="home__group">
                        <span className="muted rise home__grouplabel">{t(`home.groups.${g.group}`)}</span>
                        {g.items.map((it) => (
                          <RecentRow
                            key={it.path}
                            item={it}
                            delay={Math.min(idx++, 12) * 40}
                            leaving={leaving.includes(it.path)}
                            onOpen={() => void openPath(it.path)}
                            onFav={() => toggleFav(it)}
                            onRemove={() => remove(it)}
                          />
                        ))}
                      </div>
                    ));
                  })()}
                </div>
              ) : (
                <div className="home__grid">
                  {items.map((it, i) => (
                    <RecentCard key={it.path} item={it} delay={Math.min(i, 12) * 40} onOpen={() => void openPath(it.path)} onFav={() => toggleFav(it)} />
                  ))}
                </div>
              )}
            </section>
          </div>
        </main>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------- zone de dépôt

function DropZone({ onOpen }: { onOpen: () => void }) {
  const { t } = useTranslation();
  const ref = useRef<HTMLButtonElement>(null);
  const drag = useDrag();
  let over = false;
  if (drag.dragging && ref.current) {
    const r = ref.current.getBoundingClientRect();
    over = drag.x >= r.left && drag.x <= r.right && drag.y >= r.top && drag.y <= r.bottom;
  }
  return (
    <button ref={ref} type="button" className={`drop ${over ? "drop--over" : ""} ${drag.dragging ? "drop--armed" : ""}`} onClick={onOpen}>
      <span className="drop__bub">
        <Icons.folderUp size={24} strokeWidth={1.6} />
      </span>
      <span className="drop__text">
        <span className="drop__title">{over ? t("home.dropOver") : t("home.dropTitle")}</span>
        <span className="muted">{t("home.dropHint")}</span>
      </span>
      <span className="btn btn--primary" style={{ pointerEvents: "none" }}>
        {t("home.openFile")}
        <span className="kbd">{shortcut("O")}</span>
      </span>
    </button>
  );
}

// ---------------------------------------------------------------- reprendre la lecture

function ResumeCard({ entry, onOpen }: { entry?: RecentEntry; onOpen: (path: string) => void }) {
  const { t } = useTranslation();
  if (!entry) {
    return (
      <div className="resume rise" style={{ animationDelay: "60ms" }}>
        <span className="resume__page resume__page--empty" aria-hidden="true">
          <span className="ln" style={{ width: "40%", height: 3, background: "var(--accent)" }} />
          <span className="ln" style={{ width: "80%", height: 6, background: "#2A2B30" }} />
          <span className="ln" style={{ width: "100%", marginTop: 6 }} />
          <span className="ln" style={{ width: "94%" }} />
          <span className="ln" style={{ width: "70%" }} />
        </span>
        <div className="resume__text">
          <span className="resume__label">{t("home.resumeLabel")}</span>
          <span className="resume__name">{t("home.resumeEmptyTitle")}</span>
          <span className="muted">{t("home.resumeEmptyHint")}</span>
        </div>
      </div>
    );
  }
  const pct = readingPct(entry.lastPage, entry.pageCount);
  return (
    <div className="resume rise" style={{ animationDelay: "60ms" }}>
      <span className="resume__page" aria-hidden="true">
        <img src={fileThumbUrl(entry.path, 240)} alt="" draggable={false} />
      </span>
      <div className="resume__text">
        <span className="resume__label">{t("home.resumeLabel")}</span>
        <span className="resume__name">{entry.name}</span>
        <span className="muted tnum">
          {t("common.pageOf", { page: entry.lastPage + 1, total: entry.pageCount })} · {formatWhen(new Date(entry.openedAt)).toLowerCase()}
        </span>
        <span className="bar">
          <span className="bar__fill bar__fill--grow" style={{ width: `${pct}%` }} />
        </span>
        <Button className="resume__btn" onClick={() => onOpen(entry.path)}>
          {t("home.resumeBtn")}
          <Icons.arrowRight />
        </Button>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------- lignes et cartes

function Thumb({ path, exists, size }: { path: string; exists: boolean; size: number }) {
  const [failed, setFailed] = useState(false);
  if (!exists || failed)
    return (
      <span className="rthumb__lines" aria-hidden="true">
        <span className="ln" style={{ width: "45%", height: 3, background: "var(--accent)" }} />
        <span className="ln" style={{ width: "80%", height: 4, background: "#3A3B40" }} />
        <span className="ln" style={{ width: "100%", marginTop: 3 }} />
        <span className="ln" style={{ width: "90%" }} />
        <span className="ln" style={{ width: "70%" }} />
      </span>
    );
  return <img src={fileThumbUrl(path, size)} alt="" loading="lazy" draggable={false} onError={() => setFailed(true)} />;
}

function TagPill({ tag, missing }: { tag: Item["tag"]; missing?: boolean }) {
  const { t } = useTranslation();
  if (missing)
    return (
      <span className="tag" style={{ color: "var(--danger-text)", background: "var(--danger-soft)" }}>
        {t("home.missing")}
      </span>
    );
  if (!tag) return null;
  const s = TAG_STYLE[tag];
  return (
    <span className="tag" style={{ color: s.fg, background: s.bg }}>
      {t(`home.tags.${tag}`)}
    </span>
  );
}

function RecentRow({ item, delay, leaving, onOpen, onFav, onRemove }: { item: Item; delay: number; leaving: boolean; onOpen: () => void; onFav: () => void; onRemove: () => void }) {
  const { t } = useTranslation();
  const pct = item.lastPage !== null && item.pageCount ? readingPct(item.lastPage, item.pageCount) : 0;
  const done = item.lastPage !== null && item.pageCount !== null && item.lastPage + 1 >= item.pageCount;
  return (
    <div className={`rrow ${leaving ? "rrow--leaving" : ""} ${!item.exists ? "rrow--missing" : ""}`} style={{ animationDelay: `${delay}ms` }}>
      <button type="button" className="rrow__open" aria-label={t("home.openNamed", { name: item.name })} onClick={onOpen} disabled={!item.exists}>
        <span className="rthumb">
          <Thumb path={item.path} exists={item.exists} size={96} />
        </span>
        <span className="rrow__main">
          <span className="rrow__name">
            <span className="ellipsis">{item.name}</span>
            <TagPill tag={item.tag} missing={!item.exists} />
          </span>
          <span className="muted tnum ellipsis">
            {item.folder}
            {item.pageCount !== null && ` · ${t("common.pages", { count: item.pageCount })}`} · {formatSize(item.fileSize)}
          </span>
        </span>
        <span className="rrow__progress">
          {item.lastPage !== null && item.pageCount !== null && (
            <>
              <span className="muted tnum" style={{ fontSize: 11.5 }}>
                {t("common.pageOf", { page: item.lastPage + 1, total: item.pageCount })}
              </span>
              <span className="bar bar--thin">
                <span className={`bar__fill ${done ? "bar__fill--done" : ""}`} style={{ width: `${pct}%` }} />
              </span>
            </>
          )}
        </span>
        <span className="muted tnum rrow__when">{formatWhen(item.date)}</span>
      </button>
      {item.recent && (
        <>
          <button type="button" className="star" aria-pressed={item.favorite} aria-label={t("home.favorite", { name: item.name })} onClick={onFav}>
            <Icons.star />
          </button>
          <button type="button" className="star" aria-label={t("home.remove", { name: item.name })} onClick={onRemove}>
            <Icons.close size={13} />
          </button>
        </>
      )}
    </div>
  );
}

function RecentCard({ item, delay, onOpen, onFav }: { item: Item; delay: number; onOpen: () => void; onFav: () => void }) {
  const { t } = useTranslation();
  const pct = item.lastPage !== null && item.pageCount ? readingPct(item.lastPage, item.pageCount) : 0;
  const done = item.lastPage !== null && item.pageCount !== null && item.lastPage + 1 >= item.pageCount;
  return (
    <div style={{ position: "relative" }}>
      <button type="button" className="card" style={{ animationDelay: `${delay}ms` }} onClick={onOpen} disabled={!item.exists} aria-label={t("home.openNamed", { name: item.name })}>
        <span className="card__big">
          <span className="card__pg">
            <Thumb path={item.path} exists={item.exists} size={240} />
          </span>
        </span>
        <span className="card__name ellipsis">{item.name}</span>
        <span className="muted tnum" style={{ marginTop: -6 }}>
          {formatWhen(item.date)}
          {item.lastPage !== null && item.pageCount !== null && ` · ${t("home.progressShort", { page: item.lastPage + 1, total: item.pageCount })}`}
        </span>
        <span className="bar bar--thin">
          <span className={`bar__fill ${done ? "bar__fill--done" : ""}`} style={{ width: `${pct}%` }} />
        </span>
      </button>
      {item.recent && (
        <button type="button" className="star card__star" aria-pressed={item.favorite} aria-label={t("home.favorite", { name: item.name })} onClick={onFav}>
          <Icons.star />
        </button>
      )}
    </div>
  );
}
