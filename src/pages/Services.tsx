import { useState, type ReactNode } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ExternalLink, FolderOpen, Hexagon, Play, Puzzle, RefreshCw, RotateCw, Share2, Square, Trash2 } from "lucide-react";
import ExtensionsDialog from "../components/ExtensionsDialog";
import { openPath, openUrl } from "@tauri-apps/plugin-opener";
import clsx from "clsx";
import {
  api,
  type InstalledRuntime,
  type Release,
  type Requirement,
  type Runtime,
} from "../lib/api";
import { infoFor, labelFor, meet, needsNote, unmet, useCatalog } from "../lib/catalog";
import RequirementsDialog from "../components/RequirementsDialog";
import { installKey, useAppStore } from "../lib/store";
import { toolUrl } from "../lib/tools";
import { useRuntimeUpdates } from "../lib/updates";
import { runtimeTiles, tileFor } from "../components/runtimes";
import { Button, Card, IconTile, PageHeader } from "../components/ui";
import NodeCard from "../components/NodeCard";
import ServiceSettings from "../components/ServiceSettings";
import RelatedPanel from "../components/RelatedPanel";
import BackupsPanel from "../components/BackupsPanel";
import { ActiveTile, AvailableList, InstalledTile, InstalledTiles, NoActiveTile, VersionsLayout } from "../components/versions";
import { TunnelsPanel } from "../components/Tunnels";

const versionKey = (v: string) => v.split(".").map(Number);
const newestFirst = (a: string, b: string) => {
  const [x, y] = [versionKey(a), versionKey(b)];
  for (let i = 0; i < Math.max(x.length, y.length); i++) {
    if ((x[i] ?? 0) !== (y[i] ?? 0)) return (y[i] ?? 0) - (x[i] ?? 0);
  }
  return 0;
};

/** A runtime id from the backend's catalog, or the two lists that aren't runtimes. */
type ServiceSelection = Runtime | "node" | "tunnels";

export default function Services() {
  const notify = useAppStore((s) => s.notify);
  const [selected, setSelected] = useState<ServiceSelection>("apache");
  const installed = useQuery({ queryKey: ["installed"], queryFn: api.installedRuntimes });
  const sites = useQuery({ queryKey: ["sites"], queryFn: api.listSites });
  const settings = useQuery({ queryKey: ["settings"], queryFn: api.getSettings });
  const ollamaElsewhere = useQuery({ queryKey: ["ollama-elsewhere"], queryFn: api.ollamaElsewhere, staleTime: 60_000 });
  const catalog = useCatalog();
  const groups = catalog.data?.groups ?? [];
  const find = (r: Runtime) => installed.data?.find((i) => i.runtime === r);
  const defaultDb = settings.data?.defaultDatabase ?? "mariadb";
  const selectedGroup = groups.find((g) => g.items.includes(selected));
  const isRuntime = selected !== "node" && selected !== "tunnels";
  const runtime = isRuntime ? selected as Runtime : null;

  let openUrl: string | undefined;
  let note: string | undefined;
  if (runtime === "mariadb" || runtime === "mysql") {
    if (find(runtime)?.active && runtime === defaultDb) note = "Default database: new projects and the mysql command use it.";
  } else if (runtime === "redis" && find("redis")?.active && !settings.data?.addToPath) {
    note = "redis-cli works in LampRabit's terminal. Enable Add LampRabit's commands to PATH in Settings → Terminal to use it anywhere.";
  } else if (runtime === "mongodb" && find("mongodb")?.active && !find("mongosh")?.active) {
    note = "Install mongosh too: the MongoDB shell, which also lets LampRabit stop MongoDB cleanly.";
  }
  if (runtime === "mailpit" || runtime === "phpmyadmin" || runtime === "adminer" || runtime === "phpredisadmin" || runtime === "phppgadmin" || runtime === "pgadmin" || runtime === "neo4j") {
    openUrl = toolUrl(runtime, sites.data, settings.data?.ports.mailUi, runtime === "neo4j" ? settings.data?.ports.neo4jHttp : settings.data?.ports.pgadmin);
  }
  if (runtime === "clickhouse") openUrl = settings.data ? `http://localhost:${settings.data.ports.clickhouseHttp}/play` : undefined;
  if (runtime === "meilisearch") openUrl = settings.data ? `http://localhost:${settings.data.ports.meilisearch}/` : undefined;
  if (runtime === "rustfs") openUrl = settings.data ? `http://localhost:${settings.data.ports.rustfsConsole}/` : undefined;
  if (runtime === "minio") openUrl = settings.data ? `http://localhost:${settings.data.ports.minioConsole}/` : undefined;
  if (runtime === "rabbitmq") openUrl = settings.data ? `http://localhost:${settings.data.ports.rabbitmqManagement}/` : undefined;
  if (runtime === "n8n") openUrl = toolUrl("n8n", sites.data, settings.data?.ports.mailUi, settings.data?.ports.n8n);


  const status = (item: ServiceSelection) => {
    if (item === "node") return "NVM managed";
    if (item === "tunnels") return "Public URLs";
    if (installed.isLoading) return "Checking…";
    const info = find(item);
    return info?.active ? `Active · ${info.active}` : info?.versions.length ? `${info.versions.length} installed` : "Not installed";
  };
  const icon = (item: ServiceSelection) =>
    item === "node" ? <Hexagon size={16} strokeWidth={1.75} /> : item === "tunnels" ? <Share2 size={16} strokeWidth={1.75} /> : tileFor(item).icon(16);
  const iconClass = (item: ServiceSelection) =>
    item === "node" ? "bg-lime-600/10 text-lime-600" : item === "tunnels" ? "bg-violet-500/10 text-violet-600" : tileFor(item).className;
  const itemLabel = (item: ServiceSelection) => (item === "tunnels" ? "Active tunnels" : labelFor(catalog.data, item));

  return (
    <>
      <PageHeader title="Services" subtitle="Choose a service to manage its versions, settings, and tools." />
      <div className="grid items-start gap-6 lg:grid-cols-[16rem_minmax(0,1fr)]">
        {/* Sized to the space under the header (title bar, page header, room for the tab bar), so the list scrolls on its own and the page only when a service's details are taller. */}
        <nav aria-label="Services" className="scrollbar-thin hidden lg:block lg:sticky lg:top-4 lg:max-h-[calc(100dvh-12rem)] lg:overflow-y-auto lg:overscroll-contain lg:pr-2">
          <div className="space-y-6">
            {groups.map((group) => (
              <section key={group.id} aria-labelledby={`service-group-${group.id}`}>
                <h2 id={`service-group-${group.id}`} className="px-3 pb-2 text-xs font-bold tracking-wider text-muted uppercase">{group.title}</h2>
                <ul className="space-y-0.5">
                  {(group.items as ServiceSelection[]).map((item) => (
                    <li key={item}>
                      <button
                        type="button"
                        onClick={() => setSelected(item)}
                        aria-current={selected === item ? "true" : undefined}
                        className={clsx(
                          "flex w-full items-center gap-2.5 rounded-r-lg border-l-2 px-2.5 py-2 text-left transition-colors",
                          selected === item ? "border-accent bg-surface-2 text-fg" : "border-transparent text-fg hover:bg-surface-2",
                        )}
                      >
                        <span className={clsx("flex size-7 shrink-0 items-center justify-center rounded-md", iconClass(item))}>{icon(item)}</span>
                        <span className="min-w-0 flex-1">
                          <span className="block truncate text-sm font-medium">{itemLabel(item)}</span>
                          <span className="block truncate text-[11px] text-muted">{status(item)}</span>
                        </span>
                      </button>
                    </li>
                  ))}
                </ul>
              </section>
            ))}
          </div>
        </nav>

        <section aria-label="Service details" className="min-w-0">
          <div className="mb-4 lg:hidden">
            <label className="block text-xs font-bold text-muted uppercase" htmlFor="service-picker">Service</label>
            <select id="service-picker" value={selected} onChange={(e) => setSelected(e.target.value as ServiceSelection)} className="mt-1 h-10 w-full rounded-lg border border-line bg-surface px-3 text-sm">
              {groups.map((group) => <optgroup key={group.id} label={group.title}>{(group.items as ServiceSelection[]).map((item) => <option key={item} value={item}>{itemLabel(item)}</option>)}</optgroup>)}
            </select>
          </div>
          <div className="mb-3">
            <p className="text-sm text-muted">{selectedGroup?.blurb}</p>
          </div>
          {runtime === "ollama" && ollamaElsewhere.data && !find("ollama")?.active ? (
            <SystemOllamaCard path={ollamaElsewhere.data} />
          ) : runtime ? (
            <RuntimeCard
              key={runtime}
              runtime={runtime}
              installed={find(runtime)}
              openUrl={openUrl}
              note={note}
              action={
                runtime === "compass" && find("compass")?.active ? (
                  <Button size="sm" variant="soft" onClick={() => api.openCompass().catch((e) => notify("error", String(e)))}>
                    <ExternalLink size={14} /> Open Compass
                  </Button>
                ) : undefined
              }
            />
          ) : selected === "node" ? <NodeCard /> : <TunnelsPanel />}
          {runtime && <RelatedPanel key={`related-${runtime}`} runtime={runtime} installed={installed.data} onSelect={setSelected} />}
          {runtime && <ServiceSettings key={`settings-${runtime}`} runtime={runtime} />}
          {/* SQLite databases are project files: they can be backed up without the shell installed. */}
          {(runtime === "sqlite" || ((runtime === "mariadb" || runtime === "mysql" || runtime === "postgres" || runtime === "mongodb") && find(runtime)?.active)) && <BackupsPanel key={`backups-${runtime}`} engine={runtime} />}
          {selected === "tunnels" && <p className="mt-3 text-xs text-muted">Share a project from Sites or use this panel to share any local port.</p>}
        </section>
      </div>
    </>
  );
}
/**
 * One runtime: the version in use as a large tile, other installed versions next to it,
 * and the versions available to download as a list.
 */
function RuntimeCard({
  runtime,
  installed,
  openUrl: url,
  note,
  action,
}: {
  runtime: Runtime;
  installed?: InstalledRuntime;
  /** Shown as an Open button once the tool is installed. */
  openUrl?: string;
  /** A requirement or tip shown under the title. */
  note?: string;
  /** An extra button in the header. */
  action?: ReactNode;
}) {
  const queryClient = useQueryClient();
  const notify = useAppStore((s) => s.notify);
  const installs = useAppStore((s) => s.installs);
  const [extOpen, setExtOpen] = useState(false);
  const [confirmRemove, setConfirmRemove] = useState(false);
  const catalogInfo = useCatalog();
  const info = infoFor(catalogInfo.data, runtime);
  const label = labelFor(catalogInfo.data, runtime);
  const paths = useQuery({ queryKey: ["paths"], queryFn: api.envPaths });
  // What the version in use (or the newest download) needs, for the note under the title.
  const needs = useQuery({ queryKey: ["requirements", runtime], queryFn: () => api.runtimeRequirements(runtime), enabled: (info?.needs.length ?? 0) > 0 });
  const [asking, setAsking] = useState<{ release: Release; requirements: Requirement[] } | null>(null);
  const os = paths.data?.os;
  const updates = useRuntimeUpdates().data?.filter((u) => u.runtime === runtime) ?? [];

  const catalog = useQuery({
    queryKey: ["catalog", runtime],
    queryFn: () => api.runtimeCatalog(runtime),
    staleTime: 10 * 60 * 1000,
  });

  // Catalogs are cached for a few hours; this asks for a fresh one.
  const recheck = useMutation({
    mutationFn: () => api.runtimeCatalog(runtime, true),
    onSuccess: (data) => queryClient.setQueryData(["catalog", runtime], data),
    onError: (e) => notify("error", String(e)),
  });
  const refresh = () => {
    for (const key of ["installed", "services", "settings", "requirements"]) queryClient.invalidateQueries({ queryKey: [key] });
  };
  const onError = (e: unknown) => notify("error", String(e));
  const install = useMutation({
    // Needs first (the versions that fit this release), then the release itself.
    mutationFn: async ({ release, requirements = [] }: { release: Release; requirements?: Requirement[] }) => {
      await meet(requirements);
      await api.installRuntime(release);
    },
    onSuccess: (_, { release, requirements = [] }) => {
      const extra = unmet(requirements).map((r) => r.label);
      notify("info", `${label} ${release.version} installed${extra.length ? `, with ${extra.join(" and ")}` : ""}`);
      setAsking(null);
      refresh();
      queryClient.invalidateQueries({ queryKey: ["runtime-updates"] });
    },
    onError: (e) => {
      // Whatever got installed before the failure is real; show it.
      refresh();
      onError(e);
    },
  });
  // Asks first when this version needs something that isn't in use yet.
  const checkAndInstall = useMutation({
    mutationFn: (release: Release) => (info?.needs.length ? api.runtimeRequirements(runtime, release.version) : Promise.resolve([])),
    onSuccess: (requirements, release) => {
      const missing = unmet(requirements);
      if (missing.length === 0) install.mutate({ release });
      else setAsking({ release, requirements: missing });
    },
    onError,
  });
  const activate = useMutation({ mutationFn: (v: string) => api.setActiveVersion(runtime, v), onSuccess: refresh, onError });
  const remove = useMutation({
    mutationFn: (v: string) => api.removeRuntime(runtime, v),
    onSuccess: refresh,
    onError,
  });

  const services = useQuery({ queryKey: ["services"], queryFn: api.listServices });
  const service = services.data?.find((s) => s.kind === runtime);
  const startService = useMutation({
    mutationFn: () => api.startService(service!.kind),
    onSuccess: refresh,
    onError: (e) => notify("error", String(e)),
  });
  const stopService = useMutation({
    mutationFn: () => api.stopService(service!.kind),
    onSuccess: refresh,
    onError: (e) => notify("error", String(e)),
  });
  const restartService = useMutation({
    mutationFn: () => api.restartService(service!.kind),
    onSuccess: refresh,
    onError: (e) => notify("error", String(e)),
  });
  const serviceBusy = startService.isPending || stopService.isPending || restartService.isPending;

  const have = [...(installed?.versions ?? [])].sort(newestFirst);
  const active = installed?.active ?? null;
  const others = have.filter((v) => v !== active);
  const available = (catalog.data ?? []).filter((r) => !have.includes(r.version)).sort((a, b) => newestFirst(a.version, b.version));
  const folder = (v: string) => (paths.data ? `${paths.data.root}${os === "windows" ? "\\" : "/"}bin${os === "windows" ? "\\" : "/"}${runtime}${os === "windows" ? "\\" : "/"}${v}` : null);
  const progressOf = (v: string) => installs[installKey(runtime, v)];
  const update = (v: string) => updates.find((u) => u.installed === v);
  const needsWarning = info?.unavailable ?? needsNote(needs.data);

  return (
    <Card className="overflow-hidden">
      <div className="flex flex-wrap items-center gap-4 border-b border-line px-5 py-4">
        <IconTile className={tileFor(runtime).className}>{tileFor(runtime).icon(22)}</IconTile>
        <div className="min-w-0 flex-1 basis-60">
          <h3 className="text-lg font-bold tracking-tight">{label}</h3>
          <p className="text-sm text-muted">
            {info ? `${info.blurb} · ${info.source}` : null}
          </p>
          {needsWarning && <p className={clsx("mt-1 text-xs font-medium", info?.unavailable ? "text-danger-text" : "text-warn")}>{needsWarning}</p>}
          {note && <p className="mt-1 text-xs font-medium text-accent-text">{note}</p>}
        </div>
        <div className="flex flex-wrap items-center gap-2">
          {action}
          {service && service.installed && (
            service.state === "running" ? (
              <div className="flex items-center gap-1.5">
                <span className="inline-flex items-center gap-1.5 rounded-full bg-accent-soft px-2.5 py-1 text-xs font-semibold text-accent-text">
                  <span className="size-1.5 rounded-full bg-ok" />
                  Running{service.port ? ` :${service.port}` : ""}
                </span>
                <Button size="sm" onClick={() => stopService.mutate()} disabled={serviceBusy} aria-label={`Stop ${label}`} title="Stop service">
                  <Square size={11} className="fill-current" /> Stop
                </Button>
                <Button size="sm" variant="ghost" onClick={() => restartService.mutate()} disabled={serviceBusy} aria-label={`Restart ${label}`} title="Restart service">
                  <RotateCw size={14} className={clsx(restartService.isPending && "animate-spin")} />
                </Button>
              </div>
            ) : (
              <div className="flex items-center gap-1.5">
                <span className="inline-flex items-center gap-1.5 rounded-full bg-surface-2 px-2.5 py-1 text-xs font-medium text-muted">
                  <span className="size-1.5 rounded-full bg-muted/60" />
                  Stopped
                </span>
                <Button size="sm" variant="primary" onClick={() => startService.mutate()} disabled={serviceBusy} aria-label={`Start ${label}`} title="Start service">
                  <Play size={12} /> Start
                </Button>
              </div>
            )
          )}
          {runtime === "php" && active && os === "windows" && (
            <Button size="sm" variant="soft" onClick={() => setExtOpen(true)}>
              <Puzzle size={14} /> Extensions
            </Button>
          )}
          {url && active && (
            <Button size="sm" variant="soft" onClick={() => openUrl(url).catch(onError)}>
              <ExternalLink size={14} /> Open
            </Button>
          )}
          <Button size="sm" variant="ghost" onClick={() => recheck.mutate()} disabled={catalog.isFetching || recheck.isPending} aria-label={`Check for new ${label} versions`} title="Check for new versions">
            <RefreshCw size={14} className={clsx((catalog.isFetching || recheck.isPending) && "animate-spin")} />
          </Button>
        </div>
      </div>
      {runtime === "php" && <ExtensionsDialog open={extOpen} onClose={() => setExtOpen(false)} />}
      {asking && (
        <RequirementsDialog
          name={`${label} ${asking.release.version}`}
          requirements={asking.requirements}
          busy={install.isPending}
          onConfirm={() => install.mutate(asking)}
          onClose={() => setAsking(null)}
        />
      )}

      <VersionsLayout
        active={
          active ? (
            <ActiveTile
              version={active}
              details={
                <>
                  {(catalog.data?.find((r) => r.version === active)?.label || have.length > 1) && (
                    <div className="flex flex-wrap gap-1.5">
                      {catalog.data?.find((r) => r.version === active)?.label && (
                        <span className="rounded-md bg-surface-2 px-1.5 py-0.5 font-mono text-fg">{catalog.data.find((r) => r.version === active)!.label}</span>
                      )}
                      {have.length > 1 && <span>{have.length} versions installed</span>}
                    </div>
                  )}
                  {update(active) && !progressOf(update(active)!.release.version) && (
                    <button
                      onClick={() => checkAndInstall.mutate(update(active)!.release)}
                      className="text-left font-semibold text-accent-text hover:underline"
                      title="Installs alongside; switch to it when you're ready"
                    >
                      ↑ {update(active)!.release.version} available, install it
                    </button>
                  )}
                </>
              }
            >
              {folder(active) && (
                <Button size="sm" onClick={() => openPath(folder(active)!).catch(onError)}>
                  <FolderOpen size={13} /> Folder
                </Button>
              )}
              {/* Removing the version in use asks twice; another installed version takes over. */}
              {confirmRemove ? (
                <>
                  <Button size="sm" variant="danger" onClick={() => remove.mutate(active, { onSettled: () => setConfirmRemove(false) })} disabled={remove.isPending} autoFocus>
                    {remove.isPending ? "Removing…" : `Remove ${active}`}
                  </Button>
                  <Button size="sm" variant="ghost" onClick={() => setConfirmRemove(false)} disabled={remove.isPending}>
                    Keep
                  </Button>
                </>
              ) : (
                <Button size="sm" variant="ghost" onClick={() => setConfirmRemove(true)} aria-label={`Remove ${label} ${active}`} title="Remove">
                  <Trash2 size={13} /> Remove
                </Button>
              )}
            </ActiveTile>
          ) : (
            <NoActiveTile text="Pick a version from the list to install it. The first one you install is used automatically." />
          )
        }
      >
        {others.length > 0 && (
          <InstalledTiles>
            {others.map((v) => (
              <InstalledTile
                key={v}
                version={v}
                label={catalog.data?.find((r) => r.version === v)?.label}
                name={`${label} ${v}`}
                busy={activate.isPending || remove.isPending}
                onUse={() => activate.mutate(v)}
                onRemove={() => remove.mutate(v)}
              />
            ))}
          </InstalledTiles>
        )}
        <AvailableList
          title={active ? "Available to download" : "Choose a version"}
          name={label}
          loading={catalog.isLoading}
          error={catalog.isError ? catalog.error : undefined}
          items={available.map((r) => {
            const p = progressOf(r.version);
            const pct = p?.total ? Math.round((p.downloaded / p.total) * 100) : null;
            return {
              version: r.version,
              label: r.label,
              progress: p ? (p.stage === "downloading" ? pct : null) : undefined,
              // npm, pip or rustup still working after the download.
              stage: p?.stage === "setup" ? "Setting up" : p?.stage,
            };
          })}
          onInstall={(v) => {
            const release = available.find((r) => r.version === v);
            if (release) checkAndInstall.mutate(release);
          }}
        />
      </VersionsLayout>
    </Card>
  );
}

/**
 * Ollama already installed on the computer: LampRabit uses it (and its models) rather
 * than offering a second copy to download.
 */
function SystemOllamaCard({ path }: { path: string }) {
  const notify = useAppStore((s) => s.notify);
  return (
    <Card className="overflow-hidden">
      <div className="flex flex-wrap items-center gap-4 px-5 py-4">
        <IconTile className={runtimeTiles.ollama.className}>{runtimeTiles.ollama.icon(22)}</IconTile>
        <div className="min-w-0 flex-1 basis-60">
          <h3 className="text-lg font-bold tracking-tight">Ollama</h3>
          <p className="text-sm text-muted">Using the Ollama installed on this computer, with the models you already have.</p>
          <p className="mt-1 truncate font-mono text-xs text-muted" title={path}>
            {path}
          </p>
        </div>
        <Button size="sm" onClick={() => openPath(path.replace(/[\\/][^\\/]+$/, "")).catch((e) => notify("error", String(e)))}>
          <FolderOpen size={14} /> Folder
        </Button>
      </div>
      <p className="border-t border-line px-5 py-3 text-xs text-muted">
        Start it from Home (or let the Ollama app run it); the ollama command in LampRabit's terminal talks to it. Update it with Ollama's own updater.
      </p>
    </Card>
  );
}
