import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { openPath } from "@tauri-apps/plugin-opener";
import { useQuery } from "@tanstack/react-query";
import { Copy, Download, FolderOpen, Keyboard, Minus, Moon, Play, Square, SquareTerminal, Sun, X } from "lucide-react";
import clsx from "clsx";
import { api } from "../lib/api";
import { useAppStore } from "../lib/store";
import { useAppUpdate } from "../lib/updates";
import { useStack } from "../lib/stack";
import { useEffectiveTheme, useTheme } from "../lib/theme";
import Logo from "./Logo";
import { Button } from "./ui";

/**
 * The window's title bar. On Windows and Linux the native frame is off, so this draws
 * the minimize / maximize / close buttons; on macOS the native traffic lights sit on
 * top of it (overlay title bar), so it only leaves room for them.
 */
export default function Titlebar() {
  const notify = useAppStore((s) => s.notify);
  const navigate = useAppStore((s) => s.navigate);
  const page = useAppStore((s) => s.page);
  const update = useAppUpdate((s) => s.available);
  const os = useQuery({ queryKey: ["paths"], queryFn: api.envPaths }).data?.os;
  const services = useQuery({ queryKey: ["services"], queryFn: api.listServices });
  const paths = useQuery({ queryKey: ["paths"], queryFn: api.envPaths });
  const onError = (e: unknown) => notify("error", String(e));
  const stack = useStack();
  const anyRunning = stack.running.length > 0;
  // On a fresh install there is nothing to start yet; lead to Services instead.
  const nothingInstalled = services.data !== undefined && !services.data.some((s) => s.installed);

  return (
    <header
      data-tauri-drag-region
      className={clsx(
        "flex h-14 shrink-0 items-center gap-3 border-b border-line bg-surface/80 backdrop-blur-md select-none",
        os === "macos" ? "pr-4 pl-20" : "pl-5",
      )}
    >
      <Logo animated className="pointer-events-none size-9 shrink-0" />
      <div className="pointer-events-none leading-tight">
        <div className="font-bold tracking-tight">
          Lamp<span className="text-accent-text">Rabit</span>
        </div>
        <div className="text-xs text-muted">Local dev environment</div>
      </div>
      <ThemeToggle />

      <div className="ml-auto flex items-center gap-2">
        <Button size="sm" variant="ghost" onClick={() => useAppStore.getState().setShortcutsOpen(true)} aria-label="Keyboard shortcuts" title="Keyboard shortcuts (?)">
          <Keyboard size={15} />
        </Button>
        {update && (
          <button
            onClick={() => navigate("settings")}
            className="rounded-full bg-accent-soft px-3 py-1 text-xs font-semibold text-accent-text hover:brightness-95"
          >
            Update {update.version} available
          </button>
        )}
        <Button size="sm" onClick={() => paths.data && openPath(paths.data.www).catch(onError)} disabled={!paths.data} title="Open www (W)">
          <FolderOpen size={15} /> Open www
        </Button>
        <Button size="sm" onClick={() => api.openTerminal(null).catch(onError)} aria-label="Open terminal" title="Terminal with php, composer and mariadb (T)">
          <SquareTerminal size={15} />
        </Button>
        {/* Home has its own (profile-aware) Start, Restart and Stop in its header. */}
        {page === "overview" ? null : anyRunning ? (
          <Button size="sm" onClick={stack.stopAll} disabled={stack.busy || stack.stoppable.length === 0} title="Stop all (Shift+X)">
            <Square size={12} className="fill-current" /> Stop all
          </Button>
        ) : nothingInstalled ? (
          <Button size="sm" variant="primary" onClick={() => navigate("services")}>
            <Download size={14} /> Set up your stack
          </Button>
        ) : (
          <Button size="sm" variant="primary" onClick={stack.startStack} disabled={stack.busy} title="Shift+S">
            <Play size={14} /> {stack.profile ? `Start ${stack.profile.name}` : "Start all"}
          </Button>
        )}
      </div>

      {os && os !== "macos" && <WindowControls />}
    </header>
  );
}

/** Flips between light and dark from what's on screen; Settings → Appearance still offers "System". */
function ThemeToggle() {
  const effective = useEffectiveTheme();
  const setTheme = useTheme((s) => s.setTheme);
  const next = effective === "dark" ? "light" : "dark";
  return (
    <Button size="sm" variant="ghost" onClick={() => setTheme(next)} aria-label={`Switch to ${next} theme`} title={`Switch to ${next} theme`}>
      {effective === "dark" ? <Sun size={15} /> : <Moon size={15} />}
    </Button>
  );
}

function WindowControls() {
  const win = getCurrentWindow();
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    win.isMaximized().then(setMaximized);
    const off = win.onResized(() => win.isMaximized().then(setMaximized));
    return () => {
      off.then((f) => f());
    };
  }, [win]);

  const base = "flex h-14 w-12 items-center justify-center text-muted transition-colors";
  return (
    <div className="ml-2 flex self-stretch">
      <button aria-label="Minimize" className={clsx(base, "hover:bg-surface-2 hover:text-fg")} onClick={() => win.minimize()}>
        <Minus size={16} />
      </button>
      <button
        aria-label={maximized ? "Restore" : "Maximize"}
        className={clsx(base, "hover:bg-surface-2 hover:text-fg")}
        onClick={() => win.toggleMaximize()}
      >
        {maximized ? <Copy size={13} className="-scale-x-100" /> : <Square size={13} />}
      </button>
      <button aria-label="Close" className={clsx(base, "hover:bg-[#e81123] hover:text-white")} onClick={() => win.close()}>
        <X size={17} />
      </button>
    </div>
  );
}
