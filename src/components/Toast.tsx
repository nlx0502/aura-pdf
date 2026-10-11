import { create } from "zustand";
import { Icons } from "./icons";

export type ToastKind = "success" | "info" | "error" | "loading" | "resume";

export interface ToastData {
  id: number;
  kind: ToastKind;
  message: string;
  actionLabel?: string;
  onAction?: () => void;
  /** Bouton d'action principal (accent) plutôt que fantôme. */
  primaryAction?: boolean;
  /** Durée d'affichage en ms (0 = jusqu'à fermeture). */
  duration: number;
  position: "center" | "right";
}

interface ToastStore {
  toasts: ToastData[];
  show: (t: Partial<ToastData> & { message: string }) => number;
  update: (id: number, t: Partial<ToastData>) => void;
  dismiss: (id: number) => void;
}

let nextId = 1;
const timers = new Map<number, number>();

function schedule(id: number, duration: number, dismiss: (id: number) => void) {
  window.clearTimeout(timers.get(id));
  if (duration > 0) timers.set(id, window.setTimeout(() => dismiss(id), duration));
}

/** Notifications éphémères (toasts), accessibles depuis tout le code. */
export const useToasts = create<ToastStore>((set, get) => ({
  toasts: [],
  show: (t) => {
    const id = nextId++;
    const toast: ToastData = { id, kind: "info", duration: 4000, position: "center", ...t };
    set((s) => {
      // Au plus trois notifications par zone : la plus ancienne laisse sa place.
      const same = s.toasts.filter((x) => x.position === toast.position);
      const drop = same.length >= 3 ? same[0].id : -1;
      return { toasts: [...s.toasts.filter((x) => x.id !== drop), toast] };
    });
    schedule(id, toast.duration, get().dismiss);
    return id;
  },
  update: (id, t) => {
    set((s) => ({ toasts: s.toasts.map((x) => (x.id === id ? { ...x, ...t } : x)) }));
    const cur = get().toasts.find((x) => x.id === id);
    if (cur && t.duration !== undefined) schedule(id, cur.duration, get().dismiss);
  },
  dismiss: (id) => {
    window.clearTimeout(timers.get(id));
    timers.delete(id);
    set((s) => ({ toasts: s.toasts.filter((x) => x.id !== id) }));
  },
}));

export const toast = {
  show: (t: Partial<ToastData> & { message: string }) => useToasts.getState().show(t),
  update: (id: number, t: Partial<ToastData>) => useToasts.getState().update(id, t),
  dismiss: (id: number) => useToasts.getState().dismiss(id),
  success: (message: string, extra?: Partial<ToastData>) => useToasts.getState().show({ message, kind: "success", ...extra }),
  error: (message: string, extra?: Partial<ToastData>) => useToasts.getState().show({ message, kind: "error", duration: 7000, ...extra }),
  info: (message: string, extra?: Partial<ToastData>) => useToasts.getState().show({ message, kind: "info", ...extra }),
};

function ToastIcon({ kind }: { kind: ToastKind }) {
  if (kind === "loading") return <span className="spinner" aria-hidden="true" />;
  if (kind === "success")
    return (
      <span className="toast__icon toast__icon--success">
        <Icons.check strokeWidth={2.2} />
      </span>
    );
  if (kind === "error")
    return (
      <span className="toast__icon toast__icon--error">
        <Icons.warning />
      </span>
    );
  if (kind === "resume")
    return (
      <span className="toast__icon toast__icon--success">
        <Icons.resume />
      </span>
    );
  return (
    <span className="toast__icon toast__icon--info">
      <Icons.info />
    </span>
  );
}

/**
 * Zone d'affichage des notifications. `contained` : positionnée dans son
 * parent (ex. coin du document) plutôt que dans la fenêtre.
 */
export function ToastHost({ only, contained }: { only?: "center" | "right"; contained?: boolean }) {
  const toasts = useToasts((s) => s.toasts);
  const dismiss = useToasts((s) => s.dismiss);
  const render = (position: "center" | "right") => {
    if (only && only !== position) return null;
    const list = toasts.filter((t) => t.position === position);
    if (!list.length) return null;
    return (
      <div className={`toasts toasts--${position} ${contained ? "toasts--contained" : ""}`}>
        {list.map((t) => (
          <div key={t.id} className="toast" role={t.kind === "error" ? "alert" : "status"}>
            <ToastIcon kind={t.kind} />
            <span className="toast__msg">{t.message}</span>
            {t.actionLabel && (
              <button
                type="button"
                className={`btn ${t.primaryAction ? "btn--primary" : "btn--ghost"}`}
                onClick={() => {
                  t.onAction?.();
                  dismiss(t.id);
                }}
              >
                {t.actionLabel}
              </button>
            )}
          </div>
        ))}
      </div>
    );
  };
  return (
    <>
      {render("center")}
      {render("right")}
    </>
  );
}
