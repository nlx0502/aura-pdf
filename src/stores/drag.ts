import { create } from "zustand";

/** Glisser-déposer de fichiers depuis l'Explorateur (position en pixels CSS). */
export const useDrag = create<{ dragging: boolean; x: number; y: number }>(() => ({ dragging: false, x: 0, y: 0 }));
