import { create } from "zustand";
import type { InstallProgress, LogHistory, LogService, RawLogLine } from "./api";
import { parseLog, type LogEntry } from "./logparse";

// The Logs page draws only the rows in view, so it can keep this many.
const MAX_LOG_LINES = 10000;
const MAX_ACTIVITY = 100;

export type PageId = "overview" | "sites" | "services" | "logs" | "settings";

export interface Notice {
  id: number;
  tone: "error" | "info";
  text: string;
  /** Output lines shown under the text (a crash's last lines). */
  detail?: string[];
}

export interface Activity {
  id: number;
  at: Date;
  text: string;
  source: string;
  tone: "ok" | "info" | "muted" | "error";
}

export const installKey = (runtime: string, version: string) => `${runtime}@${version}`;

interface AppStore {
  page: PageId;
  navigate: (page: PageId) => void;
  /** Service the Logs page opens filtered to, set by "View logs" on a service. */
  /** A service, or "project:<name>". */
  logFilter: string | null;
  /** Opens Logs on one source: a service, or "project:<name>". */
  showLogs: (source: LogService | `project:${string}`) => void;
  diagnosticsOpen: boolean;
  setDiagnosticsOpen: (open: boolean) => void;
  newProjectOpen: boolean;
  setNewProjectOpen: (open: boolean) => void;
  /** The template editor: open on some YAML (a project's), and whether New project opened it. */
  templateEditor: { yaml?: string; fromWizard?: boolean } | null;
  openTemplateEditor: (editor: { yaml?: string; fromWizard?: boolean }) => void;
  closeTemplateEditor: () => void;
  /** The keyboard shortcuts help ("?"). */
  shortcutsOpen: boolean;
  setShortcutsOpen: (open: boolean) => void;
  /** How connection details are shown and copied: a URL or .env lines. */
  connMode: "url" | "env";
  setConnMode: (mode: "url" | "env") => void;
  /** The stack profile Start acts on; null means every installed service. */
  activeProfile: string | null;
  setActiveProfile: (name: string | null) => void;
  logs: LogEntry[];
  notices: Notice[];
  activity: Activity[];
  /** In-flight installs keyed by installKey(). */
  installs: Record<string, InstallProgress>;
  setInstall: (p: InstallProgress) => void;
  pushLog: (line: RawLogLine) => void;
  /** Replaces the view with history read from the log files on disk. */
  loadHistory: (history: LogHistory) => void;
  clearLogs: () => void;
  record: (text: string, source: string, tone: Activity["tone"]) => void;
  notify: (tone: Notice["tone"], text: string, detail?: string[]) => void;
  dismiss: (id: number) => void;
}

// View preferences remembered between launches. Storage can be unavailable; a choice
// then lasts for this session only.
export function readPref(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

export function writePref(key: string, value: string | null) {
  try {
    if (value === null) localStorage.removeItem(key);
    else localStorage.setItem(key, value);
  } catch {
    // see readPref
  }
}

let nextId = 1;
/** Live lines at or below this id are already part of the loaded history. */
let logCursor = 0;

export const useAppStore = create<AppStore>((set) => ({
  page: "overview",
  navigate: (page) => set({ page }),
  logFilter: null,
  showLogs: (logFilter) => set({ page: "logs", logFilter }),
  diagnosticsOpen: false,
  setDiagnosticsOpen: (diagnosticsOpen) => set({ diagnosticsOpen }),
  newProjectOpen: false,
  setNewProjectOpen: (newProjectOpen) => set({ newProjectOpen }),
  templateEditor: null,
  openTemplateEditor: (templateEditor) => set({ templateEditor }),
  closeTemplateEditor: () => set({ templateEditor: null }),
  shortcutsOpen: false,
  setShortcutsOpen: (shortcutsOpen) => set({ shortcutsOpen }),
  connMode: readPref("connMode") === "env" ? "env" : "url",
  setConnMode: (connMode) => {
    writePref("connMode", connMode);
    set({ connMode });
  },
  activeProfile: readPref("activeProfile"),
  setActiveProfile: (activeProfile) => {
    writePref("activeProfile", activeProfile);
    set({ activeProfile });
  },
  logs: [],
  notices: [],
  activity: [{ id: nextId++, at: new Date(), text: "Environment ready", source: "System", tone: "info" }],
  installs: {},
  setInstall: (p) =>
    set((s) => {
      const installs = { ...s.installs };
      const key = installKey(p.runtime, p.version);
      if (p.stage === "done" || p.stage === "error") delete installs[key];
      else installs[key] = p;
      return { installs };
    }),
  pushLog: (line) =>
    set((s) => (line.id <= logCursor ? s : { logs: [...s.logs.slice(-(MAX_LOG_LINES - 1)), parseLog(line)] })),
  loadHistory: ({ lines, cursor }) =>
    set((s) => {
      logCursor = cursor;
      // History ids are 0; give them unique negative ids so they never collide with live ones.
      // Lines without their own timestamp (stack traces, MariaDB's "Version:" line) take the
      // time of the line before them in the same file.
      const lastTime: Record<string, Date> = {};
      const history = lines.map((l, i) => {
        const entry = parseLog({ ...l, id: -(i + 1) });
        const file = `${l.service}/${l.stream}`;
        if (entry.timed) lastTime[file] = entry.time;
        else if (lastTime[file]) entry.time = lastTime[file];
        return entry;
      });
      const live = s.logs.filter((l) => l.id > cursor);
      const merged = [...history, ...live].sort((a, b) => a.time.getTime() - b.time.getTime());
      return { logs: merged.slice(-MAX_LOG_LINES) };
    }),
  clearLogs: () => set({ logs: [] }),
  record: (text, source, tone) =>
    set((s) => ({
      activity: [{ id: nextId++, at: new Date(), text, source, tone }, ...s.activity].slice(0, MAX_ACTIVITY),
    })),
  notify: (tone, text, detail) => {
    const id = nextId++;
    set((s) => ({ notices: [...s.notices, { id, tone, text, detail }] }));
    // Ones with output stay longer, to be read.
    setTimeout(() => set((s) => ({ notices: s.notices.filter((n) => n.id !== id) })), detail?.length ? 15000 : 6000);
  },
  dismiss: (id) => set((s) => ({ notices: s.notices.filter((n) => n.id !== id) })),
}));
