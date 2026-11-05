import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { PageActiveContext } from "./lib/pageActive";
import PageBoundary from "./components/PageBoundary";
import clsx from "clsx";
import { openPath } from "@tauri-apps/plugin-opener";
import { listen } from "@tauri-apps/api/event";
import { useQueryClient } from "@tanstack/react-query";
import {
  api,
  runtimeLabels,
  serviceLabels,
  type InstallProgress,
  type RawLogLine,
  type Crash,
  type StatusEvent,
} from "./lib/api";
import { useAppStore } from "./lib/store";
import { useAppUpdate } from "./lib/updates";
import Overview from "./pages/Overview";
import Sites from "./pages/Sites";
import Logs from "./pages/Logs";
import Settings from "./pages/Settings";
import Services from "./pages/Services";
import Notices from "./components/Notices";
import BottomNav from "./components/BottomNav";
import Titlebar from "./components/Titlebar";
import Diagnostics from "./components/Diagnostics";
import NewProject from "./components/NewProject";
import TemplateEditor from "./components/TemplateEditor";
import FirstRunDialog from "./components/FirstRunDialog";
import ShortcutsDialog from "./components/ShortcutsDialog";
import { useHotkeys } from "./lib/shortcuts";
import { useStack } from "./lib/stack";
import type { PageId } from "./lib/store";

function useBackendEvents() {
  const queryClient = useQueryClient();
  const { pushLog, loadHistory, notify, setInstall, record } = useAppStore.getState();

  useEffect(() => {
    const unlisten = [
      listen<StatusEvent>("service-status", ({ payload }) => {
        queryClient.invalidateQueries({ queryKey: ["services"] });
        const label = serviceLabels[payload.kind];
        if (payload.state === "running") record(`${label} started`, label, "ok");
        if (payload.state === "stopped") record(`${label} stopped`, label, "muted");
        if (payload.state === "error") record(payload.message ?? `${label} failed to start`, label, "error");
        if (payload.message) notify("error", payload.message);
      }),
      listen("service-health", () => queryClient.invalidateQueries({ queryKey: ["services"] })),
      listen("tunnels-changed", () => queryClient.invalidateQueries({ queryKey: ["tunnels"] })),
      // A project's process or dev server crashed: say so with its last lines, wherever you are.
      listen<Crash>("process-crashed", ({ payload }) => {
        const [project, process] = payload.name.split("/");
        const who = process ? `${process} (${project})` : `${project}'s dev server`;
        const code = payload.exitCode != null ? ` with exit code ${payload.exitCode}` : "";
        const next = payload.gaveUp ? ". It crashed 3 times in a few minutes, so it's left stopped." : payload.restarting ? ", restarting it." : ".";
        notify("error", `${who} crashed${code}${next}`, payload.lines);
        record(`${who} crashed`, project, "error");
      }),
      // A project's dev server came up or went away: its domain moved.
      listen<string[]>("routes-changed", ({ payload }) => {
        queryClient.invalidateQueries({ queryKey: ["sites"] });
        for (const note of payload) notify("info", note);
      }),
      listen<RawLogLine>("log-line", ({ payload }) => pushLog(payload)),
      listen<InstallProgress>("install-progress", ({ payload }) => {
        setInstall(payload);
        if (payload.stage === "done" || payload.stage === "error") {
          queryClient.invalidateQueries({ queryKey: ["installed"] });
          queryClient.invalidateQueries({ queryKey: ["services"] });
        }
        if (payload.stage === "done") {
          record(`${runtimeLabels[payload.runtime]} ${payload.version} installed`, "Services", "info");
        }
      }),
    ];
    // Subscribe first, then backfill, so no line falls in between.
    Promise.all(unlisten).then(() => api.logHistory().then(loadHistory)).catch(() => {});
    return () => unlisten.forEach((p) => p.then((off) => off()));
  }, [queryClient, pushLog, loadHistory, notify, setInstall, record]);
}

function useStartupUpdateCheck() {
  useEffect(() => {
    api
      .takeStartupWarnings()
      .then((warnings) => warnings.forEach((w) => useAppStore.getState().notify("error", w)))
      .catch(() => {});
    api
      .getSettings()
      .then((s) => {
        if (s.checkUpdates) useAppUpdate.getState().checkNow();
      })
      .catch(() => {});
  }, []);
}

const pageKeys: PageId[] = ["overview", "sites", "services", "logs", "settings"];

/** Shortcuts that work on every page (see lib/shortcuts.ts for the full list). */
function useGlobalShortcuts() {
  const { navigate, notify, setShortcutsOpen, setNewProjectOpen } = useAppStore.getState();
  const stack = useStack();
  const onError = (e: unknown) => notify("error", String(e));
  useHotkeys({
    "?": () => setShortcutsOpen(true),
    ...Object.fromEntries(pageKeys.map((p, i) => [String(i + 1), () => navigate(p)])),
    "shift+s": () => stack.startStack(),
    "shift+x": () => stack.stopAll(),
    "shift+r": () => stack.running.length > 0 && stack.restartRunning(),
    n: () => setNewProjectOpen(true),
    t: () => api.openTerminal(null).catch(onError),
    w: () =>
      api
        .envPaths()
        .then((p) => openPath(p.www))
        .catch(onError),
  });
}

const pages: Record<PageId, ReactNode> = {
  overview: <Overview />,
  sites: <Sites />,
  services: <Services />,
  logs: <Logs />,
  settings: <Settings />,
};

export default function App() {
  const page = useAppStore((s) => s.page);
  useGlobalShortcuts();
  useStartupUpdateCheck();
  const scroller = useRef<HTMLDivElement>(null);
  useBackendEvents();
  // Pages stay mounted once visited, hidden when another is shown, so switching tabs keeps
  // where you were: the selected service or project, open tabs, filters, unsaved edits.
  const [visited, setVisited] = useState<PageId[]>([page]);
  useEffect(() => setVisited((v) => (v.includes(page) ? v : [...v, page])), [page]);
  // Each page's scroll position, put back when it's shown again.
  const scrolls = useRef<Partial<Record<PageId, number>>>({});
  useLayoutEffect(() => {
    scroller.current?.scrollTo({ top: scrolls.current[page] ?? 0 });
  }, [page]);

  return (
    <div className="flex h-full flex-col">
      <Titlebar />
      <div
        ref={scroller}
        onScroll={(e) => (scrolls.current[page] = e.currentTarget.scrollTop)}
        className={clsx("min-h-0 flex-1 overflow-x-hidden", page === "logs" ? "overflow-hidden" : "overflow-y-auto")}
      >
        {visited.map((id) => (
          <main
            key={id}
            hidden={id !== page}
            className={clsx(
              "motion-page mx-auto px-6 pt-8 pb-26 sm:px-10",
              id === "overview" ? "max-w-[90rem]" : id === "logs" ? "h-full min-h-0 max-w-7xl overflow-hidden pt-4 pb-20" : "max-w-7xl",
            )}
          >
            <PageActiveContext.Provider value={id === page}>
              <PageBoundary>{pages[id]}</PageBoundary>
            </PageActiveContext.Provider>
          </main>
        ))}
      </div>
      <BottomNav />
      <Notices />
      <Diagnostics />
      <NewProject />
      <TemplateEditor />
      <FirstRunDialog />
      <ShortcutsDialog />
    </div>
  );
}
