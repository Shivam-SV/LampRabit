import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ChevronRight, Infinity as Forever, Play, Plus, RefreshCw, RotateCcw, ScrollText, Search, Square, Trash2, X } from "lucide-react";
import clsx from "clsx";
import { api, type AppInfo, type AppOutput, type AppStatus, type NodeApp, type PackageManager, type ProjectCommand, type Site } from "../lib/api";
import { useAppStore } from "../lib/store";
import { Button, Segmented, inputClass } from "./ui";

/** How each package manager runs a script, and passes it extra arguments. */
const runPrefix: Record<PackageManager, { run: string; args: string }> = {
  npm: { run: "npm run", args: " --" },
  pnpm: { run: "pnpm run", args: "" },
  yarn: { run: "yarn run", args: "" },
  bun: { run: "bun run", args: "" },
};

export const stateStyle: Record<AppStatus["state"], { label: string; dot: string; text: string }> = {
  stopped: { label: "Stopped", dot: "bg-muted/50", text: "text-muted" },
  starting: { label: "Starting", dot: "animate-pulse bg-warn", text: "text-fg" },
  running: { label: "Running", dot: "bg-ok", text: "text-accent-text" },
  crashed: { label: "Crashed", dot: "bg-danger", text: "text-danger-text" },
};

/** Every project's process status, kept current from the backend's events. */
export function useAppStatuses() {
  const initial = useQuery({ queryKey: ["app-statuses"], queryFn: api.appStatuses });
  const [live, setLive] = useState<Record<string, AppStatus>>({});
  useEffect(() => {
    const off = listen<AppStatus>("app-status", ({ payload }) => setLive((cur) => ({ ...cur, [payload.name]: payload })));
    return () => {
      off.then((f) => f());
    };
  }, []);
  return useMemo(() => {
    const out: Record<string, AppStatus> = {};
    for (const s of initial.data ?? []) out[s.name] = s;
    return { ...out, ...live };
  }, [initial.data, live]);
}

export const isActive = (s?: AppStatus) => s?.state === "starting" || s?.state === "running";

/** How a project with a package.json runs: its saved setup or what package.json suggests. */
export function useAppInfo(site: Site) {
  return useQuery({ queryKey: ["app-info", site.name], queryFn: () => api.appInfo(site.name), enabled: site.nodeProject });
}

/** The command line a project's process runs, as shown to people. */
export function appCommand(config: NodeApp) {
  return config.mode === "file" ? `node ${config.watch ? "--watch " : ""}${config.file}` : config.command.trim();
}

/** Start, stop and restart a project's process. The backend picks the port, writes the site and reloads the web server. */
export function useAppActions(name: string) {
  const queryClient = useQueryClient();
  const notify = useAppStore((s) => s.notify);
  const onError = (e: unknown) => notify("error", String(e));
  const onSuccess = () => {
    queryClient.invalidateQueries({ queryKey: ["app-info", name] });
    queryClient.invalidateQueries({ queryKey: ["sites"] });
  };
  return {
    start: useMutation({ mutationFn: () => api.appStart(name), onSuccess, onError }),
    stop: useMutation({ mutationFn: () => api.appStop(name), onError }),
    restart: useMutation({ mutationFn: () => api.appRestart(name), onSuccess, onError }),
  };
}

/** The status pill: Running · :3001. */
export function AppState({ status, className }: { status?: AppStatus; className?: string }) {
  const style = stateStyle[status?.state ?? "stopped"];
  return (
    <span className={clsx("inline-flex items-center gap-1.5 rounded-full bg-surface-2 px-2.5 py-1 text-xs font-medium", style.text, className)}>
      <span className={clsx("size-1.5 rounded-full", style.dot)} aria-hidden />
      {style.label}
      {isActive(status) && status?.port ? <span className="font-mono text-muted">:{status.port}</span> : null}
      {status?.state === "crashed" && status.exitCode != null && <span>· exit {status.exitCode}</span>}
    </span>
  );
}

/** Start / Stop / Restart buttons. */
export function AppControls({ name, status, label = "Start", size = "md" }: { name: string; status?: AppStatus; label?: string; size?: "sm" | "md" }) {
  const { start, stop, restart } = useAppActions(name);
  if (isActive(status)) {
    return (
      <span className="flex items-center gap-1.5">
        <Button size={size} onClick={() => restart.mutate()} disabled={restart.isPending} aria-label="Restart" title="Restart">
          <RotateCcw size={14} />
        </Button>
        <Button size={size} onClick={() => stop.mutate()} disabled={stop.isPending}>
          <Square size={12} className="fill-current" /> Stop
        </Button>
      </span>
    );
  }
  return (
    <Button size={size} variant="primary" onClick={() => start.mutate()} disabled={start.isPending}>
      <Play size={14} /> {start.isPending ? "Starting…" : label}
    </Button>
  );
}

/**
 * How the domain is served, for the Settings tab: dynamic (through the project's dev
 * server, on a port picked when it starts) or static (the files). The start command
 * comes from the template or package.json; changing it is optional.
 */
export function ServeSettings({ site, info, status }: { site: Site; info: AppInfo; status?: AppStatus }) {
  const queryClient = useQueryClient();
  const notify = useAppStore((s) => s.notify);
  const [draft, setDraft] = useState<NodeApp>(info.config);
  const [customising, setCustomising] = useState(false);
  useEffect(() => setDraft(info.config), [info.config]);
  const set = (patch: Partial<NodeApp>) => setDraft((d) => ({ ...d, ...patch }));
  const dirty = JSON.stringify(draft) !== JSON.stringify(info.config);
  const pm = draft.packageManager ?? info.detectedPackageManager;
  const node = site.language === "node";
  const save = useMutation({
    mutationFn: (config: NodeApp) => api.appSave(site.name, config),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["app-info", site.name] });
      queryClient.invalidateQueries({ queryKey: ["sites"] });
      notify("info", isActive(status) ? "Saved. Restart the app to use it." : "Saved.");
      setCustomising(false);
    },
    onError: (e) => notify("error", String(e)),
  });

  return (
    <div className="space-y-4 text-sm">
      {node && (
        <Row title="Served by" hint={draft.serve ? "Its dev server, behind the web server, with hot reload." : "The web server, from the build folder (dist, build, out) or the project folder."}>
          <Segmented
            value={draft.serve ? "dynamic" : "static"}
            onChange={(v) => set({ serve: v === "dynamic" })}
            options={[
              { value: "dynamic", label: "Dev server" },
              { value: "static", label: "Files" },
            ]}
            className="w-52"
          />
        </Row>
      )}

      {(draft.serve || !node) && (
        <>
          <Row title={node ? "Start command" : "Dev server"} hint={node ? "From the project's template or package.json." : "Runs next to the site, e.g. Vite for assets; the site itself stays on PHP."}>
            {customising ? null : (
              <span className="flex min-w-0 items-center gap-2">
                <code className="truncate rounded-md bg-surface-2 px-2 py-1 font-mono text-xs">{appCommand(draft) || "none"}</code>
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => {
                    // An old "node file" setup becomes the command it ran, e.g. node --watch server.js.
                    if (draft.mode === "file") set({ mode: "command", command: appCommand(draft) });
                    setCustomising(true);
                  }}
                >
                  Change
                </Button>
              </span>
            )}
          </Row>
          {customising && (
            <div className="space-y-3 rounded-xl border border-line p-3">
              <input className={clsx(inputClass, "font-mono")} value={draft.command} onChange={(e) => set({ command: e.target.value })} placeholder="npm run dev" aria-label="Start command" data-autofocus />
              <p className="text-xs text-muted">
                <code className="font-mono">{"{port}"}</code> becomes the port LampRabit picks; $PORT and $HOST are set too.
              </p>
              {Object.keys(info.scripts).length > 0 && (
                <div className="flex flex-wrap items-center gap-1.5">
                  <span className="text-xs text-muted">Scripts:</span>
                  {Object.entries(info.scripts).map(([name, script]) => (
                    <button
                      key={name}
                      type="button"
                      title={script}
                      onClick={() => {
                        // Dev servers that ignore $PORT get the port flag (Vite, Astro, Nuxt...).
                        const flag = node && /\b(vite|astro|nuxt|nuxi)\b/.test(script) ? `${runPrefix[pm].args} --host 127.0.0.1 --port {port}` : "";
                        set({ command: `${runPrefix[pm].run} ${name}${flag}` });
                      }}
                      className="rounded-md border border-line px-2 py-0.5 font-mono text-xs text-muted hover:border-accent/40 hover:text-fg"
                    >
                      {name}
                    </button>
                  ))}
                </div>
              )}
            </div>
          )}
          {node && (
            <Row title="Port" hint={draft.port ? "Kept while it's free; another one is picked if it isn't." : "Picked from 3000 up when it first starts."}>
              <span className="flex items-center gap-2">
                <input
                  type="number"
                  min={0}
                  max={65535}
                  className={clsx(inputClass.replace("w-full", ""), "w-28 font-mono")}
                  value={draft.port || ""}
                  placeholder="Auto"
                  onChange={(e) => set({ port: Number(e.target.value) || 0 })}
                  aria-label="Port"
                />
                {draft.port > 0 && (
                  <Button size="sm" variant="ghost" onClick={() => set({ port: 0 })}>
                    Auto
                  </Button>
                )}
              </span>
            </Row>
          )}
        </>
      )}

      <Row title="Node.js" hint="The project runs on this version; NVM's own default doesn't change.">
        <select className={clsx(inputClass.replace("w-full", ""), "w-52")} value={draft.node ?? ""} onChange={(e) => set({ node: e.target.value || null })}>
          <option value="">{info.projectNode ? `.nvmrc (${info.projectNode})` : "NVM's default"}</option>
          {info.nodeVersions.map((v) => (
            <option key={v} value={v}>
              {v}
            </option>
          ))}
        </select>
      </Row>
      {info.nodeVersions.length === 0 && <p className="text-xs text-danger-text">No Node.js versions are installed. Install one from Services → Node.js.</p>}
      <Row title="Package manager" hint="pnpm and Yarn run through npx when they aren't installed.">
        <select
          className={clsx(inputClass.replace("w-full", ""), "w-52")}
          value={draft.packageManager ?? ""}
          onChange={(e) => set({ packageManager: (e.target.value || null) as PackageManager | null })}
        >
          <option value="">From the lock file ({info.detectedPackageManager})</option>
          <option value="npm">npm</option>
          <option value="pnpm">pnpm</option>
          <option value="yarn">Yarn</option>
          <option value="bun">Bun</option>
        </select>
      </Row>

      {dirty && (
        <div className="flex justify-end gap-2 border-t border-line pt-3">
          <Button
            variant="ghost"
            onClick={() => {
              setDraft(info.config);
              setCustomising(false);
            }}
          >
            Cancel
          </Button>
          <Button variant="primary" onClick={() => save.mutate(draft)} disabled={save.isPending}>
            Save
          </Button>
        </div>
      )}
    </div>
  );
}

/** A label and hint on the left, a control on the right. */
export function Row({ title, hint, children }: { title: string; hint?: string; children?: React.ReactNode }) {
  return (
    <div className="flex flex-wrap items-center justify-between gap-x-6 gap-y-2">
      <div className="min-w-60 flex-1 basis-0">
        <div className="font-medium">{title}</div>
        {hint && <div className="mt-0.5 text-xs text-muted">{hint}</div>}
      </div>
      {children}
    </div>
  );
}

/** The project's commands: yours, its template's, and what package.json, composer.json and the framework CLIs offer. */
export function CommandsPanel({
  site,
  status,
  limit,
  onMore,
  onKeepRunning,
}: {
  site: Site;
  status?: AppStatus;
  limit?: number;
  onMore?: () => void;
  /** Turns a command into a process that keeps running. */
  onKeepRunning?: (c: { name: string; command: string }) => void;
}) {
  const queryClient = useQueryClient();
  const notify = useAppStore((s) => s.notify);
  const onError = (e: unknown) => notify("error", String(e));
  const list = useQuery({ queryKey: ["project-commands", site.name], queryFn: () => api.projectCommands(site.name), staleTime: 5 * 60_000 });
  const [filter, setFilter] = useState("");
  const running = status?.task ?? null;
  const compact = limit != null;

  const run = useMutation({ mutationFn: (c: { label: string; command: string }) => api.runProjectCommand(site.name, c.label, c.command), onError });
  const stop = useMutation({ mutationFn: () => api.stopProjectCommand(site.name), onError });
  const yours = useMemo(() => (list.data?.commands ?? []).filter((c) => c.origin === "yours").map((c) => ({ name: c.name, run: c.command })), [list.data]);
  const saveYours = useMutation({
    mutationFn: (commands: { name: string; run: string }[]) => api.saveProjectCommands(site.name, commands),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["project-commands", site.name] }),
    onError,
  });

  const all = list.data?.commands ?? [];
  const groups = useMemo(() => {
    const q = filter.trim().toLowerCase();
    const out = new Map<string, ProjectCommand[]>();
    for (const c of compact ? all.slice(0, limit) : all) {
      if (q && !`${c.name} ${c.command} ${c.description ?? ""}`.toLowerCase().includes(q)) continue;
      out.set(c.group, [...(out.get(c.group) ?? []), c]);
    }
    return [...out.entries()];
  }, [all, filter, compact, limit]);

  return (
    <div className="space-y-3">
      {running && (
        <div className="flex items-center gap-3 rounded-xl border border-accent/30 bg-accent-soft px-3 py-2 text-sm">
          <span className="size-2 shrink-0 animate-pulse rounded-full bg-accent" aria-hidden />
          <span className="min-w-0 flex-1 truncate">
            Running <span className="font-medium">{running}</span>
          </span>
          <Button size="sm" onClick={() => stop.mutate()}>
            <Square size={12} className="fill-current" /> Stop
          </Button>
        </div>
      )}

      {!compact && (
        <div className="flex items-center gap-1.5">
          <label className="flex h-9 min-w-0 flex-1 items-center gap-2 rounded-lg border border-line bg-surface px-3 focus-within:border-accent">
            <Search size={14} className="shrink-0 text-muted" aria-hidden />
            <input value={filter} onChange={(e) => setFilter(e.target.value)} placeholder={`Filter ${all.length} commands`} aria-label="Filter commands" className="w-full min-w-0 bg-transparent text-sm outline-none placeholder:text-muted" />
            {filter && (
              <button type="button" onClick={() => setFilter("")} aria-label="Clear filter" className="text-muted hover:text-fg">
                <X size={13} />
              </button>
            )}
          </label>
          <Button variant="ghost" onClick={() => list.refetch()} disabled={list.isFetching} aria-label="Look for commands again" title="Look for commands again">
            <RefreshCw size={14} className={clsx(list.isFetching && "animate-spin")} />
          </Button>
        </div>
      )}

      {list.isLoading && <p className="text-sm text-muted">Looking for commands…</p>}
      {list.error && <p className="text-sm text-danger-text">{String(list.error)}</p>}
      {!compact &&
        list.data?.problems.map((p) => (
          <p key={p} className="text-xs text-warn">
            {p}
          </p>
        ))}
      {list.data && all.length === 0 && <p className="text-sm text-muted">No commands found{compact ? "." : ". Type any command under the output, or keep your own with +."}</p>}

      <div className={clsx("space-y-3", !compact && "max-h-[26rem] overflow-y-auto pr-1")}>
        {groups.map(([group, commands]) => (
          <section key={group}>
            {!compact && (
              <h4 className="mb-1.5 text-[11px] font-bold tracking-wider text-muted uppercase">
                {group} <span className="font-normal">· {commands.length}</span>
              </h4>
            )}
            <ul className={clsx(compact ? "grid gap-2 sm:grid-cols-2" : "divide-y divide-line rounded-xl border border-line")}>
              {commands.map((c) => (
                <li
                  key={`${c.group}:${c.name}`}
                  className={clsx("group flex items-center gap-3", compact ? "rounded-xl border border-line px-3 py-2 hover:border-accent/40" : "px-3 py-2 hover:bg-surface-2/60")}
                >
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-sm font-medium">{c.name}</span>
                    <span className="block truncate font-mono text-[11px] text-muted" title={c.description ?? c.command}>
                      {c.description && c.description !== c.command ? c.description : c.command}
                    </span>
                  </span>
                  {c.origin === "yours" && !compact && (
                    <button
                      type="button"
                      aria-label={`Remove ${c.name}`}
                      title="Remove"
                      onClick={() => saveYours.mutate(yours.filter((y) => y.name !== c.name))}
                      className="rounded p-1 text-muted opacity-0 group-hover:opacity-100 hover:bg-surface-2 hover:text-danger-text focus:opacity-100"
                    >
                      <Trash2 size={13} />
                    </button>
                  )}
                  {onKeepRunning && !compact && (
                    <button
                      type="button"
                      aria-label={`Keep ${c.name} running`}
                      title="Keep it running (as a process)"
                      onClick={() => onKeepRunning({ name: c.name, command: c.command })}
                      className="rounded p-1 text-muted opacity-0 group-hover:opacity-100 hover:bg-surface-2 hover:text-accent-text focus:opacity-100"
                    >
                      <Forever size={14} />
                    </button>
                  )}
                  <Button size="sm" variant="soft" onClick={() => run.mutate({ label: c.name, command: c.command })} disabled={!!running} title={c.command} aria-label={`Run ${c.name}`}>
                    <Play size={12} /> Run
                  </Button>
                </li>
              ))}
            </ul>
          </section>
        ))}
      </div>
      {compact && onMore && all.length > (limit ?? 0) && (
        <button type="button" onClick={onMore} className="flex items-center gap-1 text-sm font-medium text-accent-text hover:underline">
          All {all.length} commands <ChevronRight size={14} />
        </button>
      )}
    </div>
  );
}

/**
 * New output lines per process of a project while it isn't the one shown: the counts on
 * its output tab and row. `viewing` resets to zero.
 */
export function useUnreadOutput(project: string, viewing: string) {
  const [counts, setCounts] = useState<Record<string, number>>({});
  const shown = useRef(viewing);
  shown.current = viewing;
  useEffect(() => {
    setCounts((c) => (c[viewing] ? { ...c, [viewing]: 0 } : c));
  }, [viewing]);
  useEffect(() => {
    const off = listen<AppOutput>("app-output", ({ payload }) => {
      if (!payload.name.startsWith(`${project}/`) || payload.name === shown.current) return;
      setCounts((c) => ({ ...c, [payload.name]: (c[payload.name] ?? 0) + 1 }));
    });
    return () => {
      off.then((f) => f());
    };
  }, [project]);
  return counts;
}

/** The project's output: its process and the commands run for it, live. */
export function OutputPanel({ name, height = "max-h-96 min-h-40", title, footer }: { name: string; height?: string; title?: ReactNode; footer?: ReactNode }) {
  const showLogs = useAppStore((s) => s.showLogs);
  const [lines, setLines] = useState<{ line: string; stream: string }[]>([]);
  const box = useRef<HTMLPreElement>(null);
  // Follows new output only while scrolled to the bottom, so reading earlier lines isn't interrupted.
  const atEnd = useRef(true);

  useEffect(() => {
    let alive = true;
    api.appLogs(name).then((l) => alive && setLines(l.map((line) => ({ line, stream: /^\[[^\]]+\] (\w+):/.exec(line)?.[1] ?? "output" }))));
    const off = listen<AppOutput>("app-output", ({ payload }) => {
      if (payload.name === name) setLines((cur) => [...cur.slice(-499), { line: payload.line, stream: payload.stream }]);
    });
    return () => {
      alive = false;
      off.then((f) => f());
    };
  }, [name]);
  useEffect(() => {
    if (box.current && atEnd.current) box.current.scrollTop = box.current.scrollHeight;
  }, [lines]);

  return (
    <div className="min-w-0 overflow-hidden rounded-xl border border-white/10 bg-terminal">
      <div className="flex items-center gap-2 border-b border-white/10 px-3 py-1.5">
        {title ?? <span className="flex-1 text-xs font-semibold text-slate-300">Output</span>}
        <button type="button" onClick={() => setLines([])} disabled={lines.length === 0} className="rounded px-2 py-1 text-xs text-slate-400 hover:bg-white/10 hover:text-slate-200 disabled:opacity-40" title="Clear this view (the log file keeps it)">
          Clear
        </button>
        <button type="button" onClick={() => showLogs(`project:${name}`)} className="flex items-center gap-1 rounded px-2 py-1 text-xs text-slate-400 hover:bg-white/10 hover:text-slate-200">
          <ScrollText size={12} /> Open in Logs
        </button>
      </div>
      <pre
        ref={box}
        onScroll={(e) => {
          const el = e.currentTarget;
          atEnd.current = el.scrollHeight - el.scrollTop - el.clientHeight < 24;
        }}
        className={clsx("scrollbar-none selectable overflow-y-auto px-3 py-2 font-mono text-[11.5px] leading-relaxed break-all whitespace-pre-wrap text-slate-300", height)}
      >
        {lines.length === 0 ? (
          <span className="text-slate-500">Output from the dev server and from commands shows here.</span>
        ) : (
          lines.map((l, i) => (
            <div key={i} className={clsx(l.stream === "error" && "text-red-300", l.stream === "command" && "text-violet-300")}>
              {l.line.replace(/^\[\d{4}-\d{2}-\d{2} (\d{2}:\d{2}:\d{2})\] \w+: /, "$1  ")}
            </div>
          ))
        )}
      </pre>
      {footer}
    </div>
  );
}

/**
 * The terminal's prompt line, under the output: any command, run in the project with its
 * PHP and Node. Play runs it (Stop while something runs), + keeps it in the Yours group.
 */
export function CommandPrompt({ site, status }: { site: Site; status?: AppStatus }) {
  const queryClient = useQueryClient();
  const notify = useAppStore((s) => s.notify);
  const onError = (e: unknown) => notify("error", String(e));
  const list = useQuery({ queryKey: ["project-commands", site.name], queryFn: () => api.projectCommands(site.name), staleTime: 5 * 60_000 });
  const [command, setCommand] = useState("");
  const history = useQuery({ queryKey: ["command-history", site.name], queryFn: () => api.commandHistory(site.name) });
  // Earlier commands, newest first, each once; -1 is what's being typed.
  const earlier = useMemo(() => [...new Set((history.data ?? []).map((r) => r.command))], [history.data]);
  const [back, setBack] = useState(-1);
  const draft = useRef("");
  const running = status?.task ?? null;
  const run = useMutation({
    mutationFn: (c: string) => api.runProjectCommand(site.name, c, c),
    onSuccess: () => {
      setCommand("");
      setBack(-1);
    },
    onError,
  });
  const recall = (step: 1 | -1) => {
    const next = Math.max(-1, Math.min(earlier.length - 1, back + step));
    if (next === back) return;
    if (back === -1) draft.current = command;
    setBack(next);
    setCommand(next === -1 ? draft.current : earlier[next]);
  };
  const stop = useMutation({ mutationFn: () => api.stopProjectCommand(site.name), onError });
  const keep = useMutation({
    mutationFn: (c: string) =>
      api.saveProjectCommands(site.name, [...(list.data?.commands ?? []).filter((x) => x.origin === "yours").map((x) => ({ name: x.name, run: x.command })), { name: c, run: c }]),
    onSuccess: () => {
      setCommand("");
      queryClient.invalidateQueries({ queryKey: ["project-commands", site.name] });
    },
    onError,
  });
  const text = command.trim();
  const icon = "flex size-7 shrink-0 items-center justify-center rounded-md text-slate-400 hover:bg-white/10 hover:text-slate-100 disabled:opacity-30 disabled:hover:bg-transparent";

  return (
    <form
      className="flex items-center gap-1 border-t border-white/10 px-3 py-1.5"
      onSubmit={(e) => {
        e.preventDefault();
        if (text && !running) run.mutate(text);
      }}
    >
      <span className="font-mono text-sm text-emerald-400" aria-hidden>
        $
      </span>
      <input
        value={command}
        onChange={(e) => {
          setCommand(e.target.value);
          setBack(-1);
        }}
        onKeyDown={(e) => {
          if (e.key === "ArrowUp" || e.key === "ArrowDown") {
            e.preventDefault();
            recall(e.key === "ArrowUp" ? 1 : -1);
          }
        }}
        title="↑ and ↓ go through earlier commands"
        placeholder={running ? `Running ${running}…` : site.language === "php" ? "composer update, php artisan migrate…" : "npm outdated, npx …"}
        aria-label="Command to run"
        className="h-8 min-w-0 flex-1 bg-transparent px-1.5 font-mono text-[12.5px] text-slate-100 outline-none placeholder:text-slate-500"
      />
      {running ? (
        <button type="button" onClick={() => stop.mutate()} className={clsx(icon, "text-red-300")} title={`Stop ${running}`} aria-label={`Stop ${running}`}>
          <Square size={12} className="fill-current" />
        </button>
      ) : (
        <button type="submit" disabled={!text || run.isPending} className={clsx(icon, "text-emerald-400")} title="Run" aria-label="Run">
          <Play size={14} className="fill-current" />
        </button>
      )}
      <button type="button" disabled={!text || keep.isPending} onClick={() => keep.mutate(text)} className={icon} title="Keep it in Yours" aria-label="Keep it in Yours">
        <Plus size={15} />
      </button>
    </form>
  );
}
