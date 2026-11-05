import { useEffect, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ArrowRight, Check, ExternalLink, Plus, Trash2, X } from "lucide-react";
import clsx from "clsx";
import { api, type DomainTarget, type Settings, type Site } from "../lib/api";
import { useAppStore } from "../lib/store";
import DomainTargetPicker, { targetHint } from "./DomainTargetPicker";
import { Button, Card } from "./ui";

/** A subdomain being edited: what's typed before the project's domain, and where it goes. */
interface Row {
  label: string;
  target: DomainTarget;
}

const automatic: DomainTarget = { kind: "main" };
const LABEL = /^[a-z0-9]([a-z0-9-]*[a-z0-9])?(\.[a-z0-9]([a-z0-9-]*[a-z0-9])?)*$/;
const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);

/** The site's URL with another host: the same scheme and port as the main domain. */
const urlFor = (site: Site, host: string) => site.url.replace(site.domain, host);

/** "public/", or "the project folder". */
function folderOf(site: Site) {
  const rest = site.docroot.slice(site.path.length).replace(/^[\\/]+/, "").replace(/\\/g, "/");
  return rest ? `${rest}/` : "the project folder";
}

/** Why a row can't be saved yet, or null. */
function problem(row: Row, rows: Row[]): string | null {
  if (!row.label) return "Type the subdomain, like admin or api.";
  if (!LABEL.test(row.label)) return "Use lowercase letters, digits, dashes and dots.";
  if (rows.filter((r) => r.label === row.label).length > 1) return "This subdomain is listed twice.";
  if (row.target.kind === "port" && !(row.target.port >= 1 && row.target.port <= 65535)) return "Enter a port between 1 and 65535.";
  if (row.target.kind === "process" && !row.target.name) return "Pick a process.";
  return null;
}

/** ✓ and ✗ for a row with unsaved changes. */
function SaveDiscard({ busy, disabled, title, onSave, onDiscard }: { busy: boolean; disabled: boolean; title?: string; onSave: () => void; onDiscard: () => void }) {
  return (
    <span className="flex shrink-0 items-center gap-1">
      <button
        type="button"
        onClick={onSave}
        disabled={busy || disabled}
        title={title ?? "Save"}
        aria-label="Save"
        className="flex size-8 items-center justify-center rounded-lg bg-accent text-accent-fg hover:bg-accent-hover disabled:opacity-40"
      >
        <Check size={15} />
      </button>
      <button type="button" onClick={onDiscard} disabled={busy} title="Discard" aria-label="Discard" className="flex size-8 items-center justify-center rounded-lg text-muted hover:bg-surface-2 hover:text-fg">
        <X size={15} />
      </button>
    </span>
  );
}

const iconButton = "flex size-8 shrink-0 items-center justify-center rounded-lg text-muted hover:bg-surface-2 hover:text-fg";

/**
 * Where the project's main domain goes (automatically, or set by hand) and its subdomains
 * (admin.shop.test), one row each. A changed row gets ✓/✗ and is saved on its own; the
 * web server picks it up with a new certificate.
 */
export default function ProjectDomains({ site, settings }: { site: Site; settings?: Settings }) {
  const queryClient = useQueryClient();
  const notify = useAppStore((s) => s.notify);
  const own = settings?.sites[site.name];
  const suffix = `.${site.domain}`;
  const processes = (own?.processes ?? []).filter((p) => p.port != null).map((p) => p.name);

  const savedMain = own?.main ?? automatic;
  const savedRows: Row[] = (own?.domains ?? []).map((d) => ({ label: d.host.endsWith(suffix) ? d.host.slice(0, -suffix.length) : d.host, target: d.target }));
  const savedKey = JSON.stringify([savedMain, savedRows]);
  const [main, setMain] = useState<DomainTarget>(savedMain);
  const [rows, setRows] = useState<Row[]>(savedRows);
  // A change saved elsewhere (or a refetch) shows here, but never replaces an unsaved edit.
  const previous = useRef(savedKey);
  useEffect(() => {
    const [oldMain, oldRows] = JSON.parse(previous.current) as [DomainTarget, Row[]];
    const [m, r] = JSON.parse(savedKey) as [DomainTarget, Row[]];
    previous.current = savedKey;
    setMain((cur) => (same(cur, oldMain) ? m : cur));
    setRows((cur) => (same(cur, oldRows) ? r : cur));
  }, [savedKey]);

  const invalidate = () => {
    for (const key of ["settings", "sites", "services"]) queryClient.invalidateQueries({ queryKey: [key] });
  };
  const onError = (e: unknown) => notify("error", String(e));
  const saveMain = useMutation({ mutationFn: () => api.setSiteMain(site.name, main.kind === "main" ? null : main), onSuccess: invalidate, onError });
  const saveList = useMutation({
    mutationFn: (list: Row[]) => api.setSiteDomains(site.name, list.map((r) => ({ host: `${r.label}${suffix}`, target: r.target }))),
    onSuccess: invalidate,
    onError,
  });

  const dirtyAt = (i: number) => i >= savedRows.length || !same(rows[i], savedRows[i]);
  const update = (i: number, patch: Partial<Row>) => setRows(rows.map((r, j) => (j === i ? { ...r, ...patch } : r)));
  // Saves one row: the stored list with just this row changed (or added), so other rows' edits stay drafts.
  const saveRow = (i: number) => {
    const isNew = i >= savedRows.length;
    const list = isNew ? [...savedRows, rows[i]] : savedRows.map((r, j) => (j === i ? rows[i] : r));
    if (isNew && i !== savedRows.length) {
      // Keep local order matching the stored order: the saved row moves up to its stored place.
      const next = rows.filter((_, j) => j !== i);
      next.splice(savedRows.length, 0, rows[i]);
      setRows(next);
    }
    saveList.mutate(list);
  };
  const discardRow = (i: number) => (i >= savedRows.length ? setRows(rows.filter((_, j) => j !== i)) : update(i, savedRows[i]));
  const removeRow = (i: number) => {
    setRows(rows.filter((_, j) => j !== i));
    if (i < savedRows.length) saveList.mutate(savedRows.filter((_, j) => j !== i));
  };
  const open = (host: string) => openUrl(urlFor(site, host)).catch(onError);

  const mainDirty = !same(main, savedMain);
  const mainProblem = problem({ label: "main", target: main }, []);
  const problems = rows.map((r) => problem(r, rows));
  const now = site.route ? `port ${site.route.port}` : folderOf(site);
  const domainCell = "flex h-9 w-full shrink-0 items-center sm:w-72";

  // The port each row forwards to (none for files), and whether anything answers there.
  const portOf = (target: DomainTarget, isMain: boolean): number | null => {
    switch (target.kind) {
      case "port":
        return target.port || null;
      case "process":
        return (own?.processes ?? []).find((p) => p.name === target.name)?.port ?? null;
      case "main":
        return isMain ? (site.route?.port ?? null) : portOf(savedMain, true);
      default:
        return null;
    }
  };
  const ports = [...new Set([portOf(main, true), ...rows.map((r) => portOf(r.target, false))].filter((p): p is number => p != null))].sort();
  const listening = useQuery({
    queryKey: ["ports-listening", ports.join(",")],
    queryFn: () => api.portsListening(ports),
    enabled: ports.length > 0,
    refetchInterval: 5000,
  });
  const liveness = (port: number | null) => {
    if (port == null || !listening.data) return null;
    const up = listening.data.includes(port);
    return (
      <span className={clsx("flex shrink-0 items-center gap-1.5 text-xs", up ? "text-ok" : "text-warn")} title={up ? `Something answers on port ${port}` : `Nothing listens on port ${port}: the page will show an error until it does`}>
        <span className={clsx("size-1.5 rounded-full", up ? "bg-ok" : "bg-warn")} aria-hidden />
        {up ? `listening on :${port}` : `nothing on :${port}`}
      </span>
    );
  };

  return (
    <Card className="overflow-hidden">
      <div className="flex flex-wrap items-center gap-3 border-b border-line px-5 py-3">
        <div className="min-w-0 flex-1">
          <h3 className="text-sm font-bold">Domains</h3>
          <p className="text-xs text-muted">Where each domain goes. Change a row, then ✓ to save it.</p>
        </div>
        <Button size="sm" variant="soft" onClick={() => setRows([...rows, { label: "", target: { kind: "main" } }])}>
          <Plus size={14} /> Add subdomain
        </Button>
      </div>

      <ul className="divide-y divide-line">
        {/* The main domain */}
        <li className="px-5 py-3">
          <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
            <span className={clsx(domainCell, "gap-2")}>
              <span className="rounded bg-accent-soft px-1.5 py-0.5 text-[10px] font-bold tracking-wide text-accent-text uppercase">Main</span>
              <span className="truncate font-mono text-sm font-semibold">{site.domain}</span>
            </span>
            <ArrowRight size={14} className="hidden shrink-0 text-muted sm:block" aria-hidden />
            <span title={targetHint(main, true)}>
              <DomainTargetPicker project={site.name} value={main} onChange={setMain} processes={processes} automatic label={`Where ${site.domain} goes`} />
            </span>
            {portOf(main, true) != null ? liveness(portOf(main, true)) : !mainDirty && main.kind === "main" && <span className="truncate text-xs text-muted">now {now}</span>}
            <span className="flex-1" />
            {mainDirty ? (
              <SaveDiscard busy={saveMain.isPending} disabled={!!mainProblem} title={mainProblem ?? "Save"} onSave={() => saveMain.mutate()} onDiscard={() => setMain(savedMain)} />
            ) : (
              <button type="button" onClick={() => open(site.domain)} className={iconButton} aria-label={`Open ${site.domain}`} title={`Open ${site.domain}`}>
                <ExternalLink size={14} />
              </button>
            )}
          </div>
          {mainProblem && <p className="mt-1.5 text-xs text-danger-text">{mainProblem}</p>}
        </li>

        {/* Subdomains */}
        {rows.map((r, i) => {
          const host = `${r.label}${suffix}`;
          const dirty = dirtyAt(i);
          return (
            <li key={i} className="px-5 py-3">
              <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
                <span
                  className={clsx(
                    domainCell,
                    "overflow-hidden rounded-lg border bg-bg focus-within:ring-2 focus-within:ring-accent/25",
                    problems[i] && r.label ? "border-danger" : "border-line focus-within:border-accent",
                  )}
                >
                  <input
                    className="w-0 min-w-0 flex-1 bg-transparent px-2.5 text-right font-mono text-sm outline-none"
                    value={r.label}
                    placeholder="admin"
                    onChange={(e) => update(i, { label: e.target.value.toLowerCase().replace(/[^a-z0-9.-]/g, "") })}
                    aria-label={`Subdomain of ${site.domain}`}
                    autoFocus={i >= savedRows.length && !r.label}
                  />
                  <span className="flex h-full max-w-[60%] items-center truncate border-l border-line bg-surface-2 px-2.5 font-mono text-sm text-muted select-none" title={suffix}>
                    {suffix}
                  </span>
                </span>
                <ArrowRight size={14} className="hidden shrink-0 text-muted sm:block" aria-hidden />
                <span title={targetHint(r.target, false)}>
                  <DomainTargetPicker project={site.name} value={r.target} onChange={(target) => update(i, { target })} processes={processes} automatic={false} label={`Where ${host} goes`} />
                </span>
                {liveness(portOf(r.target, false))}
                <span className="flex-1" />
                {dirty ? (
                  <SaveDiscard busy={saveList.isPending} disabled={!!problems[i]} title={problems[i] ?? "Save"} onSave={() => saveRow(i)} onDiscard={() => discardRow(i)} />
                ) : (
                  <button type="button" onClick={() => open(host)} className={iconButton} aria-label={`Open ${host}`} title={`Open ${host}`}>
                    <ExternalLink size={14} />
                  </button>
                )}
                <button
                  type="button"
                  onClick={() => removeRow(i)}
                  disabled={saveList.isPending}
                  className={clsx(iconButton, "hover:text-danger-text")}
                  aria-label={`Remove ${r.label ? host : "this subdomain"}`}
                  title="Remove"
                >
                  <Trash2 size={14} />
                </button>
              </div>
              {problems[i] && r.label && <p className="mt-1.5 text-xs text-danger-text">{problems[i]}</p>}
            </li>
          );
        })}
      </ul>

      {rows.length === 0 && (
        <p className="border-t border-line px-5 py-3 text-xs text-muted">
          No subdomains yet: add one for an admin area, an API or a second app, like <span className="font-mono">admin{suffix}</span>.
          {processes.length === 0 && " Give a process a port (Commands tab) to point a domain at it."}
        </p>
      )}
    </Card>
  );
}
