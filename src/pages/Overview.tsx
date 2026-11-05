import { useEffect, useRef, useState, type ReactNode } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { openPath, openUrl } from "@tauri-apps/plugin-opener";
import {
  Activity as Pulse,
  AlertTriangle,
  Check,
  ChevronDown,
  ChevronRight,
  Copy,
  Database,
  DatabaseZap,
  ExternalLink,
  Feather,
  FileCog,
  FileText,
  FolderOpen,
  Globe,
  Keyboard,
  Layers,
  Layers3,
  Mail,
  Moon,
  Pencil,
  Play,
  RotateCcw,
  Plus,
  RotateCw,
  ScrollText,
  Server as ServerIcon,
  ShieldCheck,
  Gauge,
  Leaf,
  BarChart3,
  Network,
  Search,
  HardDrive,
  Bot,
  Workflow,
  Inbox,
  Flame,
  Cpu,
  Square,
  Sun,
  SunMedium,
  Table2,
  TerminalSquare,
  Trash2,
  Zap,
} from "lucide-react";
import clsx from "clsx";
import {
  api,
  runtimeLabels,
  serviceLabels,
  type Database as DatabaseKind,
  type PortStatus,
  type ServiceInfo,
  type ServiceKind,
  type Settings,
  type Site,
  type SitesInfo,
  type Runtime,
} from "../lib/api";
import { readPref, useAppStore, writePref, type Activity } from "../lib/store";
import { toolUrl } from "../lib/tools";
import { runtimeTiles } from "../components/runtimes";
import { formatClock, formatUptime, useNow } from "../lib/time";
import { randomQuote } from "../lib/quotes";
import { connection, copyText, envText } from "../lib/connections";
import { formatBytes, formatCpu, useUsage, useUsageSampler } from "../lib/usage";
import { isServer, primaryServer, serverEnabled, useStack } from "../lib/stack";
import { useHotkeys } from "../lib/shortcuts";
import { Button, Card, Pill, Segmented } from "../components/ui";
import Menu from "../components/Menu";
import ProblemPanel from "../components/ProblemPanel";
import ProfilesDialog from "../components/ProfilesDialog";
import Sparkline from "../components/Sparkline";
import { Kbd } from "../components/Dialog";

const greetings = [
  { until: 5, text: "Good evening", icon: Moon },
  { until: 12, text: "Good morning", icon: Sun },
  { until: 17, text: "Good afternoon", icon: SunMedium },
  { until: 24, text: "Good evening", icon: Moon },
];

const serviceMeta: Record<ServiceKind, { icon: typeof Feather; tile: string; blurb: string }> = {
  apache: { icon: Feather, tile: "bg-rose-500/10 text-rose-500", blurb: "Web server for your local sites" },
  nginx: { icon: ServerIcon, tile: "bg-emerald-600/10 text-emerald-600", blurb: "Fast web server for your local sites" },
  caddy: { icon: ShieldCheck, tile: runtimeTiles.caddy.className, blurb: "Web server with a short config file" },
  frankenphp: { icon: Flame, tile: runtimeTiles.frankenphp.className, blurb: "Web server with PHP built in" },
  tomcat: { icon: Cpu, tile: runtimeTiles.tomcat.className, blurb: "Runs your Java web apps" },
  mariadb: { icon: Database, tile: "bg-amber-600/12 text-amber-600", blurb: "Database server for your local sites" },
  postgres: { icon: Database, tile: runtimeTiles.postgres.className, blurb: "PostgreSQL database server" },
  pgadmin: { icon: Gauge, tile: runtimeTiles.pgadmin.className, blurb: "PostgreSQL admin in the browser" },
  mongodb: { icon: Leaf, tile: runtimeTiles.mongodb.className, blurb: "Document database" },
  clickhouse: { icon: BarChart3, tile: runtimeTiles.clickhouse.className, blurb: "Analytics database" },
  neo4j: { icon: Network, tile: runtimeTiles.neo4j.className, blurb: "Graph database" },
  meilisearch: { icon: Search, tile: runtimeTiles.meilisearch.className, blurb: "Search engine" },
  typesense: { icon: Search, tile: runtimeTiles.typesense.className, blurb: "Search engine" },
  rustfs: { icon: HardDrive, tile: runtimeTiles.rustfs.className, blurb: "S3 object storage" },
  ollama: { icon: Bot, tile: runtimeTiles.ollama.className, blurb: "Local AI models" },
  minio: { icon: HardDrive, tile: runtimeTiles.minio.className, blurb: "S3 object storage (unmaintained upstream)" },
  elasticsearch: { icon: Search, tile: runtimeTiles.elasticsearch.className, blurb: "Search engine" },
  opensearch: { icon: Search, tile: runtimeTiles.opensearch.className, blurb: "Search engine" },
  n8n: { icon: Workflow, tile: runtimeTiles.n8n.className, blurb: "Workflow automation" },
  rabbitmq: { icon: Inbox, tile: runtimeTiles.rabbitmq.className, blurb: "Message queue" },
  mysql: { icon: DatabaseZap, tile: runtimeTiles.mysql.className, blurb: "Oracle MySQL, next to MariaDB" },
  mailpit: { icon: Mail, tile: "bg-sky-500/10 text-sky-500", blurb: "Catches mail sent by your apps" },
  redis: { icon: Zap, tile: "bg-red-500/10 text-red-500", blurb: "Cache, sessions and queues" },
  memcached: { icon: Layers3, tile: "bg-teal-500/10 text-teal-600", blurb: "Memory cache for sessions and queries" },
};

/** Services listed only once installed; the core stack is always listed, with an Install button. */
const optional: ServiceKind[] = ["tomcat", "postgres", "pgadmin", "mongodb", "clickhouse", "neo4j", "meilisearch", "typesense", "rustfs", "ollama", "minio", "elasticsearch", "opensearch", "n8n", "rabbitmq", "mysql", "mailpit", "redis", "memcached"];

/**
 * Installed services, once there are any; before that, the core stack with Install buttons,
 * so a fresh setup has somewhere to start. Web servers only while enabled in Settings.
 */
const visible = (s: ServiceInfo, settings: Settings | undefined, anyInstalled: boolean) =>
  (s.installed || (!anyInstalled && !optional.includes(s.kind))) && serverEnabled(s.kind, settings?.webServers);

export default function Overview() {
  const now = useNow();
  const navigate = useAppStore((s) => s.navigate);
  const notify = useAppStore((s) => s.notify);
  const queryClient = useQueryClient();
  const connMode = useAppStore((s) => s.connMode);
  const setConnMode = useAppStore((s) => s.setConnMode);
  const openNewProject = useAppStore((s) => s.setNewProjectOpen);
  const services = useQuery({ queryKey: ["services"], queryFn: api.listServices });
  const sites = useQuery({ queryKey: ["sites"], queryFn: api.listSites, refetchInterval: 15_000 });
  const settings = useQuery({ queryKey: ["settings"], queryFn: api.getSettings });
  const ports = useQuery({ queryKey: ["port-status"], queryFn: api.portStatus, refetchInterval: 5000 });
  const stack = useStack();

  const anyInstalled = (services.data ?? []).some((s) => s.installed);
  const list = (services.data ?? []).filter((s) => visible(s, settings.data, anyInstalled));
  const [selected, setSelected] = useState<Set<ServiceKind>>(new Set());
  const [focus, setFocus] = useState(0);
  const [profilesDialog, setProfilesDialog] = useState<{ initial?: ServiceKind[]; profile?: Settings["profiles"][number] } | null>(null);
  const [quote] = useState(randomQuote);
  const rows = useRef<(HTMLElement | null)[]>([]);
  useUsageSampler(stack.running.length > 0);

  const hour = new Date(now).getHours();
  const greeting = greetings.find((g) => hour < g.until)!;
  const troubled = list.filter((s) => s.problem || (s.state === "running" && s.health?.status === "degraded"));
  const installedCount = stack.usable.length;

  useEffect(() => {
    if (stack.profile) setSelected(new Set(stack.profile.services));
  }, [stack.profile?.name, stack.profile?.services.join("|")]);

  const deleteProfile = useMutation({
    mutationFn: async (name: string) => {
      if (!settings.data) throw new Error("Settings are still loading");
      return api.saveSettings({ ...settings.data, profiles: settings.data.profiles.filter((p) => p.name !== name) });
    },
    onSuccess: (saved, name) => {
      queryClient.setQueryData(["settings"], saved);
      if (useAppStore.getState().activeProfile === name) useAppStore.getState().setActiveProfile(null);
      notify("info", `Deleted ${name} profile`);
    },
    onError: (e) => notify("error", String(e)),
  });

  const selectable = list.filter((s) => s.installed);
  const chosen = selectable.filter((s) => selected.has(s.kind)).map((s) => s.kind);
  const allSelected = selectable.length > 0 && chosen.length === selectable.length;
  const toggle = (kind: ServiceKind) =>
    setSelected((cur) => {
      const next = new Set(cur);
      if (next.has(kind)) next.delete(kind);
      else next.add(kind);
      return next;
    });

  const focusRow = (step: number) => {
    // The first press lands on the current row; later ones move from it.
    const onRow = rows.current.includes(document.activeElement as HTMLElement);
    const clamped = Math.max(0, Math.min(list.length - 1, onRow ? focus + step : focus));
    setFocus(clamped);
    rows.current[clamped]?.focus();
  };
  const current = list[focus];
  const copy = (text: string, what: string) =>
    navigator.clipboard.writeText(text).then(
      () => notify("info", `Copied ${what}`),
      (e) => notify("error", String(e)),
    );

  useHotkeys({
    arrowdown: () => focusRow(1),
    j: () => focusRow(1),
    arrowup: () => focusRow(-1),
    k: () => focusRow(-1),
    " ": () => current?.installed && toggle(current.kind),
    a: () => setSelected(allSelected ? new Set() : new Set(selectable.map((s) => s.kind))),
    enter: () => {
      if (!current?.installed) return;
      if (current.state === "running") stack.stop([current.kind]);
      else stack.start([current.kind]);
    },
    s: () => stack.start(chosen.length ? chosen : current?.installed ? [current.kind] : []),
    x: () => stack.stop(chosen.length ? chosen : current?.installed ? [current.kind] : []),
    r: () => current?.state === "running" && api.restartService(current.kind).catch((e) => notify("error", String(e))),
    o: () => current && openAction(current, sites.data, settings.data, navigate, notify)?.run(),
    c: () => current?.installed && copy(copyText(connection(current.kind, current.port), connMode), `${current.label} ${connMode === "env" ? ".env" : "URL"}`),
    e: () => setConnMode(connMode === "url" ? "env" : "url"),
    l: () => {
      if (current) useAppStore.getState().showLogs(current.kind);
    },
  });

  return (
    <>
      <div className="motion-stagger space-y-6">
        <header className="flex flex-wrap items-start justify-between gap-x-6 gap-y-4">
        <div>
          <h1 className="flex items-center gap-3 text-3xl font-extrabold tracking-tight">
            {greeting.text}, developer
            <greeting.icon size={28} strokeWidth={1.75} className="text-accent-text" aria-hidden />
          </h1>
          {/* The service list shows counts; the header only speaks up when something needs doing. */}
          <p className="mt-2 text-muted empty:hidden" aria-live="polite">
            {installedCount === 0 ? (
              "Install Apache, PHP and MariaDB from Services to get started."
            ) : (
              troubled.length > 0 && (
                <span className="font-medium text-danger-text">
                  {troubled.map((s) => s.label).join(" and ")} {troubled.length === 1 ? "needs" : "need"} attention
                </span>
              )
            )}
          </p>
          {installedCount > 0 && troubled.length === 0 && (
            <figure className="mt-2 max-w-2xl">
              <blockquote className="text-muted italic">“{quote.text}”</blockquote>
              <figcaption className="mt-0.5 text-xs text-muted/80">— {quote.author}</figcaption>
            </figure>
          )}
        </div>
        <div className="flex flex-wrap items-center gap-2">
          {installedCount === 0 ? (
            <Button variant="primary" onClick={() => navigate("services")}>
              Set up your stack
            </Button>
          ) : (
            <>
              {stack.up ? (
                <Button variant="primary" onClick={stack.restartRunning} disabled={stack.busy} title="Restart the running services (Shift+R)">
                  <RotateCcw size={14} /> Restart
                </Button>
              ) : (
                <Button variant="primary" onClick={stack.startStack} disabled={stack.busy} title="Shift+S">
                  <Play size={14} /> {stack.profile ? `Start ${stack.profile.name}` : "Start all"}
                </Button>
              )}
              <Button onClick={stack.stopAll} disabled={stack.busy || stack.stoppable.length === 0} title={stack.stoppable.length === 0 ? "Nothing LampRabit runs is running" : "Shift+X"}>
                <Square size={12} className="fill-current" /> Stop all
              </Button>
            </>
          )}
        </div>
      </header>

      <VcRuntimeBanner />

      <div className="grid items-start gap-6 xl:grid-cols-[minmax(0,1fr)_340px]">
        <section aria-labelledby="services-heading">
          <div className="mb-6">
            {installedCount > 0 && (
            <ProfileTiles
              profiles={settings.data?.profiles ?? []}
              active={stack.profile?.name ?? null}
              onSelect={(name) => {
                useAppStore.getState().setActiveProfile(name);
                const profileServices = settings.data?.profiles.find((p) => p.name === name)?.services;
                setSelected(new Set(profileServices ?? selectable.map((s) => s.kind)));
              }}
              onManage={() => setProfilesDialog({})}
              onEdit={(profile) => setProfilesDialog({ profile })}
              onDelete={(name) => {
                if (window.confirm(`Delete the “${name}” profile?`)) deleteProfile.mutate(name);
              }}
              onStartAll={stack.startAll}
              onStartProfile={stack.startProfile}
              isUp={stack.isUp}
              busy={stack.busy}
            />
          )}
          </div>
          <div className="mb-3 px-1">
            <div className="flex items-center gap-3">
              <h2 id="services-heading" className="text-base font-bold">Services</h2>
              <span className="text-sm text-muted">{stack.running.length} of {installedCount} running</span>
            </div>
            <div className="flex flex-wrap items-center gap-2.5">
              <label className="flex cursor-pointer items-center gap-2 text-sm text-muted select-none">
                <input
                  type="checkbox"
                  className="size-4 accent-(--accent)"
                  checked={allSelected}
                  ref={(el) => {
                    if (el) el.indeterminate = chosen.length > 0 && !allSelected;
                  }}
                  onChange={() => setSelected(allSelected ? new Set() : new Set(selectable.map((s) => s.kind)))}
                  disabled={selectable.length === 0}
                />
                Select all
              </label>
              <span className="h-5 w-px bg-line ml-auto" aria-hidden />
              <span className="text-sm text-muted tabular-nums" aria-live="polite">
                {chosen.length} selected
              </span>
              <Button size="sm" variant="ghost" disabled={chosen.length === 0} onClick={() => setProfilesDialog({ initial: chosen })}>
                <Layers size={13} /> Save as profile
              </Button>
            </div>
          </div>

          <ul className="space-y-2.5" aria-label="Services">
            {list.map((s, i) => (
              <li key={s.kind}>
                <ServiceRow
                  ref={(el) => {
                    rows.current[i] = el;
                  }}
                  service={s}
                  now={now}
                  sites={sites.data}
                  settings={settings.data}
                  ports={ports.data}
                  selected={selected.has(s.kind)}
                  onToggle={() => toggle(s.kind)}
                  tabIndex={i === focus ? 0 : -1}
                  onFocus={() => setFocus(i)}
                  inProfile={!!stack.profile?.services.includes(s.kind)}
                  profileActive={!!stack.profile}
                  onCopy={copy}
                />
              </li>
            ))}
          </ul>
          <p className="mt-3 flex items-center gap-1.5 px-1 text-xs text-muted">
            <Keyboard size={13} aria-hidden /> Press <Kbd>?</Kbd> for keyboard shortcuts: <Kbd>J</Kbd>
            <Kbd>K</Kbd> move, <Kbd>Space</Kbd> select, <Kbd>Enter</Kbd> start or stop.
          </p>
        </section>

        <aside className="space-y-4" aria-label="Shortcuts and connections">
          <QuickShortcuts sites={sites.data} services={services.data ?? []} settings={settings.data} />
          <ConnectPanel services={stack.usable} onCopy={copy} />
          <RecentProjects sites={sites.data?.sites ?? []} onNew={() => openNewProject(true)} />
          <RecentActivity />
        </aside>
      </div>
    </div>

    {profilesDialog && <ProfilesDialog initial={profilesDialog.initial} profile={profilesDialog.profile} onClose={() => setProfilesDialog(null)} />}
  </>
  );
}

function ProfileTiles({
  profiles,
  active,
  onSelect,
  onManage,
  onEdit,
  onDelete,
  onStartAll,
  onStartProfile,
  isUp,
  busy,
}: {
  profiles: Settings["profiles"];
  active: string | null;
  onSelect: (name: string | null) => void;
  onManage: () => void;
  onEdit: (profile: Settings["profiles"][number]) => void;
  onDelete: (name: string) => void;
  onStartAll: () => void;
  onStartProfile: (services: ServiceKind[]) => void;
  /** Whether a profile's services (all services for []) are already running. */
  isUp: (services: ServiceKind[]) => boolean;
  busy: boolean;
}) {
  const profileList = [{ name: null, label: "All services", services: [] as ServiceKind[] }, ...profiles.map((p) => ({ ...p, label: p.name }))];

  return (
    <section aria-labelledby="profiles-heading">
      <div className="mb-3 flex items-center justify-between px-1">
        <div>
          <h2 id="profiles-heading" className="text-base font-bold">Stack Profiles</h2>
          <p className="text-xs text-muted">Select a profile to filter your stack, or start its services directly.</p>
        </div>
      </div>
      <div className="grid grid-cols-[repeat(auto-fill,minmax(250px,1fr))] gap-3" role="radiogroup" aria-label="Service profiles">
        {profileList.map((profile) => {
          const selected = profile.name === active;
          const up = isUp(profile.services);
          const shownServices = profile.services.slice(0, 3);
          const extraCount = profile.services.length - shownServices.length;

          return (
            <article
              key={profile.name ?? "all"}
              onClick={() => onSelect(profile.name)}
              className={clsx(
                "group relative flex min-h-[142px] cursor-pointer flex-col justify-between rounded-2xl border p-4 shadow-card transition-all",
                selected
                  ? "border-accent/70 bg-accent-soft/30 ring-1 ring-accent/30"
                  : "border-line bg-surface hover:border-accent/40 hover:shadow-md",
              )}
            >
              {/* Card Header: Icon + Title + Status/Menu */}
              <div className="flex items-start justify-between gap-2">
                <div className="flex min-w-0 items-center gap-2.5">
                  <span
                    className={clsx(
                      "flex size-8 shrink-0 items-center justify-center rounded-xl transition-colors",
                      selected ? "bg-accent text-accent-fg" : "bg-surface-2 text-muted group-hover:text-fg",
                    )}
                  >
                    <Layers size={16} />
                  </span>
                  <div className="min-w-0">
                    <span className="block truncate text-sm font-bold text-fg">
                      {profile.name ?? profile.label}
                    </span>
                    <span className="block text-[11px] text-muted">
                      {profile.name ? `${profile.services.length} ${profile.services.length === 1 ? "service" : "services"}` : "Full stack"}
                    </span>
                  </div>
                </div>

                <div className="flex shrink-0 items-center gap-1" onClick={(e) => e.stopPropagation()}>
                  {up && (
                    <span className="inline-flex items-center gap-1 rounded-full bg-ok/10 px-2 py-0.5 text-[11px] font-semibold text-ok">
                      <span className="size-1.5 rounded-full bg-ok" />
                      Running
                    </span>
                  )}
                  {profile.name && (
                    <Menu
                      label={`${profile.name} profile actions`}
                      plain
                      items={[
                        { label: "Edit profile", icon: <Pencil size={14} />, onSelect: () => onEdit(profile) },
                        { label: "Delete profile", icon: <Trash2 size={14} />, onSelect: () => onDelete(profile.name) },
                      ]}
                    />
                  )}
                </div>
              </div>

              {/* Service Badges */}
              <div className="my-2.5">
                {profile.services.length > 0 ? (
                  <div className="flex flex-wrap gap-1">
                    {shownServices.map((kind) => (
                      <span
                        key={kind}
                        className="rounded-md border border-line/60 bg-surface-2/80 px-1.5 py-0.5 text-[11px] font-medium text-fg"
                      >
                        {serviceLabels[kind]}
                      </span>
                    ))}
                    {extraCount > 0 && (
                      <span className="rounded-md border border-line/60 bg-surface-2/80 px-1.5 py-0.5 text-[11px] font-medium text-muted">
                        +{extraCount} more
                      </span>
                    )}
                  </div>
                ) : (
                  <span className="text-xs text-muted">Runs all installed services together</span>
                )}
              </div>

              {/* Card Footer: Selection Indicator & Action */}
              <div className="flex items-center justify-between border-t border-line/50 pt-2.5">
                <span className="text-xs">
                  {selected ? (
                    <span className="inline-flex items-center gap-1 font-semibold text-accent-text">
                      <Check size={12} strokeWidth={2.5} /> Active
                    </span>
                  ) : (
                    <span className="text-muted group-hover:text-fg">Click to select</span>
                  )}
                </span>

                <Button
                  size="sm"
                  variant={selected && !up ? "primary" : "secondary"}
                  disabled={busy || up}
                  onClick={(e) => {
                    e.stopPropagation();
                    if (profile.name) onStartProfile(profile.services);
                    else onStartAll();
                  }}
                  className="h-7 px-2.5 text-xs"
                >
                  {up ? (
                    <>
                      <Check size={12} /> Running
                    </>
                  ) : (
                    <>
                      <Play size={11} /> Start
                    </>
                  )}
                </Button>
              </div>
            </article>
          );
        })}

        <button
          type="button"
          onClick={onManage}
          className="flex min-h-[142px] flex-col items-center justify-center gap-2 rounded-2xl border border-dashed border-line bg-surface/40 p-4 text-sm font-semibold text-muted transition-all hover:border-accent/60 hover:bg-accent-soft/30 hover:text-accent-text focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent"
        >
          <span className="flex size-9 items-center justify-center rounded-full border border-dashed border-current">
            <Plus size={16} />
          </span>
          Add custom profile
        </button>
      </div>
    </section>
  );
}

/** What "Open" does for a service: its site, inbox or database tool. */
/**
 * The installed web tools that open a database server, each with the URL that lands on
 * that server: phpMyAdmin numbers its servers (default database first, as configs.rs
 * writes them), Adminer takes the address, phpPgAdmin and pgAdmin have LampRabit's
 * PostgreSQL in their own lists.
 */
function dbTools(service: ServiceInfo, sites: SitesInfo | undefined, settings: Settings | undefined): { tool: Runtime; url: string }[] {
  const mailUi = settings?.ports.mailUi;
  const at = (tool: Runtime, url: string | undefined, query = "") => (url ? [{ tool, url: `${url}${query}` }] : []);
  if (service.kind === "mariadb" || service.kind === "mysql") {
    const order = (["mariadb", "mysql"] as const)
      .filter((d) => settings?.versions[d])
      .sort((a, b) => Number(b === settings?.defaultDatabase) - Number(a === settings?.defaultDatabase));
    return [
      ...at("phpmyadmin", toolUrl("phpmyadmin", sites, mailUi), `?server=${Math.max(1, order.indexOf(service.kind) + 1)}`),
      ...at("adminer", toolUrl("adminer", sites, mailUi), `?server=127.0.0.1:${service.port}&username=root`),
    ];
  }
  if (service.kind === "postgres") {
    return [
      ...at("adminer", toolUrl("adminer", sites, mailUi), `?pgsql=127.0.0.1:${service.port}&username=root&db=postgres`),
      ...at("phppgadmin", toolUrl("phppgadmin", sites, mailUi)),
      ...at("pgadmin", toolUrl("pgadmin", sites, mailUi, settings?.ports.pgadmin)),
    ];
  }
  return [];
}

/** The tool a database's Open button uses: the one picked last on this device, else the first installed. */
const preferredTool = (kind: ServiceKind, tools: { tool: Runtime; url: string }[]) =>
  tools.find((t) => t.tool === readPref(`dbTool.${kind}`)) ?? tools[0];

function openAction(
  service: ServiceInfo,
  sites: SitesInfo | undefined,
  settings: Settings | undefined,
  navigate: (p: "services") => void,
  notify: (tone: "error", text: string) => void,
): { label: string; icon: typeof ExternalLink; run: () => void; always?: boolean } | null {
  const onError = (e: unknown) => notify("error", String(e));
  const mailUi = settings?.ports.mailUi;
  if (isServer(service.kind)) {
    return { label: "Open in browser", icon: ExternalLink, run: () => openUrl(connection(service.kind, service.port).url + "/").catch(onError) };
  }
  if (service.kind === "mariadb" || service.kind === "mysql" || service.kind === "postgres") {
    // The tool picked last for this database (the ⋯ menu lists the others).
    const tool = preferredTool(service.kind, dbTools(service, sites, settings));
    if (!tool) {
      const suggest = service.kind === "postgres" ? "adminer" : "phpmyadmin";
      return { label: `Install ${runtimeLabels[suggest]}`, icon: Table2, run: () => navigate("services"), always: true };
    }
    return { label: `Open ${runtimeLabels[tool.tool]}`, icon: Table2, run: () => openUrl(tool.url).catch(onError) };
  }
  if (service.kind === "mongodb") {
    return settings?.versions.compass
      ? { label: "Open Compass", icon: Table2, run: () => api.openCompass().catch(onError) }
      : { label: "Install Compass", icon: Table2, run: () => navigate("services"), always: true };
  }
  if (service.kind === "meilisearch") {
    return { label: "Open search preview", icon: ExternalLink, run: () => openUrl(`http://localhost:${service.port}/`).catch(onError) };
  }
  if (service.kind === "minio") {
    return { label: "Open console", icon: ExternalLink, run: () => openUrl(`http://localhost:${settings?.ports.minioConsole ?? 9111}/`).catch(onError) };
  }
  if (service.kind === "rabbitmq") {
    return { label: "Open management console", icon: ExternalLink, run: () => openUrl(`http://localhost:${settings?.ports.rabbitmqManagement ?? 15672}/`).catch(onError) };
  }
  if (service.kind === "n8n") {
    const url = toolUrl("n8n", sites, mailUi, service.port);
    return url ? { label: "Open n8n", icon: ExternalLink, run: () => openUrl(url).catch(onError) } : null;
  }
  if (service.kind === "rustfs") {
    return { label: "Open console", icon: ExternalLink, run: () => openUrl(`http://localhost:${settings?.ports.rustfsConsole ?? 9101}/`).catch(onError) };
  }
  if (service.kind === "clickhouse") {
    return { label: "Open console", icon: Table2, run: () => openUrl(`http://localhost:${service.port}/play`).catch(onError) };
  }
  if (service.kind === "neo4j") {
    const url = toolUrl("neo4j", sites, mailUi, service.port);
    return url ? { label: "Open Neo4j Browser", icon: ExternalLink, run: () => openUrl(url).catch(onError) } : null;
  }
  if (service.kind === "pgadmin") {
    const url = toolUrl("pgadmin", sites, mailUi, settings?.ports.pgadmin);
    return url ? { label: "Open pgAdmin", icon: ExternalLink, run: () => openUrl(url).catch(onError) } : null;
  }
  if (service.kind === "redis") {
    const url = toolUrl("phpredisadmin", sites, mailUi);
    return url
      ? { label: "Open phpRedisAdmin", icon: Table2, run: () => openUrl(url).catch(onError) }
      : { label: "Install phpRedisAdmin", icon: Table2, run: () => navigate("services"), always: true };
  }
  if (service.kind === "mailpit") {
    return { label: "Open inbox", icon: ExternalLink, run: () => openUrl(toolUrl("mailpit", sites, mailUi) ?? "http://localhost:8025/").catch(onError) };
  }
  return null;
}

/** A port another program holds, found before the service is started. */
function conflictFor(service: ServiceInfo, ports?: PortStatus[]) {
  if (service.state === "running" || service.state === "starting") return null;
  const taken = ports?.find((p) => p.port === service.port && !p.free && !p.usedByService);
  return taken ?? null;
}

function ServiceRow({
  ref,
  service,
  now,
  sites,
  settings,
  ports,
  selected,
  onToggle,
  tabIndex,
  onFocus,
  inProfile,
  profileActive,
  onCopy,
}: {
  ref: (el: HTMLElement | null) => void;
  service: ServiceInfo;
  now: number;
  sites?: SitesInfo;
  settings?: Settings;
  ports?: PortStatus[];
  selected: boolean;
  onToggle: () => void;
  tabIndex: number;
  onFocus: () => void;
  inProfile?: boolean;
  profileActive?: boolean;
  onCopy: (text: string, what: string) => void;
}) {
  const notify = useAppStore((s) => s.notify);
  const navigate = useAppStore((s) => s.navigate);
  const showLogs = useAppStore((s) => s.showLogs);
  const connMode = useAppStore((s) => s.connMode);
  const usage = useUsage((s) => s.series[service.kind]);
  const [expanded, setExpanded] = useState(false);
  // PHP's workers belong to no one service; the web servers show them, since that's
  // where a slow page's time goes.
  const php = useUsage((s) => (isServer(service.kind) ? s.latest.php : undefined));
  const onError = (e: unknown) => notify("error", String(e));
  // A failed start shows on the row itself (ProblemPanel), so no toast for it.
  const start = useMutation({ mutationFn: () => api.startService(service.kind), onError: () => { } });
  const stop = useMutation({ mutationFn: () => api.stopService(service.kind), onError });
  const restart = useMutation({ mutationFn: () => api.restartService(service.kind), onError });
  const meta = serviceMeta[service.kind];
  const running = service.state === "running";
  // Spawned and waiting to answer (Elasticsearch, n8n… take up to a minute): it can be
  // stopped already.
  const loading = service.state === "starting" && service.pid != null;
  const busy = service.state === "starting" || service.state === "stopping" || restart.isPending || start.isPending || stop.isPending;
  const degraded = running && service.health?.status === "degraded";
  const conflict = conflictFor(service, ports);
  const hasIssues = !!(conflict || degraded || (service.problem && !running));
  // Re-renders the row when a different tool is picked from the menu (the choice itself
  // lives in the device's preferences).
  const [, setToolPick] = useState<string | null>(null);
  const action = openAction(service, sites, settings, navigate, notify);
  const conn = connection(service.kind, service.port);
  const connValue = copyText(conn, connMode);

  const status = !service.installed
    ? { tone: "muted" as const, text: "Not installed" }
    : degraded
      ? { tone: "danger" as const, text: "Not responding" }
      : running
        ? { tone: "ok" as const, text: service.health?.status === "healthy" ? "Healthy" : "Running" }
        : service.problem
          ? { tone: "danger" as const, text: "Failed" }
          : service.state === "starting" || service.state === "stopping"
            ? { tone: "warn" as const, text: service.state === "starting" ? "Starting" : "Stopping" }
            : { tone: "muted" as const, text: "Stopped" };

  const lastCpu = usage?.cpu?.length ? usage.cpu[usage.cpu.length - 1] : undefined;
  const lastMem = usage?.memory?.length ? usage.memory[usage.memory.length - 1] : undefined;

  // Every installed web tool for this database; the one picked becomes its Open button's.
  const tools = dbTools(service, sites, settings);
  const toolMenu =
    tools.length > 1
      ? tools.map((t) => ({
          label: `Open in ${runtimeLabels[t.tool]}${preferredTool(service.kind, tools)?.tool === t.tool ? " (default)" : ""}`,
          icon: <Table2 size={15} />,
          onSelect: () => {
            writePref(`dbTool.${service.kind}`, t.tool);
            setToolPick(t.tool);
            openUrl(t.url).catch(onError);
          },
        }))
      : [];
  const dbMenu = [
    ...toolMenu,
    ...(service.kind === "mariadb" || service.kind === "mysql"
      ? [{ label: "Open command-line client", icon: <TerminalSquare size={15} />, onSelect: () => api.openDbClient(service.kind as DatabaseKind).catch(onError) }]
      : []),
  ];

  return (
    <article
      ref={ref}
      tabIndex={tabIndex}
      onFocus={(e) => e.target === e.currentTarget && onFocus()}
      onMouseDown={onFocus}
      aria-label={`${service.label}, ${status.text}${service.installed ? `, port ${service.port}` : ""}`}
      className={clsx(
        "rounded-2xl border bg-surface transition-all duration-150 outline-none focus-visible:ring-2 focus-visible:ring-accent/50",
        selected ? "border-accent/50 bg-accent-soft/40 shadow-sm" : running ? "border-accent/25 shadow-card" : "border-line shadow-card",
        !service.installed && "border-dashed",
        profileActive && !inProfile && "opacity-75",
      )}
    >
      {/* Compact Main Row: the name selects the service, the rest of the row opens its details. */}
      <div
        className={clsx("flex items-center gap-3 px-4 py-3", service.installed && "cursor-pointer")}
        onClick={(e) => {
          // Controls keep their own clicks, and selecting text isn't a click.
          if ((e.target as HTMLElement).closest("button, a, input, select, label, [role=menu], [role=menuitem]")) return;
          if (window.getSelection()?.toString()) return;
          if (service.installed) setExpanded(!expanded);
        }}
      >
        <input
          type="checkbox"
          className="size-4 shrink-0 accent-(--accent) disabled:opacity-30 cursor-pointer"
          checked={selected}
          onChange={onToggle}
          disabled={!service.installed}
          aria-label={`Select ${service.label}`}
          tabIndex={-1}
        />
        <div className={clsx("flex size-9 shrink-0 items-center justify-center rounded-xl", meta.tile)} aria-hidden>
          <meta.icon size={18} strokeWidth={1.75} />
        </div>
        <div className="min-w-0 flex-1 flex flex-wrap items-center gap-2">
          <button
            type="button"
            onClick={onToggle}
            disabled={!service.installed}
            tabIndex={-1}
            title={service.installed ? (selected ? `Unselect ${service.label}` : `Select ${service.label}`) : undefined}
            className="cursor-pointer rounded text-[14px] font-bold text-fg hover:text-accent-text disabled:cursor-default disabled:hover:text-fg"
          >
            {service.label}
          </button>
          <Pill tone={status.tone}>{status.text}</Pill>
          {service.installed && service.port > 0 && (
            <span className="font-mono text-xs px-1.5 py-0.5 rounded-md bg-surface-2 border border-line text-muted">
              :{service.port}
            </span>
          )}
          {service.version && <span className="font-mono text-xs text-muted/80 hidden sm:inline">v{service.version}</span>}
          {running && lastCpu !== undefined && lastMem !== undefined ? (
            <span className="hidden md:inline-flex items-center gap-1.5 font-mono text-[11px] text-muted bg-surface-2/60 px-2 py-0.5 rounded border border-line/50">
              <span>CPU <strong className="text-fg font-medium">{formatCpu(lastCpu)}</strong></span>
              <span>·</span>
              <span>RAM <strong className="text-fg font-medium">{formatBytes(lastMem)}</strong></span>
            </span>
          ) : (
            <span className="hidden lg:inline text-xs text-muted truncate max-w-xs">{meta.blurb}</span>
          )}
        </div>

        {/* Action Controls */}
        <div className="flex shrink-0 items-center gap-1.5">
          {!service.installed ? (
            <Button size="sm" variant="primary" onClick={() => navigate("services")}>
              Install
            </Button>
          ) : service.external ? (
            <span className="px-2 text-xs text-muted" title="Started by the Ollama app; quit it there to stop it.">
              Ollama app
            </span>
          ) : running || loading || service.state === "stopping" ? (
            <>
              <Button size="sm" onClick={() => stop.mutate()} disabled={busy && !loading} aria-label={`Stop ${service.label}`}>
                <Square size={11} className="fill-current" /> Stop
              </Button>
              <Button size="sm" variant="ghost" onClick={() => restart.mutate()} disabled={busy} aria-label={`Restart ${service.label}`} title="Restart (R)">
                <RotateCw size={14} className={clsx(restart.isPending && "animate-spin")} />
              </Button>
            </>
          ) : (
            <Button size="sm" variant="soft" onClick={() => start.mutate()} disabled={busy} aria-label={`Start ${service.label}`}>
              <Play size={12} /> Start
            </Button>
          )}

          {action && service.installed && (
            <Button
              size="sm"
              variant="ghost"
              onClick={action.run}
              disabled={!running && !action.always}
              aria-label={action.label}
              title={`${action.label} (O)`}
            >
              <action.icon size={14} />
            </Button>
          )}

          {service.installed && (
            <Button
              size="sm"
              variant="ghost"
              onClick={() => onCopy(connValue, `${service.label} ${connMode === "env" ? ".env" : "URL"}`)}
              title={connMode === "env" ? envText(conn.env) : `Copy ${conn.url} (C)`}
              aria-label={`Copy ${service.label} connection`}
            >
              <Copy size={13} className="text-muted" />
            </Button>
          )}

          <Menu
            items={[
              ...dbMenu,
              {
                label: service.kind === "mailpit" ? "Open data folder" : "Open config file",
                icon: <FileCog size={15} />,
                onSelect: () => openPath(service.configPath).catch(() => notify("error", "Created on first start")),
              },
              {
                label: "Open error log",
                icon: <FileText size={15} />,
                onSelect: () => openPath(service.logPath).catch(() => notify("error", "No log file yet")),
              },
              { label: "View logs", icon: <ScrollText size={15} />, onSelect: () => showLogs(service.kind) },
            ]}
          />

          {service.installed && (
            <button
              type="button"
              onClick={() => setExpanded(!expanded)}
              aria-expanded={expanded}
              aria-label={expanded ? `Collapse ${service.label} details` : `Expand ${service.label} details`}
              className={clsx(
                "p-1.5 rounded-lg text-muted hover:text-fg hover:bg-surface-2 transition-transform duration-200 cursor-pointer",
                (expanded || hasIssues) && "rotate-180 text-accent-text"
              )}
            >
              <ChevronDown size={15} />
            </button>
          )}
        </div>
      </div>

      {/* Expandable Details Drawer */}
      {service.installed && (expanded || hasIssues) && (
        <div className="border-t border-line/60 bg-surface-2/30 px-4 py-3 rounded-b-2xl space-y-3">
          <p className="text-xs text-muted">{meta.blurb}</p>

          <div className="flex flex-wrap items-end gap-x-6 gap-y-3">
            <Stat label="Port" value={service.port} live={running} />
            <Stat label="PID" value={service.pid ?? "—"} />
            <Stat label="Uptime" value={running && service.startedAt ? formatUptime(now - service.startedAt) : "—"} />
            {running && (
              <>
                <Sparkline label="CPU" values={usage?.cpu ?? []} format={formatCpu} floor={10} note="100% is one full CPU core" />
                <Sparkline label="RAM" values={usage?.memory ?? []} format={formatBytes} floor={64 * 1024 ** 2} />
                {php && (
                  <span title={`All PHP workers: ${php.processes} process${php.processes === 1 ? "" : "es"}`}>
                    <Stat label="PHP" value={`${formatCpu(php.cpu)} · ${formatBytes(php.memory)}`} />
                  </span>
                )}
              </>
            )}
            <button
              onClick={() => onCopy(connValue, `${service.label} ${connMode === "env" ? ".env" : "URL"}`)}
              className="group ml-auto flex max-w-full min-w-0 items-center gap-2 rounded-lg border border-line bg-surface px-2.5 py-1.5 text-left font-mono text-xs text-fg hover:border-accent/40 shadow-xs cursor-pointer"
              title={connMode === "env" ? envText(conn.env) : `Copy ${conn.url} (C)`}
              aria-label={`Copy ${service.label} connection ${connMode === "env" ? "as .env lines" : conn.url}`}
            >
              <span className="truncate">
                {connMode === "url" || conn.env.length === 0 ? conn.url : conn.env.length === 1 ? envText(conn.env) : `${conn.env[0][0]}=… +${conn.env.length - 1} more`}
              </span>
              <Copy size={13} className="shrink-0 text-muted group-hover:text-accent-text" />
            </button>
          </div>

          {conflict && (
            <div className="flex items-start gap-2.5 rounded-xl border border-warn/30 bg-warn/10 p-3 text-sm" role="status">
              <AlertTriangle size={16} className="mt-0.5 shrink-0 text-warn" aria-hidden />
              <div>
                <span className="font-medium">Port {conflict.port} is in use</span>
                <span className="text-muted">
                  {" "}
                  by {conflict.owner ? `${conflict.owner.name} (PID ${conflict.owner.pid})` : "another program"}, so {service.label} can't start on it.
                  Stop that program, or pick another port in Settings → Ports.
                </span>
              </div>
            </div>
          )}
          {degraded && (
            <div className="flex items-start gap-2.5 rounded-xl border border-warn/30 bg-warn/10 p-3 text-sm" role="status">
              <Pulse size={16} className="mt-0.5 shrink-0 text-warn" aria-hidden />
              <div>
                <span className="font-medium">The process is running but not answering.</span>{" "}
                <span className="text-muted">{service.health?.detail} Try Restart, or check its logs.</span>
              </div>
            </div>
          )}
          {service.problem && !running && (
            <ProblemPanel kind={service.kind} problem={service.problem} />
          )}
        </div>
      )}
    </article>
  );
}

function Stat({ label, value, live }: { label: string; value: string | number; live?: boolean }) {
  return (
    <div className="flex min-w-12 flex-col gap-0.5">
      <span className="text-[10px] font-bold tracking-wider text-muted uppercase">{label}</span>
      <span className={clsx("font-mono text-[13px] tabular-nums", live ? "font-semibold text-accent-text" : "text-fg")}>{value}</span>
    </div>
  );
}

/** Windows: Apache and PHP need Microsoft's Visual C++ runtime. */
function VcRuntimeBanner() {
  const notify = useAppStore((s) => s.notify);
  const status = useQuery({ queryKey: ["vc-runtime"], queryFn: api.vcRuntimeStatus });
  const install = useMutation({
    mutationFn: api.vcRuntimeInstall,
    onSuccess: () => {
      notify("info", "Visual C++ runtime installed");
      status.refetch();
    },
    onError: (e) => notify("error", String(e)),
  });
  if (!status.data?.applicable || status.data.installed) return null;
  return (
    <div className="flex flex-wrap items-center gap-4 rounded-2xl border border-warn/30 bg-warn/10 px-5 py-3.5">
      <p className="min-w-60 flex-1 text-sm">
        Apache and PHP need the Microsoft Visual C++ runtime, which isn't installed. Windows asks for administrator approval once.
      </p>
      <Button variant="primary" onClick={() => install.mutate()} disabled={install.isPending}>
        {install.isPending ? "Installing…" : "Install runtime"}
      </Button>
    </div>
  );
}

function PanelTitle({ title, extra }: { title: string; extra?: ReactNode }) {
  return (
    <div className="mb-2 flex items-center justify-between gap-2 px-1">
      <h2 className="text-sm font-bold">{title}</h2>
      {extra}
    </div>
  );
}

/** One-click tools, each saying what it needs before it can open. */
function QuickShortcuts({ sites, services, settings }: { sites?: SitesInfo; services: ServiceInfo[]; settings?: Settings }) {
  const navigate = useAppStore((s) => s.navigate);
  const notify = useAppStore((s) => s.notify);
  const mailUi = settings?.ports.mailUi;
  const up = (kind: ServiceKind) => services.some((s) => s.kind === kind && s.state === "running");
  const web = primaryServer(settings);
  const db = up("mariadb") || up("mysql");

  const installed = (kind: ServiceKind) => services.some((s) => s.kind === kind && s.installed);
  const onError = (e: unknown) => notify("error", String(e));
  const webTool = (tool: "phpmyadmin" | "adminer" | "phpredisadmin") => () => {
    const url = toolUrl(tool, sites, mailUi);
    if (url) openUrl(url).catch(onError);
  };
  const tools = [
    { tool: "mailpit" as const, label: "Mailpit inbox", ready: up("mailpit"), need: "Start Mailpit", readyText: "Open inbox", installed: installed("mailpit"), open: () => openUrl(toolUrl("mailpit", sites, mailUi) ?? "http://localhost:8025/").catch(onError) },
    { tool: "phpmyadmin" as const, label: "phpMyAdmin", ready: up(web) && db, need: `Start ${serviceLabels[web]} and a database`, readyText: "Browse databases", installed: !!toolUrl("phpmyadmin", sites, mailUi), open: webTool("phpmyadmin") },
    { tool: "adminer" as const, label: "Adminer", ready: up(web) && db, need: `Start ${serviceLabels[web]} and a database`, readyText: "Lightweight database admin", installed: !!toolUrl("adminer", sites, mailUi), open: webTool("adminer") },
    { tool: "phpredisadmin" as const, label: "phpRedisAdmin", ready: up(web) && up("redis"), need: `Start ${serviceLabels[web]} and Redis`, readyText: "Browse Redis keys", installed: !!toolUrl("phpredisadmin", sites, mailUi), open: webTool("phpredisadmin") },
  ];
  const readyCount = tools.filter((t) => t.installed && t.ready).length;

  return (
    <Card className="p-4">
      <PanelTitle title="Quick shortcuts" extra={<span className="text-xs text-muted">{readyCount} of {tools.length} ready</span>} />
      <div className="space-y-1">
        {tools.map((t) => {
          const tiles = runtimeTiles[t.tool];
          const hint = !t.installed ? "Not installed. Click to install" : t.ready ? t.readyText : t.need;
          return (
            <button
              key={t.tool}
              onClick={() => (t.installed ? t.open() : navigate("services"))}
              disabled={t.installed && !t.ready}
              className="group flex w-full items-center gap-3 rounded-xl border border-transparent px-2 py-2 text-left hover:border-line hover:bg-surface-2/60 disabled:cursor-not-allowed disabled:opacity-70 disabled:hover:border-transparent disabled:hover:bg-transparent"
            >
              <span className={clsx("flex size-8 shrink-0 items-center justify-center rounded-lg", tiles.className)} aria-hidden>
                {tiles.icon(16)}
              </span>
              <span className="min-w-0 flex-1">
                <span className="block truncate text-sm font-semibold">{t.label}</span>
                <span className={clsx("block truncate text-xs", t.installed && t.ready ? "font-medium text-accent-text" : "text-muted")}>{hint}</span>
              </span>
              <ChevronRight size={15} className="shrink-0 text-muted" aria-hidden />
            </button>
          );
        })}
      </div>
    </Card>
  );
}

/** Connection details for the installed services, as URLs or ready-to-paste .env lines. */
function ConnectPanel({ services, onCopy }: { services: ServiceInfo[]; onCopy: (text: string, what: string) => void }) {
  const mode = useAppStore((s) => s.connMode);
  const setMode = useAppStore((s) => s.setConnMode);
  const [copied, setCopied] = useState<string | null>(null);
  // One web server's APP_URL is enough: the first one listed.
  const web = services.find((s) => isServer(s.kind));
  const shown = services.filter((s) => !isServer(s.kind) || s === web);
  if (shown.length === 0) return null;

  const copy = (key: string, text: string, what: string) => {
    onCopy(text, what);
    setCopied(key);
    setTimeout(() => setCopied((c) => (c === key ? null : c)), 1500);
  };
  const allEnv = shown.map((s) => `# ${s.label}\n${envText(connection(s.kind, s.port).env)}`).join("\n\n");

  return (
    <Card className="p-4">
      <PanelTitle
        title="Connect your app"
        extra={
          <Segmented
            value={mode}
            onChange={setMode}
            className="p-0.5! text-xs"
            options={[
              { value: "url", label: "URL" },
              { value: "env", label: ".env" },
            ]}
          />
        }
      />
      <ul className="space-y-1.5">
        {shown.map((s) => {
          const c = connection(s.kind, s.port);
          const text = copyText(c, mode);
          return (
            <li key={s.kind}>
              <button
                onClick={() => copy(s.kind, text, `${s.label} ${mode === "env" ? ".env" : "URL"}`)}
                className="group flex w-full items-start gap-2 rounded-lg border border-line bg-bg px-2.5 py-2 text-left hover:border-accent/40"
                aria-label={`Copy ${s.label} connection`}
              >
                <span className="min-w-0 flex-1">
                  <span className="block text-xs font-semibold text-muted">{s.label}</span>
                  <span className="block font-mono text-xs break-all whitespace-pre-wrap text-fg">{text}</span>
                </span>
                {copied === s.kind ? (
                  <Check size={14} className="mt-0.5 shrink-0 text-accent-text" aria-hidden />
                ) : (
                  <Copy size={14} className="mt-0.5 shrink-0 text-muted group-hover:text-accent-text" aria-hidden />
                )}
              </button>
            </li>
          );
        })}
      </ul>
      <Button size="sm" variant="soft" className="mt-3 w-full" onClick={() => copy("all", allEnv, "all services as .env")}>
        {copied === "all" ? <Check size={14} /> : <Copy size={14} />} Copy everything as .env
      </Button>
    </Card>
  );
}

function RecentProjects({ sites, onNew }: { sites: Site[]; onNew: () => void }) {
  const navigate = useAppStore((s) => s.navigate);
  const notify = useAppStore((s) => s.notify);
  const onError = (e: unknown) => notify("error", String(e));
  const recent = [...sites].sort((a, b) => b.modified - a.modified).slice(0, 4);

  return (
    <Card className="p-4">
      <PanelTitle
        title="Recent projects"
        extra={
          <button onClick={() => navigate("sites")} className="text-xs font-semibold text-accent-text hover:underline">
            View all ›
          </button>
        }
      />
      {recent.length === 0 && <p className="px-1 py-2 text-sm text-muted">Each folder in www becomes a site.</p>}
      <ul className="space-y-0.5">
        {recent.map((site) => (
          <li key={site.path} className="group flex items-center gap-1">
            <button
              onClick={() => openUrl(site.url).catch(onError)}
              className="flex min-w-0 flex-1 items-center gap-3 rounded-xl px-2 py-2 text-left hover:bg-surface-2/60"
              title={`Open ${site.url}`}
            >
              <span className="flex size-8 shrink-0 items-center justify-center rounded-lg bg-accent-soft text-accent-text" aria-hidden>
                <Globe size={16} />
              </span>
              <span className="min-w-0 flex-1">
                <span className="block truncate text-sm font-semibold">{site.domain}</span>
                <span className="selectable block truncate font-mono text-xs text-muted">{site.path}</span>
              </span>
              <span className="shrink-0 rounded-md bg-surface-2 px-1.5 py-0.5 text-[11px] font-medium text-muted">
                {serviceLabels[site.server]}
              </span>
            </button>
            <Button size="sm" variant="ghost" onClick={() => openPath(site.path).catch(onError)} aria-label={`Open ${site.name} folder`}>
              <FolderOpen size={14} />
            </Button>
          </li>
        ))}
      </ul>
      <button
        onClick={onNew}
        className="mt-1.5 flex w-full items-center justify-center gap-1.5 rounded-xl border border-dashed border-line py-2 text-sm font-medium text-muted hover:border-accent/50 hover:bg-accent-soft/40 hover:text-accent-text"
        title="New project (N)"
      >
        <Plus size={14} /> New project
      </button>
    </Card>
  );
}

const activityDot: Record<Activity["tone"], string> = {
  ok: "bg-ok",
  info: "bg-sky-500",
  muted: "bg-muted/50",
  error: "bg-danger",
};

function RecentActivity() {
  const activity = useAppStore((s) => s.activity);
  const navigate = useAppStore((s) => s.navigate);

  return (
    <Card className="p-4">
      <PanelTitle
        title="Recent activity"
        extra={
          <button onClick={() => navigate("logs")} className="text-xs font-semibold text-accent-text hover:underline">
            Logs ›
          </button>
        }
      />
      <ul className="space-y-1.5 px-1">
        {activity.slice(0, 4).map((a) => (
          <li key={a.id} className="flex items-center gap-2.5 text-xs">
            <span className={clsx("size-2 shrink-0 rounded-full", activityDot[a.tone])} aria-hidden />
            <span className="font-mono text-muted tabular-nums">{formatClock(a.at)}</span>
            <span className="min-w-0 flex-1 truncate">{a.text}</span>
          </li>
        ))}
      </ul>
    </Card>
  );
}
