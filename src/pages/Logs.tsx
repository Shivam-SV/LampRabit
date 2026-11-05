import { useEffect, useMemo, useRef, useState } from "react";
import { CaseSensitive, ChevronDown, ChevronRight, Copy, Download, EyeOff, FileCode, Pause, Play, Regex, RotateCcw, Search, Trash2, WrapText, X } from "lucide-react";
import clsx from "clsx";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { save } from "@tauri-apps/plugin-dialog";
import { useVirtualizer } from "@tanstack/react-virtual";
import { api, serviceLabels, type Dump, type DumpNode, type LogStream, type ServiceKind } from "../lib/api";
import type { Level, LogEntry } from "../lib/logparse";
import { useAppStore } from "../lib/store";
import { Button, Card, PageHeader, Switch } from "../components/ui";

/** Badge colours for the sources that have one; the rest share a neutral one. */
const badgeColors: Record<string, string> = {
  apache: "bg-rose-500/20 text-rose-300",
  nginx: "bg-emerald-500/20 text-emerald-300",
  caddy: "bg-cyan-500/20 text-cyan-300",
  frankenphp: "bg-violet-500/20 text-violet-300",
  tomcat: "bg-yellow-500/20 text-yellow-300",
  php: "bg-indigo-500/25 text-indigo-300",
  mariadb: "bg-amber-500/20 text-amber-300",
  mysql: "bg-blue-500/20 text-blue-300",
  postgres: "bg-sky-500/20 text-sky-300",
  mongodb: "bg-green-500/20 text-green-300",
  redis: "bg-red-500/20 text-red-300",
  mailpit: "bg-sky-500/20 text-sky-300",
  system: "bg-emerald-500/15 text-emerald-300",
  dumps: "bg-fuchsia-500/20 text-fuchsia-300",
};
const neutralBadge = "bg-slate-500/25 text-slate-200";
const projectBadge = "bg-violet-500/20 text-violet-300";

/** Sources that aren't services. */
const ownSources: Record<string, string> = { php: "PHP", system: "System", dumps: "Dumps" };

/** A line's source: a service id, "dumps", "system", or "project:<name>". */
const sourceOf = (l: LogEntry) => (l.service === "project" ? `project:${(l.project ?? "").split("/")[0]}` : l.service);
const sourceLabel = (source: string) => (source.startsWith("project:") ? source.slice(8) : (ownSources[source] ?? serviceLabels[source as ServiceKind]));

const levels: Record<Level, { dot: string; text: string }> = {
  error: { dot: "bg-red-400", text: "text-red-400" },
  warn: { dot: "bg-amber-400", text: "text-amber-300" },
  notice: { dot: "bg-yellow-300", text: "text-yellow-200" },
  info: { dot: "bg-sky-400", text: "text-sky-300" },
  access: { dot: "bg-sky-400", text: "text-sky-300" },
  debug: { dot: "bg-slate-500", text: "text-slate-400" },
};
const levelOrder: Level[] = ["error", "warn", "notice", "info", "access", "debug"];

/** Streams, in the words the filter uses. */
const streams: { id: LogStream; label: string; hint: string }[] = [
  { id: "output", label: "Output", hint: "What services and projects print" },
  { id: "error", label: "Errors", hint: "Error logs and what programs print to stderr" },
  { id: "access", label: "Access", hint: "Web server request logs" },
  { id: "command", label: "Commands", hint: "Commands LampRabit ran for projects, and how they ended" },
];

const ranges = [
  { id: "all", label: "All time", ms: Infinity },
  { id: "5m", label: "Last 5 minutes", ms: 5 * 60_000 },
  { id: "15m", label: "Last 15 minutes", ms: 15 * 60_000 },
  { id: "1h", label: "Last hour", ms: 60 * 60_000 },
  { id: "24h", label: "Last 24 hours", ms: 24 * 60 * 60_000 },
] as const;

const pad = (n: number) => String(n).padStart(2, "0");
const stamp = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
const columns = "grid-cols-[9.5rem_5.5rem_4rem_minmax(0,1fr)]";

export default function Logs() {
  const queryClient = useQueryClient();
  const logs = useAppStore((s) => s.logs);
  const clearLogs = useAppStore((s) => s.clearLogs);
  const loadHistory = useAppStore((s) => s.loadHistory);
  const notify = useAppStore((s) => s.notify);
  const onError = (e: unknown) => notify("error", String(e));
  const sites = useQuery({ queryKey: ["sites"], queryFn: api.listSites });
  const services = useQuery({ queryKey: ["services"], queryFn: api.listServices });
  const settings = useQuery({ queryKey: ["settings"], queryFn: api.getSettings });

  // Caught dumps, kept current from the backend.
  const dumps = useQuery({ queryKey: ["dumps"], queryFn: api.listDumps });
  useEffect(() => {
    const off = listen("dump", () => queryClient.invalidateQueries({ queryKey: ["dumps"] }));
    return () => {
      off.then((f) => f());
    };
  }, [queryClient]);
  const catchDumps = useMutation({
    mutationFn: api.setCatchDumps,
    onSuccess: (_, on) => {
      notify("info", on ? "dump() and dd() now show here. PHP restarted to pick it up." : "dump() and dd() show in the page again.");
      queryClient.invalidateQueries({ queryKey: ["settings"] });
    },
    onError,
  });

  // Lines per source, for the sidebar.
  const counts = useMemo(() => {
    const out = new Map<string, number>();
    for (const l of logs) out.set(sourceOf(l), (out.get(sourceOf(l)) ?? 0) + 1);
    return out;
  }, [logs]);
  // Installed services, then LampRabit's own sources, then any other service that has lines.
  const serviceSources = useMemo(() => {
    const ids = [...(services.data ?? []).filter((s) => s.installed).map((s) => s.kind as string), "php", "dumps", "system"];
    for (const key of counts.keys()) if (!key.startsWith("project:") && !ids.includes(key)) ids.push(key);
    return [...new Set(ids)];
  }, [services.data, counts]);
  const projects = useMemo(() => {
    const names = new Set<string>();
    for (const key of counts.keys()) if (key.startsWith("project:")) names.add(key.slice(8));
    for (const s of sites.data?.sites ?? []) if (s.nodeProject) names.add(s.name);
    return [...names].filter(Boolean).sort();
  }, [counts, sites.data]);
  const allSources = useMemo(() => [...serviceSources, ...projects.map((p) => `project:${p}`)], [serviceSources, projects]);

  // A service's or project's "View logs" hands over a source to filter to. The page stays
  // mounted between visits, so each new one is applied when it arrives, then cleared.
  const focus = useAppStore((s) => s.logFilter);
  /** Sources switched off; new ones show up switched on. */
  const [hidden, setHidden] = useState<Set<string>>(new Set());
  useEffect(() => {
    if (!focus) return;
    setHidden(new Set(allSources.filter((s) => s !== focus)));
    useAppStore.setState({ logFilter: null });
    // Only when a new focus arrives.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [focus]);

  const [wrapLines, setWrapLines] = useState(true);
  const [confirmClear, setConfirmClear] = useState(false);
  const [loading, setLoading] = useState(true);
  const sourcePicker = useRef<HTMLDetailsElement>(null);

  // The narrow-screen source picker is a <details>: close it on outside clicks and Escape.
  useEffect(() => {
    const close = (e: MouseEvent | KeyboardEvent) => {
      const picker = sourcePicker.current;
      if (!picker?.open) return;
      if (e instanceof KeyboardEvent ? e.key === "Escape" : !picker.contains(e.target as Node)) picker.open = false;
    };
    document.addEventListener("mousedown", close);
    document.addEventListener("keydown", close);
    return () => {
      document.removeEventListener("mousedown", close);
      document.removeEventListener("keydown", close);
    };
  }, []);

  // Always show what is on disk, including earlier runs.
  useEffect(() => {
    api
      .logHistory()
      .then(loadHistory)
      .catch((e) => notify("error", String(e)))
      .finally(() => setLoading(false));
  }, [loadHistory, notify]);

  /** Empties one source's files (or all), then reloads what's left from disk. */
  const clear = async (source?: string) => {
    setConfirmClear(false);
    setFrozen(null);
    setSelected(null);
    if (!source) clearLogs();
    try {
      await api.clearLogs(source);
    } catch (e) {
      notify("error", String(e));
    }
    loadHistory(await api.logHistory());
    if (!source || source === "dumps") queryClient.invalidateQueries({ queryKey: ["dumps"] });
  };
  const [query, setQuery] = useState("");
  const [useRegex, setUseRegex] = useState(false);
  const [caseSensitive, setCaseSensitive] = useState(false);
  /** Hide matching lines instead of showing only them (quieting noisy output). */
  const [exclude, setExclude] = useState(false);
  const [shownLevels, setShownLevels] = useState<Set<Level>>(() => new Set(levelOrder));
  const [shownStreams, setShownStreams] = useState<Set<LogStream>>(() => new Set(streams.map((s) => s.id)));
  const [range, setRange] = useState<(typeof ranges)[number]["id"]>("all");
  const [autoScroll, setAutoScroll] = useState(true);
  /** When paused, the view is frozen at these lines. */
  const [frozen, setFrozen] = useState<LogEntry[] | null>(null);
  /** The line whose details show under the list. */
  const [selected, setSelected] = useState<number | null>(null);
  const scroller = useRef<HTMLDivElement>(null);

  // The search as a test, or why the regex is invalid.
  const search = useMemo((): { test: (s: string) => boolean } | { error: string } | null => {
    const q = query.trim();
    if (!q) return null;
    if (useRegex) {
      try {
        const re = new RegExp(q, caseSensitive ? "" : "i");
        return { test: (s) => re.test(s) };
      } catch (e) {
        return { error: (e as Error).message };
      }
    }
    const needle = caseSensitive ? q : q.toLowerCase();
    return { test: (s) => (caseSensitive ? s : s.toLowerCase()).includes(needle) };
  }, [query, useRegex, caseSensitive]);

  const source = frozen ?? logs;
  const shownSources = allSources.filter((s) => !hidden.has(s));
  const sourceSummary =
    shownSources.length === allSources.length ? "All sources" : shownSources.length === 1 ? sourceLabel(shownSources[0]) : shownSources.length === 0 ? "No sources" : `${shownSources.length} sources`;
  const visible = useMemo(() => {
    const since = Date.now() - ranges.find((r) => r.id === range)!.ms;
    const test = search && "test" in search ? search.test : null;
    return source.filter(
      (l) => !hidden.has(sourceOf(l)) && shownLevels.has(l.level) && shownStreams.has(l.stream) && l.time.getTime() >= since && (!test || test(l.message) !== exclude),
    );
  }, [source, hidden, search, exclude, shownLevels, shownStreams, range]);

  const toggleIn = <T,>(set: (f: (s: Set<T>) => Set<T>) => void, value: T) =>
    set((current) => {
      const next = new Set(current);
      if (next.has(value)) next.delete(value);
      else next.add(value);
      return next;
    });
  const solo = (id: string) => setHidden(new Set(allSources.filter((s) => s !== id)));
  const filtered = hidden.size > 0 || !!query.trim() || shownLevels.size !== levelOrder.length || shownStreams.size !== streams.length || range !== "all";
  const resetFilters = () => {
    setHidden(new Set());
    setQuery("");
    setExclude(false);
    setShownLevels(new Set(levelOrder));
    setShownStreams(new Set(streams.map((s) => s.id)));
    setRange("all");
  };

  const asText = () => visible.map((l) => `[${stamp(l.time)}] [${sourceLabel(sourceOf(l))}] ${l.level.padEnd(6)} | ${l.message}`).join("\n");
  const exportLogs = async () => {
    const path = await save({
      defaultPath: `lamprabit-logs-${stamp(new Date()).replace(/[: ]/g, "-")}.log`,
      filters: [{ name: "Log file", extensions: ["log", "txt"] }],
    });
    if (!path) return;
    try {
      await api.saveText(path, asText() + "\n");
      notify("info", `Exported ${visible.length} lines`);
    } catch (e) {
      notify("error", String(e));
    }
  };
  const copyText = async (text: string, what: string) => {
    try {
      await navigator.clipboard.writeText(text);
      notify("info", `Copied ${what}`);
    } catch (e) {
      notify("error", String(e));
    }
  };

  // Only the rows in view are drawn, so thousands of lines stay quick.
  const rows = useVirtualizer({
    count: visible.length,
    getScrollElement: () => scroller.current,
    estimateSize: () => 30,
    overscan: 20,
    getItemKey: (i) => visible[i].id,
  });
  useEffect(() => {
    if (autoScroll && !frozen && visible.length > 0) rows.scrollToIndex(visible.length - 1, { align: "end" });
  }, [visible.length, autoScroll, frozen, rows]);

  const selectedLine = selected == null ? null : (source.find((l) => l.id === selected) ?? null);

  const toggleChip = (on: boolean) =>
    clsx("flex items-center gap-1.5 rounded-md px-2 py-1 text-xs font-medium transition-colors", on ? "bg-surface text-fg shadow-card" : "text-muted hover:bg-surface-2 hover:text-fg");
  const iconToggle = (on: boolean) =>
    clsx("flex size-7 shrink-0 items-center justify-center rounded-md transition-colors", on ? "bg-accent-soft text-accent-text" : "text-muted hover:bg-surface-2 hover:text-fg");
  const sourceCheckbox = (id: string, label: string) => (
    <label key={id} className="flex cursor-pointer items-center gap-2 rounded-lg px-2.5 py-1.5 text-sm hover:bg-surface-2">
      <input type="checkbox" className="size-4 accent-(--accent)" checked={!hidden.has(id)} onChange={() => toggleIn(setHidden, id)} />
      <span className="truncate">{label}</span>
    </label>
  );

  return (
    <div className="flex h-full min-h-0 flex-col overflow-hidden motion-stagger">
      <PageHeader title="Logs" subtitle="Live output from your services and projects, and what your code dumps." />

      <Card className="mb-3 shrink-0 space-y-2.5 p-3">
        <div className="flex flex-wrap items-center gap-2">
          {/* On narrow screens the sources are a menu; wider ones have the sidebar. */}
          <details ref={sourcePicker} className="group relative shrink-0 lg:hidden">
            <summary className="flex h-10 min-w-40 cursor-pointer list-none items-center justify-between gap-3 rounded-lg border border-line bg-surface px-3 text-sm hover:border-accent/50 [&::-webkit-details-marker]:hidden">
              <span>{sourceSummary}</span>
              <ChevronDown size={14} className="text-muted transition-transform group-open:rotate-180" />
            </summary>
            <div className="absolute top-full left-0 z-30 mt-1.5 max-h-96 w-64 overflow-y-auto rounded-xl border border-line bg-surface p-1.5 shadow-float">
              <label className="flex cursor-pointer items-center gap-2 rounded-lg px-2.5 py-2 text-sm font-semibold hover:bg-surface-2">
                <input type="checkbox" className="size-4 accent-(--accent)" checked={hidden.size === 0} onChange={() => setHidden(hidden.size === 0 ? new Set(allSources) : new Set())} />
                All sources
              </label>
              <div className="mt-1 px-2.5 pt-1.5 pb-0.5 text-[11px] font-bold tracking-wider text-muted uppercase">Services</div>
              {serviceSources.map((s) => sourceCheckbox(s, sourceLabel(s)))}
              {projects.length > 0 && (
                <>
                  <div className="mt-1 px-2.5 pt-1.5 pb-0.5 text-[11px] font-bold tracking-wider text-muted uppercase">Projects</div>
                  {projects.map((p) => sourceCheckbox(`project:${p}`, p))}
                </>
              )}
            </div>
          </details>

          <label
            className={clsx(
              "flex h-10 min-w-64 flex-1 basis-64 items-center gap-2 rounded-lg border bg-surface pr-1 pl-3 focus-within:border-accent",
              search && "error" in search ? "border-danger" : "border-line",
            )}
          >
            <Search size={15} className="shrink-0 text-muted" aria-hidden />
            <input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder={useRegex ? "Regular expression, e.g. error|fail" : "Search log messages"}
              aria-label="Search log messages"
              className={clsx("w-full bg-transparent text-sm outline-none placeholder:text-muted", useRegex && "font-mono")}
            />
            {query && (
              <button type="button" onClick={() => setQuery("")} aria-label="Clear search" className="rounded p-1 text-muted hover:bg-surface-2 hover:text-fg">
                <X size={14} />
              </button>
            )}
            <button type="button" onClick={() => setCaseSensitive((v) => !v)} aria-pressed={caseSensitive} title="Match case" aria-label="Match case" className={iconToggle(caseSensitive)}>
              <CaseSensitive size={16} />
            </button>
            <button type="button" onClick={() => setUseRegex((v) => !v)} aria-pressed={useRegex} title="Regular expression" aria-label="Regular expression" className={iconToggle(useRegex)}>
              <Regex size={15} />
            </button>
            <button type="button" onClick={() => setExclude((v) => !v)} aria-pressed={exclude} title="Hide matching lines instead" aria-label="Hide matching lines" className={iconToggle(exclude)}>
              <EyeOff size={15} />
            </button>
          </label>

          <label className="relative">
            <span className="sr-only">Time range</span>
            <select
              value={range}
              onChange={(e) => setRange(e.target.value as typeof range)}
              aria-label="Time range"
              className="h-10 w-40 appearance-none rounded-lg border border-line bg-surface px-3 pr-8 text-sm text-fg outline-none focus:border-accent focus:ring-2 focus:ring-accent/20"
            >
              {ranges.map((r) => (
                <option key={r.id} value={r.id}>
                  {r.label}
                </option>
              ))}
            </select>
            <ChevronDown size={14} className="pointer-events-none absolute top-1/2 right-2.5 -translate-y-1/2 text-muted" aria-hidden />
          </label>

          <button
            type="button"
            onClick={() => setFrozen((current) => (current ? null : logs))}
            aria-pressed={!frozen}
            className="flex h-10 items-center gap-2 rounded-lg border border-line bg-surface px-3 text-sm font-medium text-fg hover:bg-surface-2"
          >
            <span className={clsx("size-2 rounded-full", frozen ? "bg-slate-400" : "animate-pulse bg-emerald-500")} aria-hidden />
            {frozen ? (
              <>
                <Play size={14} /> Paused
              </>
            ) : (
              <>
                <Pause size={14} /> Live
              </>
            )}
          </button>
          <Button variant="ghost" onClick={() => setWrapLines((current) => !current)} aria-pressed={wrapLines} title={wrapLines ? "One line per entry (the full text is in its details)" : "Wrap long lines"}>
            <WrapText size={15} /> {wrapLines ? "Wrap" : "No wrap"}
          </Button>
          <Button variant="ghost" onClick={() => copyText(asText(), `${visible.length} log ${visible.length === 1 ? "line" : "lines"}`)} disabled={visible.length === 0}>
            <Copy size={14} /> Copy
          </Button>
          <Button variant="ghost" onClick={exportLogs} disabled={visible.length === 0}>
            <Download size={14} /> Export
          </Button>
          {confirmClear ? (
            <div className="flex items-center gap-2 rounded-lg border border-danger/30 bg-danger-soft px-2 py-1">
              <span className="text-xs text-danger-text">Delete all log files?</span>
              <Button size="sm" variant="danger" onClick={() => clear()}>
                Clear
              </Button>
              <Button size="sm" onClick={() => setConfirmClear(false)}>
                Cancel
              </Button>
            </div>
          ) : (
            <Button variant="ghost" onClick={() => setConfirmClear(true)}>
              <Trash2 size={14} /> Clear
            </Button>
          )}
        </div>

        <div className="flex flex-wrap items-center gap-x-4 gap-y-2 border-t border-line pt-2.5">
          <div className="flex items-center gap-2">
            <span className="text-xs font-semibold text-muted">Severity</span>
            <div className="flex flex-wrap items-center gap-0.5 rounded-lg border border-line bg-bg p-1" role="group" aria-label="Filter by severity">
              {levelOrder.map((level) => (
                <button key={level} onClick={() => toggleIn(setShownLevels, level)} aria-pressed={shownLevels.has(level)} className={clsx(toggleChip(shownLevels.has(level)), "capitalize")}>
                  <span className={clsx("size-1.5 rounded-full", levels[level].dot)} />
                  {level}
                </button>
              ))}
            </div>
          </div>
          <div className="flex items-center gap-2">
            <span className="text-xs font-semibold text-muted">Stream</span>
            <div className="flex flex-wrap items-center gap-0.5 rounded-lg border border-line bg-bg p-1" role="group" aria-label="Filter by stream">
              {streams.map((s) => (
                <button key={s.id} onClick={() => toggleIn(setShownStreams, s.id)} aria-pressed={shownStreams.has(s.id)} title={s.hint} className={toggleChip(shownStreams.has(s.id))}>
                  {s.label}
                </button>
              ))}
            </div>
          </div>
          <span className="ml-auto flex items-center gap-3 text-xs tabular-nums text-muted">
            {search && "error" in search && <span className="text-danger-text">Invalid expression: {search.error}</span>}
            {filtered && (
              <button type="button" onClick={resetFilters} className="flex items-center gap-1 font-medium text-accent-text hover:underline">
                <RotateCcw size={12} /> Reset filters
              </button>
            )}
          </span>
        </div>
      </Card>

      <div className="flex min-h-0 flex-1 gap-3">
        {/* Sources, with their line counts: click to show only one, the box to switch one off. */}
        <Card className="hidden w-60 shrink-0 flex-col overflow-hidden lg:flex">
          <div className="flex items-center justify-between gap-2 border-b border-line px-3 py-2.5">
            <span className="text-xs font-bold tracking-wider text-muted uppercase">Sources</span>
            <button type="button" onClick={() => setHidden(new Set())} disabled={hidden.size === 0} className="text-xs font-medium text-accent-text hover:underline disabled:opacity-0">
              Show all
            </button>
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto p-1.5">
            {serviceSources.map((id) => (
              <SourceRow key={id} id={id} label={sourceLabel(id)} count={counts.get(id) ?? 0} on={!hidden.has(id)} onToggle={() => toggleIn(setHidden, id)} onSolo={() => solo(id)} onClear={() => clear(id)}>
                {id === "dumps" && (
                  <label className="mx-2 mb-1.5 flex items-start gap-2 rounded-lg bg-surface-2/60 px-2 py-1.5 text-[11px] text-muted">
                    <Switch checked={!!settings.data?.catchDumps} onChange={(v) => catchDumps.mutate(v)} disabled={catchDumps.isPending || !settings.data} label="Catch dump() and dd()" />
                    <span>
                      <span className="block font-medium text-fg">Catch dump() and dd()</span>
                      Laravel and Symfony dumps show here instead of in the page (web requests).
                    </span>
                  </label>
                )}
              </SourceRow>
            ))}
            {projects.length > 0 && <div className="mt-2 px-2 pt-1 pb-0.5 text-[11px] font-bold tracking-wider text-muted uppercase">Projects</div>}
            {projects.map((p) => {
              const id = `project:${p}`;
              return <SourceRow key={id} id={id} label={p} count={counts.get(id) ?? 0} on={!hidden.has(id)} onToggle={() => toggleIn(setHidden, id)} onSolo={() => solo(id)} onClear={() => clear(id)} />;
            })}
          </div>
        </Card>

        <section className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-2xl border border-white/10 bg-terminal text-slate-200 shadow-float" aria-label="Log output">
          <div className="flex shrink-0 items-center justify-between gap-3 border-b border-white/10 bg-terminal-2 px-4 py-2.5 text-xs">
            <span className="font-semibold text-slate-200">Log output</span>
            <span className="text-slate-400">
              {sourceSummary} · {visible.length} {visible.length === 1 ? "line" : "lines"}
            </span>
          </div>
          <div ref={scroller} className="selectable scrollbar-none min-h-0 flex-1 overflow-y-auto overscroll-contain pb-3 font-mono text-[12px] leading-6">
            <div className={clsx("sticky top-0 z-10 grid border-b border-white/10 bg-terminal-2 px-4 py-2 text-[11px] font-semibold tracking-wide text-slate-400 uppercase", columns)}>
              <span>Timestamp</span>
              <span>Source</span>
              <span>Level</span>
              <span>Message</span>
            </div>
            {visible.length === 0 ? (
              <div className="flex min-h-48 items-center justify-center px-4 text-center font-sans text-sm text-slate-400">
                {loading ? "Loading logs…" : logs.length === 0 ? "No log output yet. Start a service or a project to see its logs here." : "No lines match your filters."}
              </div>
            ) : (
              <div className="relative w-full" style={{ height: rows.getTotalSize() }}>
                {rows.getVirtualItems().map((item) => {
                  const l = visible[item.index];
                  const src = sourceOf(l);
                  return (
                    <div
                      key={item.key}
                      data-index={item.index}
                      ref={rows.measureElement}
                      onClick={() => {
                        // Selecting text isn't a click on the line.
                        if (!window.getSelection()?.toString()) setSelected(l.id === selected ? null : l.id);
                      }}
                      className={clsx(
                        "absolute top-0 left-0 grid w-full cursor-pointer items-start px-4 py-1.5 hover:bg-white/5",
                        columns,
                        l.stream === "command" && "bg-white/3",
                        l.id === selected && "bg-white/10 hover:bg-white/10",
                      )}
                      style={{ transform: `translateY(${item.start}px)` }}
                    >
                      <span className="whitespace-nowrap pr-3 text-slate-400">{stamp(l.time)}</span>
                      <span className="min-w-0 pr-3">
                        <span className={clsx("inline-flex max-w-full truncate rounded-md px-1 text-[10px]", l.service === "project" ? projectBadge : (badgeColors[l.service] ?? neutralBadge))} title={sourceLabel(src)}>
                          {sourceLabel(src)}
                        </span>
                      </span>
                      <span className={clsx("whitespace-nowrap pr-3 text-[10px] capitalize", levels[l.level].text)}>
                        <span className={clsx("mr-2 inline-block size-1.5 rounded-full align-middle", levels[l.level].dot)} />
                        {l.level}
                      </span>
                      <span className={clsx("min-w-0", wrapLines ? "wrap-break-word whitespace-pre-wrap" : "truncate whitespace-pre", l.stream === "command" && "text-violet-200", l.service === "dumps" && "text-fuchsia-200")}>
                        {l.message}
                      </span>
                    </div>
                  );
                })}
              </div>
            )}
          </div>

          {selectedLine && (
            <LineDetails
              line={selectedLine}
              lines={source}
              dumps={dumps.data ?? []}
              onClose={() => setSelected(null)}
              onCopy={(text) => copyText(text, "the line")}
              onOpenFile={(file) => api.openInEditor(file).catch(onError)}
            />
          )}

          <div className="sticky bottom-0 z-10 flex shrink-0 flex-wrap items-center gap-3 border-t border-white/10 bg-terminal-2 px-4 py-2.5 text-xs text-slate-300">
            <span className={clsx("size-2 rounded-full", frozen ? "bg-slate-400" : "animate-pulse bg-emerald-400")} />
            <span className={clsx("font-semibold", frozen ? "text-slate-300" : "text-emerald-300")}>{frozen ? "Paused" : "Live"}</span>
            <span className="text-slate-500">·</span>
            <span>
              {visible.length} matching {visible.length === 1 ? "line" : "lines"}
            </span>
            <span className="text-slate-500">· click a line for its details</span>
            <span className="flex-1" />
            <span>Auto-scroll</span>
            <Switch checked={autoScroll} onChange={setAutoScroll} label="Auto-scroll" />
          </div>
        </section>
      </div>
    </div>
  );
}

/** A source in the sidebar: its switch, line count, "only this" and a two-step clear. */
function SourceRow({ id, label, count, on, onToggle, onSolo, onClear, children }: {
  id: string;
  label: string;
  count: number;
  on: boolean;
  onToggle: () => void;
  onSolo: () => void;
  onClear: () => void;
  children?: React.ReactNode;
}) {
  const [arming, setArming] = useState(false);
  useEffect(() => {
    if (!arming) return;
    const t = setTimeout(() => setArming(false), 3000);
    return () => clearTimeout(t);
  }, [arming]);
  return (
    <div>
      <div className="group flex items-center gap-1.5 rounded-lg px-2 py-1 hover:bg-surface-2">
        <input type="checkbox" className="size-3.5 shrink-0 accent-(--accent)" checked={on} onChange={onToggle} aria-label={`Show ${label}`} />
        <button type="button" onClick={onSolo} title={`Show only ${label}`} className={clsx("min-w-0 flex-1 truncate text-left text-sm", on ? "text-fg" : "text-muted")}>
          {label}
        </button>
        <span className="shrink-0 text-[11px] tabular-nums text-muted group-hover:hidden">{count || ""}</span>
        <button
          type="button"
          onClick={() => (arming ? (setArming(false), onClear()) : setArming(true))}
          title={arming ? `Click again to clear ${label}'s logs` : `Clear ${label}'s logs`}
          aria-label={`Clear ${label}'s logs`}
          data-source={id}
          className={clsx("hidden shrink-0 items-center gap-1 rounded px-1 text-[11px] group-hover:flex", arming ? "bg-danger-soft text-danger-text" : "text-muted hover:text-danger-text")}
        >
          <Trash2 size={12} />
          {arming && "Clear?"}
        </button>
      </div>
      {children}
    </div>
  );
}

/** A selected line in full: its text, the lines around it from the same source, and a dump's value. */
function LineDetails({ line, lines, dumps, onClose, onCopy, onOpenFile }: {
  line: LogEntry;
  lines: LogEntry[];
  dumps: Dump[];
  onClose: () => void;
  onCopy: (text: string) => void;
  onOpenFile: (file: string) => void;
}) {
  const src = sourceOf(line);
  const same = lines.filter((l) => sourceOf(l) === src);
  const at = same.findIndex((l) => l.id === line.id);
  const around = at < 0 ? [] : same.slice(Math.max(0, at - 5), at + 6);
  const dumpId = line.service === "dumps" ? Number(/dump #(\d+)/.exec(line.message)?.[1]) : NaN;
  const dump = Number.isNaN(dumpId) ? undefined : dumps.find((d) => d.id === dumpId);

  return (
    <div className="flex max-h-[50%] min-h-32 shrink-0 flex-col border-t border-white/10 bg-terminal-2">
      <div className="flex shrink-0 items-center gap-2 border-b border-white/10 px-4 py-2 text-xs">
        <span className="text-slate-400">{stamp(line.time)}</span>
        <span className={clsx("rounded-md px-1 text-[10px]", line.service === "project" ? projectBadge : (badgeColors[line.service] ?? neutralBadge))}>{sourceLabel(src)}</span>
        <span className={clsx("capitalize", levels[line.level].text)}>{line.level}</span>
        <span className="flex-1" />
        {dump?.file && (
          <button type="button" onClick={() => onOpenFile(dump.file!)} className="flex items-center gap-1 rounded px-2 py-1 text-slate-300 hover:bg-white/10" title={dump.file}>
            <FileCode size={12} /> Open {dump.file.split(/[\\/]/).pop()}:{dump.line}
          </button>
        )}
        <button type="button" onClick={() => onCopy(line.message)} className="flex items-center gap-1 rounded px-2 py-1 text-slate-300 hover:bg-white/10">
          <Copy size={12} /> Copy
        </button>
        <button type="button" onClick={onClose} aria-label="Close details" className="rounded p-1 text-slate-400 hover:bg-white/10 hover:text-slate-100">
          <X size={14} />
        </button>
      </div>
      <div className="selectable min-h-0 flex-1 space-y-3 overflow-y-auto px-4 py-3 font-mono text-[12px] leading-5">
        {line.service === "dumps" ? (
          dump ? (
            <div className="space-y-1">
              <div className="text-[11px] text-slate-400">
                {[dump.project, dump.origin, dump.file && `${dump.file}:${dump.line}`].filter(Boolean).join(" · ")}
              </div>
              <DumpValue node={dump.tree} depth={0} />
            </div>
          ) : (
            <p className="font-sans text-xs text-slate-400">This dump's value isn't kept after LampRabit restarts, or after the dumps are cleared; its summary is: {line.message}</p>
          )
        ) : (
          <p className="break-all whitespace-pre-wrap text-slate-200">{line.message}</p>
        )}
        {line.service !== "dumps" && around.length > 1 && (
          <div>
            <div className="mb-1 font-sans text-[11px] font-semibold tracking-wide text-slate-500 uppercase">Around it in {sourceLabel(src)}</div>
            {around.map((l) => (
              <div key={l.id} className={clsx("truncate", l.id === line.id ? "text-slate-100" : "text-slate-500")} title={l.message}>
                <span className="mr-3">{stamp(l.time).slice(11)}</span>
                {l.message}
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

/** A dumped value as a tree, folded like Symfony's dumper: the first levels open. */
function DumpValue({ node, name, depth }: { node: DumpNode; name?: string; depth: number }) {
  const [open, setOpen] = useState(depth < 2);
  const children = node.children ?? [];
  const has = children.length > 0;
  const head = (() => {
    switch (node.kind) {
      case "string":
        return (
          <span className="text-emerald-300">
            "{node.value}
            {node.cut ? "…" : ""}"
          </span>
        );
      case "int":
      case "float":
        return <span className="text-sky-300">{node.value}</span>;
      case "bool":
      case "null":
        return <span className="text-violet-300">{node.value ?? "null"}</span>;
      case "array":
        return <span className="text-slate-400">array:{node.count ?? children.length} [</span>;
      case "object":
        return (
          <span>
            <span className="text-amber-300">{node.class}</span> <span className="text-slate-400">{"{"}</span>
          </span>
        );
      case "resource":
        return <span className="text-amber-300">resource ({node.class})</span>;
      default:
        return <span className="text-slate-300">{node.value}</span>;
    }
  })();
  const close = node.kind === "array" ? "]" : node.kind === "object" ? "}" : null;
  return (
    <div>
      <div className="flex items-start gap-1">
        {has ? (
          <button type="button" onClick={() => setOpen(!open)} className="mt-0.5 shrink-0 text-slate-500 hover:text-slate-200" aria-expanded={open} aria-label={open ? "Fold" : "Unfold"}>
            {open ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
          </button>
        ) : (
          <span className="w-3.25 shrink-0" />
        )}
        <span className="min-w-0 break-all">
          {name !== undefined && <span className="text-slate-400">{name} =&gt; </span>}
          {head}
          {close && (!has || !open) && (
            <span className="text-slate-400">
              {has ? " …" : ""}
              {close}
            </span>
          )}
        </span>
      </div>
      {has && open && (
        <div className="ml-1.5 border-l border-white/10 pl-3">
          {children.map((c, i) => (
            <DumpValue key={`${c.key}-${i}`} name={c.key} node={c.node} depth={depth + 1} />
          ))}
          {!!node.cut && <div className="text-slate-500">… {node.cut} more</div>}
          {close && <div className="text-slate-400">{close}</div>}
        </div>
      )}
    </div>
  );
}
