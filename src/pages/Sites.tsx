import { Fragment, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { openPath, openUrl } from "@tauri-apps/plugin-opener";
import {
  AlertTriangle,
  Check,
  Code2,
  Copy,
  Database,
  ExternalLink,
  FileText,
  FolderOpen,
  FolderPlus,
  GitBranch,
  Globe,
  Clock,
  Package,
  Plus,
  Play,
  RotateCcw,
  Square,
  RefreshCw,
  Search,
  ShieldCheck,
  SquareTerminal,
  Trash2,
  X,
  type LucideIcon,
} from "lucide-react";
import clsx from "clsx";
import { api, runtimeLabels, serviceLabels, type AppStatus, type CommandRun, type Language, type ProjectFacts, type ProjectFile, type ProjectProcess, type Server, type DomainTarget, type ServiceKind, type Settings, type Site, type SitesInfo } from "../lib/api";
import { useAppStore } from "../lib/store";
import Dialog from "../components/Dialog";
import ProjectDomains from "../components/ProjectDomains";
import ProjectProcesses, { processFrom } from "../components/ProjectProcesses";
import { toolUrl } from "../lib/tools";
import { useHotkeys } from "../lib/shortcuts";
import { bestMatch, satisfies } from "../lib/constraint";
import { formatAgo, useNow } from "../lib/time";
import { Button, Card, PageHeader, Switch, inputClass } from "../components/ui";
import { ShareButton, TunnelRow, useProjectTunnel } from "../components/Tunnels";
import { AppControls, CommandPrompt, CommandsPanel, OutputPanel, Row, ServeSettings, appCommand, isActive, stateStyle, useAppActions, useAppInfo, useAppStatuses, useUnreadOutput } from "../components/ProjectRun";

/** Avatar colors per framework; anything else gets the accent. */
const frameworkStyle: Record<string, string> = {
  Laravel: "bg-red-500/12 text-red-600 dark:text-red-400",
  WordPress: "bg-sky-600/12 text-sky-700 dark:text-sky-400",
  Symfony: "bg-zinc-500/15 text-fg",
  Drupal: "bg-blue-500/12 text-blue-600 dark:text-blue-400",
  CodeIgniter: "bg-orange-500/12 text-orange-600 dark:text-orange-400",
  CakePHP: "bg-rose-500/12 text-rose-600 dark:text-rose-400",
  Yii: "bg-emerald-500/12 text-emerald-700 dark:text-emerald-400",
  Slim: "bg-teal-500/12 text-teal-700 dark:text-teal-400",
  Statamic: "bg-pink-500/12 text-pink-600 dark:text-pink-400",
  Magento: "bg-orange-600/12 text-orange-700 dark:text-orange-400",
  PHP: "bg-indigo-500/12 text-indigo-600 dark:text-indigo-400",
  Vite: "bg-violet-500/12 text-violet-600 dark:text-violet-400",
  "Next.js": "bg-zinc-500/15 text-fg",
  Nuxt: "bg-emerald-500/12 text-emerald-700 dark:text-emerald-400",
  Astro: "bg-orange-500/12 text-orange-600 dark:text-orange-400",
  SvelteKit: "bg-orange-600/12 text-orange-700 dark:text-orange-400",
  NestJS: "bg-rose-500/12 text-rose-600 dark:text-rose-400",
  Remix: "bg-zinc-500/15 text-fg",
  Express: "bg-zinc-500/15 text-fg",
  Node: "bg-green-600/12 text-green-700 dark:text-green-400",
  Static: "bg-surface-2 text-muted",
};

const languageLabel: Record<Language, string> = { php: "PHP", node: "Node.js", java: "Java", python: "Python", go: "Go", rust: "Rust", static: "Static" };

type Sort = "recent" | "name";
type Tab = "overview" | "commands" | "domains" | "settings";

export default function Sites() {
  const queryClient = useQueryClient();
  const notify = useAppStore((s) => s.notify);
  const openNewProject = useAppStore((s) => s.setNewProjectOpen);
  const onError = (e: unknown) => notify("error", String(e));
  const info = useQuery({ queryKey: ["sites"], queryFn: api.listSites, refetchInterval: 10_000 });
  const paths = useQuery({ queryKey: ["paths"], queryFn: api.envPaths });
  const statuses = useAppStatuses();
  const refresh = () => queryClient.invalidateQueries({ queryKey: ["sites"] });
  const [query, setQuery] = useState("");
  const [language, setLanguage] = useState<Language | null>(null);
  const [sort, setSort] = useState<Sort>("recent");
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [tab, setTab] = useState<Tab>("overview");

  const apply = useMutation({ mutationFn: api.applySites, onSuccess: refresh, onError });
  const trust = useMutation({
    mutationFn: api.trustCa,
    onSuccess: (note) => {
      notify(note ? "error" : "info", note ?? "Certificate trusted. Restart your browser if it still shows a warning.");
      refresh();
    },
    onError,
  });

  const data = info.data;
  const os = paths.data?.os;
  const all = useMemo(() => data?.sites ?? [], [data]);
  const languages = useMemo(() => (["php", "node", "static"] as Language[]).filter((l) => all.some((s) => s.language === l)), [all]);
  const q = query.trim().toLowerCase();
  const shown = all
    .filter((s) => !language || s.language === language)
    .filter((s) => !q || [s.name, s.domain, s.framework ?? "", s.gitBranch ?? ""].some((v) => v.toLowerCase().includes(q)))
    .sort((a, b) => (sort === "name" ? a.name.localeCompare(b.name) : b.modified - a.modified));
  const selectedSite = shown.find((site) => site.path === selectedPath) ?? shown[0];
  const running = all.filter((s) => isActive(statuses[s.name])).length;

  return (
    <>
      <PageHeader
        title="Sites"
        subtitle={
          <>
            Every folder in <span className="font-mono">www</span> is served at{" "}
            <span className="font-mono">&lt;folder&gt;.{data?.tld ?? "test"}</span>, sub-domains included.
          </>
        }
        actions={
          <>
            <Button onClick={() => paths.data && openPath(paths.data.www).catch(onError)}>
              <FolderOpen size={16} /> Open www
            </Button>
            <Button variant="primary" onClick={() => openNewProject(true)} title="New project (N)">
              <FolderPlus size={16} /> New project
            </Button>
            <Button onClick={() => info.refetch()} disabled={info.isFetching} aria-label="Refresh sites">
              <RefreshCw size={15} className={clsx(info.isFetching && "animate-spin")} />
            </Button>
          </>
        }
      />

      <div className="mb-5 space-y-3 empty:mb-0">
        {data?.needsApply && (
          <Banner
            icon={RefreshCw}
            tone="info"
            text="Your sites changed since the web server started. Apply to update domains, certificates and the hosts file."
            action={
              <Button variant="primary" onClick={() => apply.mutate()} disabled={apply.isPending}>
                {apply.isPending ? "Applying…" : "Apply changes"}
              </Button>
            }
          />
        )}
        {data && !data.hostsOk && !data.needsApply && data.sites.length > 0 && (
          <Banner
            icon={AlertTriangle}
            tone="warn"
            text={
              data.manageHosts
                ? "Some domains are not in the hosts file yet. This happens automatically when the web server starts, or update it now (needs administrator approval)."
                : "Some domains are not in the hosts file, and automatic updates are off in Settings."
            }
            action={
              <Button onClick={() => apply.mutate()} disabled={apply.isPending}>
                Update hosts file
              </Button>
            }
          />
        )}
        {data && data.unserved.length > 0 && (
          <Banner
            icon={AlertTriangle}
            tone="warn"
            text={
              <>
                {data.unserved.length === 1 ? "One folder in www isn't served:" : `${data.unserved.length} folders in www aren't served:`}
                <ul className="mt-1 space-y-0.5">
                  {data.unserved.map((u) => (
                    <li key={u.name}>
                      <span className="font-mono font-medium">{u.name}</span> <span className="text-muted">{u.reason}</span>
                    </li>
                  ))}
                </ul>
              </>
            }
          />
        )}
        {data && data.shadowedTools.length > 0 && (
          <Banner
            icon={AlertTriangle}
            tone="warn"
            text={`A folder in www uses ${data.shadowedTools.join(" and ")}, so that address shows your folder instead of the built-in tool. Rename the folder to get the tool back.`}
          />
        )}
        {data?.caTrusted === false && (
          <Banner
            icon={ShieldCheck}
            tone="warn"
            text={
              <>
                Browsers will warn on HTTPS sites until you trust LampRabit's local certificate authority.
                {os === "windows" && " Windows asks you to confirm once."}
                {os === "linux" && " Linux asks for your password once; Chrome and Firefox are covered too."}{" "}
                {os !== "linux" && <span className="text-muted">Firefox uses its own certificate list; enable "security.enterprise_roots.enabled" there.</span>}
              </>
            }
            action={
              <Button variant="primary" onClick={() => trust.mutate()} disabled={trust.isPending}>
                <ShieldCheck size={16} /> Trust certificate
              </Button>
            }
          />
        )}
      </div>

      {all.length === 0 ? (
        data && (
          <Card className="flex flex-col items-center gap-3 px-6 py-14 text-center">
            <div className="flex size-14 items-center justify-center rounded-full bg-accent-soft text-accent-text">
              <Globe size={26} />
            </div>
            <div className="text-lg font-semibold">No sites yet</div>
            <p className="max-w-md text-muted">
              Create a folder inside <span className="font-mono">www</span> with an <span className="font-mono">index.php</span> or a package.json, or start from a
              template: Laravel, WordPress, Next.js, Vite, Nuxt and more.
            </p>
            <Button variant="primary" onClick={() => openNewProject(true)}>
              <FolderPlus size={16} /> New project
            </Button>
          </Card>
        )
      ) : (
        <div className="grid items-start gap-5 lg:grid-cols-[minmax(240px,290px)_minmax(0,1fr)]">
          <Card className="min-w-0 overflow-hidden lg:sticky lg:top-4">
            <div className="space-y-2.5 border-b border-line p-3">
              <div className="flex items-center justify-between px-1">
                <h2 className="text-sm font-bold">
                  Projects <span className="font-normal text-muted tabular-nums">{shown.length === all.length ? all.length : `${shown.length} of ${all.length}`}</span>
                </h2>
                <span className="flex items-center gap-2 text-xs text-muted">
                  {running > 0 && (
                    <span className="flex items-center gap-1" title="Projects with a running dev server">
                      <span className="size-1.5 rounded-full bg-ok" aria-hidden /> {running} running
                    </span>
                  )}
                  <select value={sort} onChange={(e) => setSort(e.target.value as Sort)} aria-label="Sort projects" className="rounded-md bg-transparent py-0.5 text-xs text-muted hover:text-fg">
                    <option value="recent">Recent</option>
                    <option value="name">A–Z</option>
                  </select>
                </span>
              </div>
              <label className="flex h-9 items-center gap-2 rounded-lg border border-line bg-surface-2/60 px-2.5 focus-within:border-accent">
                <Search size={14} className="shrink-0 text-muted" aria-hidden />
                <input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Search name, framework, branch" aria-label="Search projects" className="w-full min-w-0 bg-transparent text-sm outline-none placeholder:text-muted" />
                {query && (
                  <button type="button" onClick={() => setQuery("")} aria-label="Clear search" className="text-muted hover:text-fg">
                    <X size={13} />
                  </button>
                )}
              </label>
              {languages.length > 1 && (
                <div className="flex flex-wrap gap-1" role="radiogroup" aria-label="Language">
                  {[null, ...languages].map((l) => (
                    <button
                      key={l ?? "all"}
                      role="radio"
                      aria-checked={language === l}
                      onClick={() => setLanguage(l)}
                      className={clsx("rounded-md px-2 py-0.5 text-xs font-medium transition-colors", language === l ? "bg-accent-soft text-accent-text" : "text-muted hover:bg-surface-2 hover:text-fg")}
                    >
                      {l ? languageLabel[l] : "All"}
                    </button>
                  ))}
                </div>
              )}
            </div>
            {shown.length === 0 ? (
              <p className="px-4 py-8 text-center text-sm text-muted">No projects match.</p>
            ) : (
              <ul className="scrollbar-thin space-y-0.5 p-1.5 lg:max-h-[calc(100dvh-16rem)] lg:overflow-y-auto lg:overscroll-contain" aria-label="Projects">
                {shown.map((site) => (
                  <li key={site.path}>
                    <ProjectRow site={site} status={statuses[site.name]} selected={selectedSite?.path === site.path} onSelect={() => setSelectedPath(site.path)} />
                  </li>
                ))}
              </ul>
            )}
          </Card>
          <div className="min-w-0">{selectedSite ? <ProjectDetail key={selectedSite.path} site={selectedSite} info={data} status={statuses[selectedSite.name]} tab={tab} setTab={setTab} onSelect={setSelectedPath} /> : null}</div>
        </div>
      )}
    </>
  );
}

function Banner({ icon: Icon, tone, text, action }: { icon: LucideIcon; tone: "info" | "warn"; text: ReactNode; action?: ReactNode }) {
  return (
    <div className={clsx("flex flex-wrap items-center gap-4 rounded-2xl border px-5 py-3.5", tone === "info" ? "border-accent/25 bg-accent-soft" : "border-warn/30 bg-warn/10")}>
      <Icon size={20} className={tone === "info" ? "text-accent-text" : "text-warn"} />
      <div className="min-w-60 flex-1 text-sm">{text}</div>
      {action}
    </div>
  );
}

function Avatar({ site, status, size = "md" }: { site: Site; status?: AppStatus; size?: "md" | "lg" }) {
  return (
    <span className="relative shrink-0">
      <span
        className={clsx(
          "flex items-center justify-center font-bold",
          size === "lg" ? "size-12 rounded-2xl text-lg" : "size-9 rounded-xl text-sm",
          frameworkStyle[site.framework ?? ""] ?? "bg-accent-soft text-accent-text",
        )}
        aria-hidden
      >
        {(site.framework && site.framework !== "PHP" && site.framework !== "Static" ? site.framework : site.name).charAt(0).toUpperCase()}
      </span>
      {(isActive(status) || site.route) && (
        <span
          className={clsx("absolute -right-0.5 -bottom-0.5 rounded-full border-2 border-surface", size === "lg" ? "size-3.5" : "size-3", site.route || status?.state === "running" ? "bg-ok" : "animate-pulse bg-warn")}
          title={site.route ? `Served from port ${site.route.port}` : "Starting"}
        />
      )}
    </span>
  );
}

function ProjectRow({ site, status, selected, onSelect }: { site: Site; status?: AppStatus; selected: boolean; onSelect: () => void }) {
  const notify = useAppStore((s) => s.notify);
  const onError = (e: unknown) => notify("error", String(e));
  const { start, stop } = useAppActions(site.name);
  const active = isActive(status);
  const canRun = site.nodeProject || site.serveMode === "dynamic";

  return (
    <div
      role="button"
      tabIndex={0}
      aria-current={selected ? "true" : undefined}
      onClick={onSelect}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onSelect();
        }
      }}
      className={clsx(
        "group relative flex w-full cursor-pointer items-center gap-3 rounded-xl px-2.5 py-2 text-left transition-colors outline-none focus-visible:ring-2 focus-visible:ring-accent",
        selected ? "bg-accent-soft" : "hover:bg-surface-2",
      )}
    >
      <Avatar site={site} status={status} />
      <span className="min-w-0 flex-1">
        <span className={clsx("block truncate text-sm font-semibold", selected && "text-accent-text")}>{site.name}</span>
        <span className="block truncate text-xs text-muted">
          {site.framework && site.framework !== "Static" ? site.framework : languageLabel[site.language]}
          {site.gitBranch && (
            <>
              <span aria-hidden> · </span>
              <GitBranch size={10} className="inline" aria-hidden /> {site.gitBranch}
            </>
          )}
        </span>
      </span>
      <div className="flex shrink-0 items-center gap-1 group-hover:hidden">
        {site.route ? (
          <span className="rounded-md bg-ok/12 px-1.5 py-0.5 font-mono text-[11px] text-accent-text" title={`Served from port ${site.route.port}`}>
            :{site.route.port}
          </span>
        ) : (
          status?.state === "crashed" && <span className="size-2 rounded-full bg-danger" title="Crashed" />
        )}
      </div>
      <div className="hidden shrink-0 items-center gap-1 group-hover:flex">
        {canRun &&
          (active ? (
            <button
              type="button"
              onClick={(e) => {
                e.stopPropagation();
                stop.mutate();
              }}
              disabled={stop.isPending}
              className="rounded-lg p-1 text-muted hover:bg-surface hover:text-danger-text"
              title="Stop dev server"
              aria-label={`Stop dev server for ${site.name}`}
            >
              <Square size={12} className="fill-current" />
            </button>
          ) : (
            <button
              type="button"
              onClick={(e) => {
                e.stopPropagation();
                start.mutate();
              }}
              disabled={start.isPending}
              className="rounded-lg p-1 text-muted hover:bg-surface hover:text-accent-text"
              title="Start dev server"
              aria-label={`Start dev server for ${site.name}`}
            >
              <Play size={12} />
            </button>
          ))}
        <button
          type="button"
          onClick={(e) => {
            e.stopPropagation();
            openUrl(site.url).catch(onError);
          }}
          className="rounded-lg p-1 text-muted hover:bg-surface hover:text-accent-text"
          title={`Open ${site.url}`}
          aria-label={`Open ${site.name} in browser`}
        >
          <ExternalLink size={13} />
        </button>
      </div>
    </div>
  );
}

/**
 * The project's dev server in the header: one clear "Start dev server" button, and while it
 * runs a compact group with its state and port, restart and stop.
 */
function DevServerButton({ name, status }: { name: string; status?: AppStatus }) {
  const { start, stop, restart } = useAppActions(name);
  if (isActive(status)) {
    const starting = status?.state === "starting";
    return (
      <div className="flex h-9 items-stretch overflow-hidden rounded-lg border border-line bg-surface shadow-card" role="group" aria-label="Dev server">
        <span className="flex items-center gap-2 px-3 text-sm font-medium" role="status">
          <span className={clsx("size-2 rounded-full", starting ? "animate-pulse bg-warn" : "bg-ok")} aria-hidden />
          {starting ? "Starting…" : "Dev server"}
          {!starting && status?.port ? <span className="font-mono text-xs text-muted">:{status.port}</span> : null}
        </span>
        <button
          type="button"
          onClick={() => restart.mutate()}
          disabled={restart.isPending}
          title="Restart dev server"
          aria-label="Restart dev server"
          className="flex items-center border-l border-line px-2.5 text-muted transition-colors hover:bg-surface-2 hover:text-fg disabled:opacity-50"
        >
          <RotateCcw size={14} className={clsx(restart.isPending && "animate-spin")} />
        </button>
        <button
          type="button"
          onClick={() => stop.mutate()}
          disabled={stop.isPending}
          className="flex items-center gap-1.5 border-l border-line px-3 text-sm font-medium text-danger-text transition-colors hover:bg-danger/10 disabled:opacity-50"
        >
          <Square size={11} className="fill-current" /> Stop
        </button>
      </div>
    );
  }
  return (
    <Button variant="primary" onClick={() => start.mutate()} disabled={start.isPending} title="Start the project's dev server">
      <Play size={13} className="fill-current" /> {start.isPending ? "Starting…" : "Start dev server"}
    </Button>
  );
}

function ProjectDetail({ site, info, status, tab, setTab, onSelect }: { site: Site; info?: SitesInfo; status?: AppStatus; tab: Tab; setTab: (t: Tab) => void; onSelect: (path: string) => void }) {
  const notify = useAppStore((s) => s.notify);
  const onError = (e: unknown) => notify("error", String(e));
  const settings = useQuery({ queryKey: ["settings"], queryFn: api.getSettings });
  const app = useAppInfo(site);
  const facts = useQuery({ queryKey: ["project-facts", site.name], queryFn: () => api.projectFacts(site.name), refetchInterval: 30_000 });
  const tunnel = useProjectTunnel(site);
  const dynamic = site.serveMode === "dynamic";
  // Running from your own terminal (or anything else LampRabit didn't start).
  const external = !!site.route && !isActive(status);
  // PHP projects with a package.json have an asset dev server (Vite) next to the site.
  const devServer = !dynamic && site.nodeProject && !!app.data && !!appCommand(app.data.config);
  const php = site.php ?? settings.data?.versions.php ?? null;
  const pmaUrl = toolUrl("phpmyadmin", info, settings.data?.ports.mailUi);
  const adminerUrl = toolUrl("adminer", info, settings.data?.ports.mailUi);
  const targetDbUrl = site.database
    ? (pmaUrl ? `${pmaUrl}?server=1&db=${encodeURIComponent(site.database)}` : adminerUrl ? `${adminerUrl}?db=${encodeURIComponent(site.database)}` : null)
    : (pmaUrl ?? adminerUrl ?? null);
  // The Commands tab's dot: a crashed process (red) outranks something running.
  const statuses = useAppStatuses();
  const processStates = Object.values(statuses).filter((s) => s.name.startsWith(`${site.name}/`));
  const activity = processStates.some((s) => s.state === "crashed") || status?.state === "crashed" ? "crashed" : status?.task || processStates.some(isActive) ? "running" : null;
  const tabs: { id: Tab; label: string }[] = [
    { id: "overview", label: "Overview" },
    { id: "commands", label: "Commands" },
    { id: "domains", label: "Domains" },
    { id: "settings", label: "Settings" },
  ];
  // [ and ] step through the project's tabs (listed in SHORTCUTS).
  const step = (by: number) => setTab(tabs[(tabs.findIndex((t) => t.id === tab) + by + tabs.length) % tabs.length].id);
  useHotkeys({ "[": () => step(-1), "]": () => step(1) });

  const now = useNow();
  // The one chip next to the name: the framework, else the language.
  const hasFramework = !!site.framework && site.framework !== "Static" && site.framework !== "PHP";
  const kindLabel = hasFramework ? site.framework! : languageLabel[site.language];
  // The site only answers while its web server runs.
  const services = useQuery({ queryKey: ["services"], queryFn: api.listServices, refetchInterval: 5000 });
  const web = services.data?.find((s) => s.kind === site.server);
  const serverUp = web?.state === "running";
  const serverName = serviceLabels[site.server];
  const offReason = !web
    ? "Checking the web server…"
    : !web.installed
      ? `${serverName} isn't installed. Install it on the Services page to open this site.`
      : web.state === "starting"
        ? `${serverName} is still starting.`
        : `${serverName} isn't running. Start it to open ${site.domain}.`;

  return (
    <div className="space-y-4">
      <Card className="p-5 shadow-card">
        {/* Title: the project, its one framework (or language), and where it stands */}
        <div className="flex flex-wrap items-start gap-4">
          <Avatar site={site} status={status} size="lg" />
          <div className="min-w-0 flex-1">
            <div className="flex min-w-0 flex-wrap items-center gap-x-2.5 gap-y-1.5">
              <h2 className="truncate text-2xl font-bold tracking-tight text-fg">{site.name}</h2>
              <Chip className={frameworkStyle[kindLabel]}>
                {kindLabel}
                {hasFramework && facts.data?.frameworkVersion && <span className="font-mono opacity-80"> {facts.data.frameworkVersion}</span>}
              </Chip>
            </div>
            <div className="mt-2 flex flex-wrap items-center gap-x-5 gap-y-1.5 text-xs text-muted">
              <span className="flex min-w-0 items-center gap-1.5" title={site.gitBranch ? `Git branch ${site.gitBranch}` : "This project isn't a Git repository"}>
                <GitBranch size={14} className="shrink-0" aria-hidden />
                {site.gitBranch ? (
                  <>
                    <span className="truncate font-mono text-fg">{site.gitBranch}</span>
                    {!!facts.data?.git?.changes && <span className="text-xs text-warn">· {facts.data.git.changes} uncommitted</span>}
                  </>
                ) : (
                  <span>No Git</span>
                )}
              </span>
              <button
                type="button"
                onClick={() => openPath(site.path).catch(onError)}
                className="flex min-w-0 items-center gap-1.5 hover:text-accent-text"
                title={`${site.path}: open the folder`}
              >
                <FolderOpen size={14} className="shrink-0" aria-hidden />
                <span className="truncate font-mono">{site.path.split(/[\\/]/).pop()}</span>
              </button>
              <span className="flex items-center gap-1.5" title={site.modified ? new Date(site.modified).toLocaleString() : undefined}>
                <Clock size={14} className="shrink-0" aria-hidden />
                {site.modified ? `Changed ${formatAgo(site.modified, now)}` : "Not changed yet"}
              </span>
            </div>
          </div>

          <div className="flex shrink-0 flex-wrap items-center gap-2">
            {/* A dev server started outside LampRabit (your terminal) still shows. */}
            {external && <LivePill site={site} />}
            {dynamic && !external && <AppControls name={site.name} status={status} />}
            {!serverUp && web?.installed && web.state !== "starting" && (
              // The one main button while the site can't answer (unless the dev server's Start already is).
              <Button
                variant={dynamic && !external && !isActive(status) ? "secondary" : "primary"}
                onClick={() => api.startService(site.server).then(() => services.refetch(), onError)}
                title={`Start ${serviceLabels[site.server]}`}
              >
                <Play size={13} className="fill-current" /> Start {serviceLabels[site.server]}
              </Button>
            )}
            {/* A disabled button gets no hover events; the wrapper carries the reason. */}
            <span title={serverUp ? `Open ${site.url}` : offReason} tabIndex={serverUp ? -1 : 0} className="inline-flex rounded-lg">
              <Button
                variant={!serverUp ? "ghost" : dynamic && !isActive(status) && !external ? "secondary" : "primary"}
                onClick={() => openUrl(site.url).catch(onError)}
                disabled={!serverUp}
                aria-label={serverUp ? "Open site" : `Open site (${offReason})`}
              >
                <ExternalLink size={14} /> Open site
              </Button>
            </span>
          </div>
        </div>

        <div role="tablist" aria-label={`${site.name} sections`} className="-mx-5 mt-4 flex flex-wrap gap-1 border-y border-line/80 px-3">
          {tabs.map((t) => (
            <button
              key={t.id}
              role="tab"
              aria-selected={tab === t.id}
              onClick={() => setTab(t.id)}
              className={clsx("-mb-px flex shrink-0 items-center gap-1.5 border-b-2 px-3.5 py-2.5 text-sm font-medium transition-colors", tab === t.id ? "border-accent text-fg" : "border-transparent text-muted hover:text-fg")}
            >
              {t.label}
              {t.id === "commands" && activity && (
                <span
                  className={clsx("size-1.5 rounded-full", activity === "crashed" ? "bg-danger" : "animate-pulse bg-accent")}
                  title={activity === "crashed" ? "A process crashed" : "Something is running"}
                  aria-label={activity === "crashed" ? "A process crashed" : "Something is running"}
                />
              )}
            </button>
          ))}
        </div>

        <div role="tabpanel" className="-mx-5 border-b border-line/80 bg-bg/60 px-5 py-4">
          {tab === "overview" && <Overview site={site} info={info} settings={settings.data} facts={facts.data} status={status} php={php} onCommands={() => setTab("commands")} />}
          {tab === "commands" && <CommandsTab site={site} settings={settings.data} status={status} />}
          {tab === "domains" && <ProjectDomains site={site} settings={settings.data} />}
          {tab === "settings" && <SettingsTab site={site} settings={settings.data} status={status} onRenamed={onSelect} />}
        </div>

        {/* Action toolbar */}
        <div className="-mb-1 flex flex-wrap items-center gap-1.5 pt-3">
          <Button size="sm" variant="ghost" onClick={() => api.openInEditor(site.path).catch(onError)}>
            <Code2 size={14} /> Editor
          </Button>
          <Button size="sm" variant="ghost" onClick={() => api.openTerminal(site.path).catch(onError)}>
            <SquareTerminal size={14} /> Terminal
          </Button>
          <Button size="sm" variant="ghost" onClick={() => openPath(site.path).catch(onError)}>
            <FolderOpen size={14} /> Folder
          </Button>
          {(targetDbUrl || site.database) && (
            <Button
              size="sm"
              variant="ghost"
              onClick={() => {
                if (targetDbUrl) {
                  openUrl(targetDbUrl).catch(onError);
                } else {
                  useAppStore.getState().navigate("services");
                  notify("info", "Install phpMyAdmin or Adminer in Services to manage databases");
                }
              }}
              title={site.database ? `Open database "${site.database}"` : "Open database manager"}
            >
              <Database size={14} /> {site.database || "Database"}
            </Button>
          )}
          {facts.data?.envFile && (
            <Button
              size="sm"
              variant="ghost"
              onClick={() => api.openInEditor(`${site.path}/${facts.data?.envFile}`).catch(onError)}
              title={`Open ${facts.data.envFile} in editor`}
            >
              <FileText size={14} /> {facts.data.envFile}
            </Button>
          )}

          <span className="flex-1" />

          {!tunnel && <ShareButton site={site} />}
          {devServer && <DevServerButton name={site.name} status={status} />}
        </div>
        {tunnel && (
          <div className="mt-3">
            <TunnelRow tunnel={tunnel} />
          </div>
        )}

      </Card>

    </div>
  );
}



function Chip({ className, children }: { className?: string; children: ReactNode }) {
  return <span className={clsx("inline-flex items-center gap-1 rounded-md px-2 py-0.5 text-xs font-medium", className ?? "bg-surface-2 text-muted")}>{children}</span>;
}

/** "Running · :5173 · your terminal": a dev server LampRabit found rather than started. */
function LivePill({ site }: { site: Site }) {
  const port = site.ports.find((p) => p.port === site.route?.port);
  return (
    <span className="inline-flex items-center gap-1.5 rounded-full bg-surface-2 px-2.5 py-1 text-xs font-medium text-accent-text" title={port?.command}>
      <span className="size-1.5 rounded-full bg-ok" aria-hidden />
      Running <span className="font-mono text-muted">:{site.route?.port}</span>
      <span className="text-muted">· started outside LampRabit</span>
    </span>
  );
}

function Overview({
  site,
  info,
  settings,
  facts,
  status,
  php,
  onCommands,
}: {
  site: Site;
  info?: SitesInfo;
  settings?: Settings;
  facts?: ProjectFacts;
  status?: AppStatus;
  php: string | null;
  onCommands: () => void;
}) {
  const queryClient = useQueryClient();
  const notify = useAppStore((s) => s.notify);
  const navigate = useAppStore((s) => s.navigate);
  const onError = (e: unknown) => notify("error", String(e));
  const install = useMutation({
    mutationFn: (manager: string) => (manager === "Composer" ? api.runProjectCommand(site.name, "composer install", "composer install") : api.appInstall(site.name)),
    onSuccess: onCommands,
    onError,
  });
  const dbUrl = site.database ? databaseUrl(site.database, info, settings) : null;
  const siteUrl = site.url.replace(/\/$/, "");
  const appUrl = facts?.env.find(([k]) => k === "APP_URL")?.[1];
  const urlMismatch = !!appUrl && appUrl.replace(/\/$/, "") !== siteUrl;
  const fixUrl = useMutation({
    mutationFn: () => api.setProjectEnv(site.name, "APP_URL", siteUrl),
    onSuccess: () => {
      notify("info", `APP_URL is now ${siteUrl}`);
      queryClient.invalidateQueries({ queryKey: ["project-facts", site.name] });
    },
    onError,
  });

  // composer.json's PHP requirement against the PHP the site runs on (FrankenPHP brings its own).
  const installed = useQuery({ queryKey: ["installed"], queryFn: api.installedRuntimes });
  const phpVersions = installed.data?.find((i) => i.runtime === "php")?.versions ?? [];
  const needs = facts?.phpRequirement ?? null;
  const phpMismatch = site.language === "php" && site.server !== "frankenphp" && !!needs && !!php && !satisfies(php, needs);
  const better = phpMismatch && needs ? bestMatch(phpVersions, needs) : null;
  const switchPhp = useMutation({
    mutationFn: (version: string) => api.setSitePhp(site.name, version),
    onSuccess: (_, version) => {
      notify("info", `${site.name} now uses PHP ${version}`);
      for (const key of ["sites", "settings", "services", "php-status"]) queryClient.invalidateQueries({ queryKey: [key] });
    },
    onError,
  });

  return (
    <div className="space-y-4">
      {phpMismatch && (
        <div className="flex flex-wrap items-center gap-3 rounded-xl border border-warn/30 bg-warn/10 px-4 py-3 text-sm">
          <AlertTriangle size={16} className="shrink-0 text-warn" aria-hidden />
          <span className="min-w-0 flex-1 text-fg">
            This project needs PHP <span className="font-mono">{needs}</span> but runs on PHP {php}, so it may fail to start.
          </span>
          {better ? (
            <Button size="sm" variant="primary" onClick={() => switchPhp.mutate(better)} disabled={switchPhp.isPending}>
              {switchPhp.isPending ? "Switching…" : `Switch to PHP ${better}`}
            </Button>
          ) : (
            <Button size="sm" onClick={() => navigate("services")}>
              Install a matching PHP
            </Button>
          )}
        </div>
      )}

      {(isActive(status) || status?.task) && <OutputPanel name={site.name} height="max-h-56 min-h-24" />}

      <ProjectInfo site={site} settings={settings} facts={facts} status={status} php={php} />

      <div className="grid gap-4 xl:grid-cols-2">
        <Section
          title="Dependencies"
          action={
            facts?.dependencies?.some((d) => !d.installed || d.outdated) ? (
              <span className="text-[11px] font-semibold text-warn">Needs install</span>
            ) : facts?.dependencies?.length ? (
              <span className="text-[11px] font-medium text-accent-text">All ready</span>
            ) : null
          }
        >
          {facts?.dependencies && facts.dependencies.length > 0 ? (
            <div className="space-y-3">
              {facts.dependencies.map((d) => (
                <div key={d.manager} className="flex items-center justify-between gap-3 rounded-xl border border-line/60 bg-surface-2/40 p-3">
                  <div className="min-w-0">
                    <div className="flex items-center gap-2">
                      <Package size={15} className="text-accent-text" />
                      <span className="text-sm font-semibold text-fg">{d.manager === "Composer" ? "Composer" : `Node (${d.manager})`}</span>
                    </div>
                    <p className="mt-0.5 text-xs text-muted">
                      {d.count} {d.count === 1 ? "package" : "packages"}
                      {d.devCount > 0 && <span> · {d.devCount} dev</span>}
                    </p>
                  </div>
                  {d.installed && d.outdated ? (
                    <span className="flex shrink-0 items-center gap-2">
                      <span className="text-xs font-semibold text-warn" title="The lock file changed since the last install (a pull or a branch switch).">
                        Out of date
                      </span>
                      <Button size="sm" variant="primary" onClick={() => install.mutate(d.manager)} disabled={install.isPending || !!status?.task}>
                        <Package size={13} /> Update
                      </Button>
                    </span>
                  ) : d.installed ? (
                    <span className="inline-flex items-center gap-1 rounded-full bg-ok/10 px-2.5 py-1 text-xs font-semibold text-ok">
                      <Check size={12} /> Installed
                    </span>
                  ) : (
                    <Button size="sm" variant="primary" onClick={() => install.mutate(d.manager)} disabled={install.isPending || !!status?.task}>
                      <Package size={13} /> Install
                    </Button>
                  )}
                </div>
              ))}
            </div>
          ) : (
            <p className="py-2 text-xs text-muted">No package manager dependencies found in project.</p>
          )}
        </Section>

        <Section
          title="Database & Environment"
          action={
            facts?.envFile && (
              <button
                type="button"
                onClick={() => api.openInEditor(`${site.path}/${facts.envFile}`).catch(onError)}
                className="cursor-pointer text-xs font-semibold text-accent-text hover:underline"
              >
                Edit {facts.envFile}
              </button>
            )
          }
        >
          <div className="space-y-3">
            {site.database && <DatabaseRow site={site} database={site.database} dbUrl={dbUrl} />}

            {urlMismatch && (
              <div className="flex items-center gap-2 rounded-xl border border-warn/30 bg-warn/10 py-1.5 pr-1.5 pl-2.5 text-xs text-warn">
                <AlertTriangle size={14} className="shrink-0" />
                <span className="min-w-0 flex-1 truncate" title={`APP_URL in ${facts?.envFile ?? ".env"} is ${appUrl}; the site is ${siteUrl}.`}>
                  APP_URL doesn't match the site's address
                </span>
                <Button size="sm" onClick={() => fixUrl.mutate()} disabled={fixUrl.isPending} title={`Set APP_URL to ${siteUrl}`}>
                  {fixUrl.isPending ? "Fixing…" : "Fix"}
                </Button>
              </div>
            )}

            {facts?.env && facts.env.length > 0 ? (
              <div className="space-y-1.5 pt-1">
                <span className="text-[11px] font-bold tracking-wider text-muted uppercase">Key Environment</span>
                <div className="grid gap-1.5 sm:grid-cols-2">
                  {facts.env.map(([key, value]) => (
                    <div key={key} className="flex items-center justify-between gap-2 rounded-lg border border-line/50 bg-surface px-2.5 py-1.5 text-xs">
                      <span className="font-mono text-muted">{key}</span>
                      <span className="max-w-35 truncate font-mono font-medium text-fg" title={value}>
                        {value}
                      </span>
                    </div>
                  ))}
                </div>
              </div>
            ) : facts?.envFile ? (
              <p className="text-xs text-muted">No public variables shown. Keys and passwords remain hidden.</p>
            ) : !site.database ? (
              <p className="py-2 text-xs text-muted">No database or environment configuration detected.</p>
            ) : null}
          </div>
        </Section>
      </div>
    </div>
  );
}

/**
 * The project's database and whether it can be used: its server installed and running,
 * and the database created. Each gap comes with the one button that closes it.
 */
function DatabaseRow({ site, database, dbUrl }: { site: Site; database: string; dbUrl: string | null }) {
  const queryClient = useQueryClient();
  const notify = useAppStore((s) => s.notify);
  const navigate = useAppStore((s) => s.navigate);
  const onError = (e: unknown) => notify("error", String(e));
  const status = useQuery({ queryKey: ["database-status", site.name], queryFn: () => api.databaseStatus(site.name), refetchInterval: 10_000 });
  const create = useMutation({
    mutationFn: () => api.createProjectDatabase(site.name),
    onSuccess: () => {
      notify("info", `Created the database ${database}`);
      status.refetch();
    },
    onError,
  });
  const start = useMutation({
    mutationFn: (kind: ServiceKind) => api.startService(kind),
    onSuccess: () => {
      status.refetch();
      queryClient.invalidateQueries({ queryKey: ["services"] });
    },
    onError,
  });
  const s = status.data;
  // phpMyAdmin only manages MariaDB and MySQL.
  const managed = !s || s.engine === "mariadb" || s.engine === "mysql";

  let note: ReactNode = null;
  let action: ReactNode = dbUrl && managed && (
    <Button size="sm" variant="secondary" onClick={() => openUrl(dbUrl).catch(onError)} className="shrink-0 text-xs">
      <ExternalLink size={12} /> Open in manager
    </Button>
  );
  if (s && !s.installed) {
    note = <span className="text-warn">{s.label} isn't installed</span>;
    action = (
      <Button size="sm" onClick={() => navigate("services")}>
        Install {s.label}
      </Button>
    );
  } else if (s && !s.running) {
    note = <span className="text-warn">{s.label} is stopped</span>;
    action = (
      <Button size="sm" variant="primary" onClick={() => start.mutate(s.engine as ServiceKind)} disabled={start.isPending}>
        <Play size={12} className="fill-current" /> {start.isPending ? "Starting…" : `Start ${s.label}`}
      </Button>
    );
  } else if (s && s.exists === false) {
    note = <span className="text-warn">Doesn't exist yet in {s.label}</span>;
    action = (
      <Button size="sm" variant="primary" onClick={() => create.mutate()} disabled={create.isPending}>
        <Plus size={13} /> {create.isPending ? "Creating…" : "Create it"}
      </Button>
    );
  } else if (s) {
    note = <span className="text-ok">Ready · {s.label}</span>;
  }

  return (
    <div className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-line/60 bg-surface-2/40 p-3">
      <div className="flex min-w-0 items-center gap-2.5">
        <span className="flex size-7 shrink-0 items-center justify-center rounded-lg bg-accent-soft text-accent-text">
          <Database size={15} />
        </span>
        <div className="min-w-0">
          <span className="block truncate font-mono text-sm font-bold text-fg">{database}</span>
          <span className="block text-xs">{note ?? <span className="text-muted">Database</span>}</span>
        </div>
      </div>
      {action}
    </div>
  );
}

/**
 * The project at a glance: one quiet two-column list of label/value rows; long values
 * truncate and show in full on hover. The folder, Git and last change are in the header.
 */
function ProjectInfo({ site, settings, facts, status, php }: { site: Site; settings?: Settings; facts?: ProjectFacts; status?: AppStatus; php: string | null }) {
  const queryClient = useQueryClient();
  const notify = useAppStore((s) => s.notify);
  const onError = (e: unknown) => notify("error", String(e));
  const node = useQuery({ queryKey: ["node"], queryFn: api.nodeStatus, enabled: site.nodeProject });
  const toggleSsl = useMutation({
    mutationFn: (ssl: boolean) => api.setSiteSsl(site.name, ssl),
    onSuccess: () => {
      for (const key of ["sites", "services"]) queryClient.invalidateQueries({ queryKey: [key] });
    },
    onError,
  });

  const docroot = site.docroot.slice(site.path.length).replace(/^[\\/]+/, "").replace(/\\/g, "/");
  const hasFramework = !!site.framework && site.framework !== "Static" && site.framework !== "PHP";
  const subdomains = (settings?.sites[site.name]?.domains ?? []).map((d) => d.host);
  const nodeVersion = status?.node ?? site.app?.node ?? site.node ?? node.data?.current ?? null;
  const nodeMissing = !!site.node && !!node.data?.manager && !node.data.installed.some((v) => v === site.node || v.startsWith(`${site.node}.`));

  // Where the domain's requests end up right now: a dev server's port, Tomcat, or the files.
  const target = site.route ? `http://${site.route.host}:${site.route.port}` : site.proxy && site.language === "java" ? site.proxy : docroot ? `${docroot}/` : "project folder";
  const targetNote = site.route
    ? site.ports.find((p) => p.port === site.route!.port)?.process
    : site.language === "php"
      ? site.server === "frankenphp"
        ? "FrankenPHP's built-in PHP"
        : php
          ? `PHP ${php}`
          : "no PHP installed"
      : site.language === "java"
        ? "Tomcat"
        : "static files";

  const runtimes: ReactNode[] = [];
  if (site.language === "php") {
    runtimes.push(
      <span key="php">
        {site.server === "frankenphp" ? "FrankenPHP" : php ? `PHP ${php}` : "PHP (none installed)"}
        {facts?.phpRequirement && <span className="text-muted"> · requires {facts.phpRequirement}</span>}
      </span>,
    );
  } else if (site.language !== "node" && site.language !== "static" && site.languageVersion) {
    runtimes.push(<span key="lang">{`${languageLabel[site.language]} ${site.languageVersion}`}</span>);
  }
  if (site.nodeProject) {
    runtimes.push(
      <span key="node">
        Node {nodeVersion ?? "default"}
        {nodeMissing && <span className="text-warn"> (not installed)</span>}
      </span>,
    );
  }

  return (
    <Section title="Project details">
      <dl className="grid gap-x-10 sm:grid-cols-2">
        <InfoRow label="Path" mono>
          <span className="selectable truncate" title={site.path}>
            {site.path}
          </span>
          <CopyButton text={site.path} />
        </InfoRow>
        <InfoRow label="Served from">
          <span className="truncate" title={targetNote ? `${target} · ${targetNote}` : target}>
            <span className="font-mono text-[13px]">{target}</span>
            {targetNote && <span className="text-muted"> · {targetNote}</span>}
          </span>
        </InfoRow>
        <InfoRow label="Main domain" mono>
          <button type="button" onClick={() => openUrl(site.url).catch(onError)} className="truncate text-accent-text hover:underline" title={`Open ${site.url}`}>
            {site.url.replace(/\/$/, "")}
          </button>
        </InfoRow>
        {subdomains.length > 0 && (
          <InfoRow label="Subdomains" mono>
            <span className="flex min-w-0 flex-wrap justify-end gap-x-3 gap-y-0.5">
              {subdomains.map((host) => (
                <button key={host} type="button" onClick={() => openUrl(site.url.replace(site.domain, host)).catch(onError)} className="truncate text-accent-text hover:underline" title={`Open ${host}`}>
                  {host}
                </button>
              ))}
            </span>
          </InfoRow>
        )}
        <InfoRow label="HTTPS">
          <Switch checked={site.ssl} onChange={(v) => toggleSsl.mutate(v)} label={`HTTPS for ${site.domain}`} />
        </InfoRow>
        <InfoRow label="Serve mode">{site.serveMode === "dynamic" ? "Dynamic (dev server)" : "Static (web server)"}</InfoRow>
        <InfoRow label="Web server">{serviceLabels[site.server]}</InfoRow>
        <InfoRow label="Framework">
          {hasFramework ? (
            <span className="truncate">
              {site.framework}
              {facts?.frameworkVersion && <span className="font-mono text-muted"> {facts.frameworkVersion}</span>}
            </span>
          ) : (
            <span className="text-muted">None</span>
          )}
        </InfoRow>
        <InfoRow label="Language">{languageLabel[site.language]}</InfoRow>
        <InfoRow label="Runtime">
          {runtimes.length ? (
            <span className="truncate">
              {runtimes.map((r, i) => (
                <Fragment key={i}>
                  {i > 0 && <span className="text-muted"> · </span>}
                  {r}
                </Fragment>
              ))}
            </span>
          ) : (
            <span className="text-muted">None</span>
          )}
        </InfoRow>
        <InfoRow label="Database" mono={!!site.database}>
          {site.database ?? <span className="text-muted">None</span>}
        </InfoRow>
        {facts?.package && (
          <InfoRow label="Package" mono>
            <span className="truncate">
              {facts.package}
              {facts.version && <span className="text-muted"> v{facts.version}</span>}
            </span>
          </InfoRow>
        )}
      </dl>
    </Section>
  );
}

function InfoRow({ label, mono, children }: { label: string; mono?: boolean; children: ReactNode }) {
  return (
    <div className="flex min-h-10 items-center justify-between gap-4 border-t border-line/60 py-2">
      <dt className="shrink-0 text-xs text-muted">{label}</dt>
      <dd className={clsx("flex min-w-0 items-center justify-end gap-1.5 text-right text-sm text-fg", mono && "font-mono text-[13px]")}>{children}</dd>
    </div>
  );
}

function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <button
      type="button"
      onClick={() =>
        navigator.clipboard.writeText(text).then(() => {
          setCopied(true);
          setTimeout(() => setCopied(false), 1200);
        })
      }
      className="shrink-0 rounded p-0.5 text-muted hover:text-accent-text"
      title="Copy path"
      aria-label="Copy path"
    >
      {copied ? <Check size={13} className="text-accent-text" /> : <Copy size={13} />}
    </button>
  );
}

function Section({ title, action, children }: { title: string; action?: ReactNode; children: ReactNode }) {
  return (
    <Card className="p-5">
      <div className="mb-3 flex items-center justify-between gap-3">
        <h3 className="text-xs font-bold tracking-wider text-muted uppercase">{title}</h3>
        {action}
      </div>
      <div>{children}</div>
    </Card>
  );
}

function SettingsTab({ site, settings, status, onRenamed }: { site: Site; settings?: Settings; status?: AppStatus; onRenamed: (path: string) => void }) {
  const queryClient = useQueryClient();
  const notify = useAppStore((s) => s.notify);
  const onError = (e: unknown) => notify("error", String(e));
  const installed = useQuery({ queryKey: ["installed"], queryFn: api.installedRuntimes });
  const app = useAppInfo(site);
  const { restart } = useAppActions(site.name);
  const phpVersions = installed.data?.find((i) => i.runtime === "php")?.versions ?? [];
  const onSuccess = () => {
    for (const key of ["sites", "settings", "services", "php-status"]) queryClient.invalidateQueries({ queryKey: [key] });
  };
  const setPhp = useMutation({ mutationFn: (v: string | null) => api.setSitePhp(site.name, v), onSuccess, onError });
  const setServer = useMutation({ mutationFn: (v: Server | null) => api.setSiteServer(site.name, v), onSuccess, onError });
  const setLanguage = useMutation({ mutationFn: (v: string | null) => api.setLanguageVersion(site.name, v), onSuccess, onError });
  const setWorker = useMutation({ mutationFn: (v: boolean) => api.setSiteWorker(site.name, v), onSuccess, onError });
  const restartWorkers = useMutation({
    mutationFn: api.restartFrankenphpWorkers,
    onSuccess: () => notify("info", "FrankenPHP's workers reloaded the code"),
    onError,
  });
  const worker = !!settings?.sites[site.name]?.worker;
  const languageRuntime = (["python", "java", "go", "rust"] as const).find((l) => l === site.language);
  const languageVersions = languageRuntime ? (installed.data?.find((i) => i.runtime === languageRuntime)?.versions ?? []) : [];
  const busy = setPhp.isPending || setServer.isPending || setWorker.isPending;
  const select = clsx(inputClass.replace("w-full", ""), "w-52");

  return (
    <div className="space-y-4">
      <Section title="Web server">
        <div className={clsx("space-y-4 text-sm", busy && "opacity-60")}>
          {(languageRuntime === "python" || languageRuntime === "java") && (
            <Row
              title={`${runtimeLabels[languageRuntime]} version`}
              hint={`Saved in the project's ${languageRuntime === "python" ? ".python-version" : ".java-version"}, so other tools (pyenv, jenv) read it too. python/java in the project's folder use it.`}
            >
              <select value={site.versionRequest?.version ?? ""} disabled={setLanguage.isPending} onChange={(e) => setLanguage.mutate(e.target.value || null)} className={select}>
                <option value="">Default · {settings?.versions[languageRuntime] ?? "none"}</option>
                {site.versionRequest && !languageVersions.includes(site.versionRequest.version) && (
                  <option value={site.versionRequest.version}>{site.versionRequest.version} (from {site.versionRequest.file})</option>
                )}
                {languageVersions.map((v) => (
                  <option key={v} value={v}>
                    {v}
                  </option>
                ))}
              </select>
            </Row>
          )}
          {(languageRuntime === "go" || languageRuntime === "rust") && (
            <Row
              title={`${runtimeLabels[languageRuntime]} version`}
              hint={languageRuntime === "go" ? "Set by the go and toolchain lines in go.mod." : "Set by rust-toolchain.toml; rustup installs that toolchain on first use."}
            >
              <span className="text-muted">
                {site.versionRequest ? `${site.versionRequest.version} (${site.versionRequest.file})` : `Default · ${settings?.versions[languageRuntime] ?? "none"}`}
              </span>
            </Row>
          )}
          {site.language === "php" && site.server === "frankenphp" && (
            <Row title="PHP version" hint="FrankenPHP runs its own built-in PHP, so the PHP versions and extensions installed in LampRabit don't apply here.">
              <span className="text-muted">FrankenPHP's PHP</span>
            </Row>
          )}
          {site.language === "php" && site.server !== "frankenphp" && (
            <Row title="PHP version" hint="Each site can use its own; the others keep theirs.">
              <select value={site.php ?? ""} disabled={busy} onChange={(e) => setPhp.mutate(e.target.value || null)} className={select}>
                <option value="">Default · {settings?.versions.php ?? "none"}</option>
                {phpVersions.map((v) => (
                  <option key={v} value={v}>
                    {v}
                  </option>
                ))}
              </select>
            </Row>
          )}
          {settings && settings.webServers.length > 1 && (
            <Row
              title="Web server"
              hint={`${runtimeLabels[settings.webServers[0]]}, the primary server, uses ports ${settings.ports.http}/${settings.ports.https}; the others their own. The other servers redirect this domain to the one you pick.`}
            >
              <select value={site.server} disabled={busy} onChange={(e) => setServer.mutate(e.target.value as Server)} className={select}>
                {settings.webServers.map((s) => (
                  <option key={s} value={s}>
                    {runtimeLabels[s]}
                  </option>
                ))}
              </select>
            </Row>
          )}
          {site.server === "frankenphp" && site.language === "php" && (
            <Row
              title="Worker mode"
              hint="FrankenPHP keeps the app in memory between requests, which is much faster. The app has to support it (Laravel Octane, Symfony Runtime…); after changing code, restart the workers."
            >
              <div className="flex shrink-0 items-center gap-3">
                <Switch checked={worker} onChange={(v) => setWorker.mutate(v)} label={`Worker mode for ${site.domain}`} />
                {worker && (
                  <Button size="sm" disabled={restartWorkers.isPending} onClick={() => restartWorkers.mutate()}>
                    <RotateCcw size={13} /> {restartWorkers.isPending ? "Restarting…" : "Restart workers"}
                  </Button>
                )}
              </div>
            </Row>
          )}
        </div>
      </Section>

      {site.nodeProject && app.data && (
        <Section
          title={site.language === "node" ? "Dev server" : "Asset dev server"}
          action={
            isActive(status) ? (
              <Button size="sm" variant="ghost" onClick={() => restart.mutate()}>
                Restart to apply
              </Button>
            ) : undefined
          }
        >
          <ServeSettings site={site} info={app.data} status={status} />
        </Section>
      )}

      {site.language === "php" && <PhpSettings site={site} settings={settings} />}
      <ShareSetup site={site} settings={settings} />
      <ProjectActions site={site} onRenamed={onRenamed} />
    </div>
  );
}

/** php.ini settings a site can change for itself; the backend checks the same rules. */
const SITE_INI: { key: string; label: string; hint: string; placeholder: string; pattern: RegExp }[] = [
  { key: "memory_limit", label: "Memory limit", hint: "Per request, like 512M; -1 for no limit.", placeholder: "512M", pattern: /^(-1|[1-9]\d*[KMG]?)$/ },
  { key: "upload_max_filesize", label: "Largest upload", hint: "Like 128M; post_max_size follows it.", placeholder: "128M", pattern: /^(-1|[1-9]\d*[KMG]?)$/ },
  { key: "max_execution_time", label: "Time limit", hint: "Seconds a request may run; 0 for none.", placeholder: "120", pattern: /^\d+$/ },
];

/**
 * This site's own PHP settings and Xdebug, without touching the other sites: written as a
 * [PATH=] section of php.ini, which PHP's FastCGI workers apply per folder.
 */
function PhpSettings({ site, settings }: { site: Site; settings?: Settings }) {
  const queryClient = useQueryClient();
  const notify = useAppStore((s) => s.notify);
  const own = settings?.sites[site.name];
  const savedValues = own?.phpIni ?? {};
  const savedXdebug = !!own?.xdebug;
  const savedKey = JSON.stringify([savedValues, savedXdebug]);
  const [values, setValues] = useState<Record<string, string>>(savedValues);
  const [xdebug, setXdebug] = useState(savedXdebug);
  useEffect(() => {
    const [v, x] = JSON.parse(savedKey) as [Record<string, string>, boolean];
    setValues(v);
    setXdebug(x);
  }, [savedKey]);
  const version = site.php ?? settings?.versions.php ?? null;
  const extensions = useQuery({ queryKey: ["php-extensions", version], queryFn: () => api.phpExtensions(version!), enabled: !!version });
  const xdebugOn = !!extensions.data?.find((e) => e.name === "xdebug")?.enabled;
  const franken = site.server === "frankenphp";

  const cleaned = Object.fromEntries(Object.entries(values).map(([k, v]) => [k, v.trim()]).filter(([, v]) => v));
  const dirty = JSON.stringify(cleaned) !== JSON.stringify(Object.fromEntries(Object.entries(savedValues).filter(([, v]) => v))) || xdebug !== savedXdebug;
  const invalid = SITE_INI.find((f) => cleaned[f.key] && !f.pattern.test(cleaned[f.key]));
  const save = useMutation({
    mutationFn: () => api.setSitePhpIni(site.name, cleaned, xdebug),
    onSuccess: () => {
      notify("info", `Saved ${site.name}'s PHP settings; PHP restarted with them.`);
      for (const key of ["settings", "sites", "services"]) queryClient.invalidateQueries({ queryKey: [key] });
    },
    onError: (e) => notify("error", String(e)),
  });
  const small = clsx(inputClass.replace("w-full", ""), "w-32 font-mono");

  return (
    <Section
      title="PHP for this site"
      action={
        dirty && (
          <span className="flex items-center gap-2">
            <Button size="sm" variant="ghost" onClick={() => (setValues(savedValues), setXdebug(savedXdebug))} disabled={save.isPending}>
              Cancel
            </Button>
            <Button size="sm" variant="primary" onClick={() => save.mutate()} disabled={save.isPending || !!invalid}>
              {save.isPending ? "Saving…" : "Save"}
            </Button>
          </span>
        )
      }
    >
      <div className={clsx("space-y-4 text-sm", franken && "opacity-60")}>
        {franken && <p className="text-xs text-warn">FrankenPHP runs its own PHP, so these apply only when the site uses Apache, Nginx or Caddy (Web server above).</p>}
        {SITE_INI.map((f) => (
          <Row key={f.key} title={f.label} hint={f.hint}>
            <input
              className={clsx(small, cleaned[f.key] && !f.pattern.test(cleaned[f.key]) && "border-danger")}
              value={values[f.key] ?? ""}
              placeholder={`${f.placeholder} (default)`}
              disabled={franken}
              onChange={(e) => setValues({ ...values, [f.key]: e.target.value })}
              aria-label={f.label}
            />
          </Row>
        ))}
        <Row title="Show errors" hint="PHP errors in the page (display_errors).">
          <select className={small} value={values.display_errors ?? ""} disabled={franken} onChange={(e) => setValues({ ...values, display_errors: e.target.value })}>
            <option value="">Default (On)</option>
            <option value="On">On</option>
            <option value="Off">Off</option>
          </select>
        </Row>
        <Row
          title="Debug every request (Xdebug)"
          hint={
            xdebugOn
              ? "Xdebug starts without XDEBUG_TRIGGER, for this site only. Your IDE listens on port 9003."
              : `Turn on Xdebug for PHP ${version ?? ""} first: Services → PHP → Extensions.`
          }
        >
          <Switch checked={xdebug} onChange={setXdebug} disabled={franken || (!xdebugOn && !xdebug)} label="Debug every request with Xdebug" />
        </Row>
        {invalid && <p className="text-xs text-danger-text">{invalid.label}: {invalid.hint}</p>}
      </div>
    </Section>
  );
}

/** What applying a lamprabit.toml sets, one line each, for its preview. */
function fileLines(file: ProjectFile, site: Site): [string, string][] {
  const target = (t: DomainTarget) =>
    t.kind === "main" ? "wherever the main domain goes" : t.kind === "files" ? `files in ${t.root || "the web folder"}` : t.kind === "port" ? `port ${t.port}` : `process ${t.name}`;
  const lines: [string, string][] = [];
  if (file.php) lines.push(["PHP", file.php]);
  if (file.server) lines.push(["Web server", serviceLabels[file.server]]);
  if (file.ssl != null) lines.push(["HTTPS", file.ssl ? "On" : "Off"]);
  for (const [k, v] of Object.entries(file.phpIni ?? {})) lines.push([`php.ini ${k}`, v]);
  if (file.main) lines.push([site.domain, target(file.main)]);
  for (const d of file.domains ?? []) lines.push([`${d.name}.${site.domain}`, target(d.target)]);
  for (const p of file.processes ?? []) lines.push([`Process ${p.name}`, p.command]);
  return lines;
}

/**
 * The project's setup as lamprabit.toml in its folder, to commit: saved from here, and
 * applied only after a look at what it sets (processes are commands from the repository).
 */
function ShareSetup({ site, settings }: { site: Site; settings?: Settings }) {
  const queryClient = useQueryClient();
  const notify = useAppStore((s) => s.notify);
  const onError = (e: unknown) => notify("error", String(e));
  const [previewing, setPreviewing] = useState(false);
  const file = useQuery({ queryKey: ["project-file", site.name], queryFn: () => api.projectFile(site.name) });
  const save = useMutation({
    mutationFn: () => api.saveProjectFile(site.name),
    onSuccess: () => {
      notify("info", `Saved ${site.name}'s setup to lamprabit.toml`);
      file.refetch();
    },
    onError,
  });
  const apply = useMutation({
    mutationFn: async (f: ProjectFile) => {
      const own = settings?.sites[site.name];
      // Through the same commands as editing by hand, so each part gets the same checks.
      if (f.php !== undefined) await api.setSitePhp(site.name, f.php);
      if (f.server) await api.setSiteServer(site.name, f.server);
      if (f.ssl != null) await api.setSiteSsl(site.name, f.ssl);
      if (f.phpIni) await api.setSitePhpIni(site.name, f.phpIni, !!own?.xdebug);
      // Processes before domains: a subdomain may point at one.
      if (f.processes) await api.setSiteProcesses(site.name, f.processes);
      await api.setSiteMain(site.name, f.main && f.main.kind !== "main" ? f.main : null);
      if (f.domains) await api.setSiteDomains(site.name, f.domains.map((d) => ({ host: `${d.name}.${site.domain}`, target: d.target })));
    },
    onSuccess: () => {
      notify("info", `Applied ${site.name}'s lamprabit.toml`);
      setPreviewing(false);
      for (const key of ["settings", "sites", "services", "project-file"]) queryClient.invalidateQueries({ queryKey: [key] });
    },
    onError: (e) => {
      onError(e);
      queryClient.invalidateQueries({ queryKey: ["settings"] });
    },
  });
  const info = file.data;

  return (
    <Section title="Share this setup">
      <div className="space-y-3 text-sm">
        <Row
          title="lamprabit.toml"
          hint={
            !info
              ? "Save the domains, processes and PHP settings into the project, so whoever clones the repository gets the same setup."
              : info.matches
                ? "The project's file matches this setup."
                : "The project's file differs from this setup: apply it here, or overwrite it with this setup."
          }
        >
          <span className="flex shrink-0 items-center gap-2">
            {info && !info.matches && (
              <Button variant="primary" onClick={() => setPreviewing(true)}>
                Apply it…
              </Button>
            )}
            <Button onClick={() => save.mutate()} disabled={save.isPending || (!!info && info.matches)}>
              {info ? "Save this setup to it" : "Save to lamprabit.toml"}
            </Button>
          </span>
        </Row>
        {file.error && <p className="text-xs text-danger-text">{String(file.error)}</p>}
        <Row title="As a template" hint="A template that recreates this project (its repository, installs, database, processes and domains), to start the next one the same way. It opens in the editor first.">
          <Button
            onClick={() =>
              api.templateFromProject(site.name).then(
                (yaml) => useAppStore.getState().openTemplateEditor({ yaml }),
                onError,
              )
            }
          >
            Save as a template…
          </Button>
        </Row>
      </div>
      {previewing && info && (
        <Dialog
          open
          onClose={() => !apply.isPending && setPreviewing(false)}
          title="Apply lamprabit.toml?"
          footer={
            <>
              <Button onClick={() => setPreviewing(false)} disabled={apply.isPending}>
                Cancel
              </Button>
              <Button variant="primary" onClick={() => apply.mutate(info.file)} disabled={apply.isPending}>
                {apply.isPending ? "Applying…" : "Apply"}
              </Button>
            </>
          }
          width="max-w-xl"
        >
          <div className="space-y-3 text-sm">
            <p className="text-muted">It replaces this project's settings with these. Processes are commands from the repository: check them before applying.</p>
            <dl className="divide-y divide-line rounded-lg border border-line">
              {fileLines(info.file, site).map(([k, v], i) => (
                <div key={i} className="flex gap-3 px-3 py-1.5">
                  <dt className="w-40 shrink-0 truncate text-xs text-muted">{k}</dt>
                  <dd className="min-w-0 flex-1 font-mono text-xs break-all">{v}</dd>
                </div>
              ))}
              {fileLines(info.file, site).length === 0 && <p className="px-3 py-2 text-xs text-muted">The file is empty: applying it clears the main domain's choice.</p>}
            </dl>
          </div>
        </Dialog>
      )}
    </Section>
  );
}

/** Renaming and deleting the project. */
function ProjectActions({ site, onRenamed }: { site: Site; onRenamed: (path: string) => void }) {
  const queryClient = useQueryClient();
  const notify = useAppStore((s) => s.notify);
  const [name, setName] = useState(site.name);
  const [deleting, setDeleting] = useState(false);
  const refresh = () => {
    for (const key of ["sites", "settings", "services"]) queryClient.invalidateQueries({ queryKey: [key] });
  };
  const rename = useMutation({
    mutationFn: (next: string) => api.renameProject(site.name, next),
    onSuccess: (_, next) => {
      notify("info", `Renamed ${site.name} to ${next}`);
      onRenamed(site.path.slice(0, site.path.length - site.name.length) + next);
      refresh();
    },
    onError: (e) => notify("error", String(e)),
  });
  const next = name.trim();
  const domain = site.domain.replace(/^[^.]+/, next.toLowerCase().replace(/[^a-z0-9-]+/g, "-"));

  return (
    <Section title="Project">
      <div className="space-y-4 text-sm">
        <Row title="Rename" hint={next && next !== site.name ? `The folder becomes ${next} and the site ${domain}. Its database keeps its name.` : "Renames the folder and the domain. Everything running for it stops first."}>
          <form
            className="flex items-center gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              if (next && next !== site.name) rename.mutate(next);
            }}
          >
            <input className={clsx(inputClass.replace("w-full", ""), "w-48 font-mono")} value={name} onChange={(e) => setName(e.target.value)} aria-label="New name" />
            <Button type="submit" disabled={!next || next === site.name || rename.isPending}>
              {rename.isPending ? "Renaming…" : "Rename"}
            </Button>
          </form>
        </Row>
        <Row title="Delete" hint="Moves the folder to the Recycle Bin and removes its domains and settings.">
          <Button variant="danger" onClick={() => setDeleting(true)}>
            <Trash2 size={14} /> Delete…
          </Button>
        </Row>
      </div>
      {deleting && <DeleteProjectDialog site={site} onClose={() => setDeleting(false)} onDeleted={refresh} />}
    </Section>
  );
}

function DeleteProjectDialog({ site, onClose, onDeleted }: { site: Site; onClose: () => void; onDeleted: () => void }) {
  const notify = useAppStore((s) => s.notify);
  const [typed, setTyped] = useState("");
  const [dropDb, setDropDb] = useState(false);
  const remove = useMutation({
    mutationFn: () => api.deleteProject(site.name, dropDb),
    onSuccess: () => {
      notify("info", `Deleted ${site.name}; its folder is in the Recycle Bin.`);
      onClose();
      onDeleted();
    },
    onError: (e) => notify("error", String(e)),
  });
  const confirmed = typed.trim() === site.name;
  return (
    <Dialog
      open
      onClose={() => !remove.isPending && onClose()}
      title={`Delete ${site.name}?`}
      icon={<Trash2 size={18} />}
      footer={
        <>
          <Button onClick={onClose} disabled={remove.isPending}>
            Cancel
          </Button>
          <Button variant="danger" onClick={() => remove.mutate()} disabled={!confirmed || remove.isPending}>
            {remove.isPending ? "Deleting…" : "Delete project"}
          </Button>
        </>
      }
    >
      <div className="space-y-3 text-sm">
        <p className="text-muted">
          Its folder goes to the Recycle Bin (you can restore it from there), and {site.domain}, its subdomains, processes and settings are removed. Anything running for it stops.
        </p>
        {site.database && (
          <label className="flex items-start gap-2">
            <input type="checkbox" className="mt-1" checked={dropDb} onChange={(e) => setDropDb(e.target.checked)} />
            <span>
              Also drop the database <span className="font-mono">{site.database}</span>
              <span className="block text-xs text-danger-text">This can't be undone; make a backup first (Services → database → Backups) if you may need it.</span>
            </span>
          </label>
        )}
        <label className="block">
          <span className="mb-1 block text-xs font-semibold text-muted">
            Type <span className="font-mono text-fg">{site.name}</span> to confirm
          </span>
          <input className={clsx(inputClass, "font-mono")} value={typed} onChange={(e) => setTyped(e.target.value)} data-autofocus />
        </label>
      </div>
    </Dialog>
  );
}

/**
 * Commands and processes in one place: the project's commands next to their output, so a
 * run's result shows without scrolling, and the processes that keep running below.
 */
function CommandsTab({ site, settings, status }: { site: Site; settings?: Settings; status?: AppStatus }) {
  const [offer, setOffer] = useState<ProjectProcess | null>(null);
  // Whose output the terminal shows: the project's commands, or one of its processes.
  const [showing, setShowing] = useState(site.name);
  const unread = useUnreadOutput(site.name, showing);
  const statuses = useAppStatuses();
  const processes = settings?.sites[site.name]?.processes ?? [];
  const keys = processes.map((p) => `${site.name}/${p.name}`);
  // A removed process's tab falls back to the commands.
  const shown = showing === site.name || keys.includes(showing) ? showing : site.name;

  const tab = (key: string, label: string, state?: AppStatus["state"]) => (
    <button
      key={key}
      type="button"
      onClick={() => setShowing(key)}
      className={clsx("flex shrink-0 items-center gap-1.5 rounded px-2 py-1 text-xs font-medium", shown === key ? "bg-white/10 text-slate-100" : "text-slate-400 hover:text-slate-200")}
    >
      {state && <span className={clsx("size-1.5 rounded-full", stateStyle[state].dot)} aria-hidden />}
      {label}
      {!!unread[key] && shown !== key && <span className="rounded-full bg-accent px-1.5 text-[9px] leading-4 font-bold text-accent-fg">{unread[key] > 99 ? "99+" : unread[key]}</span>}
    </button>
  );

  return (
    <div className="space-y-4">
      <div className="grid grid-cols-1 items-start gap-4 lg:grid-cols-2">
        <Card className="min-w-0 p-4">
          <CommandsPanel site={site} status={status} onKeepRunning={(c) => setOffer(processFrom(c.name, c.command))} />
        </Card>
        <OutputPanel
          key={shown}
          name={shown}
          height="h-[27rem]"
          title={
            <span className="scrollbar-none flex min-w-0 flex-1 items-center gap-1 overflow-x-auto">
              {tab(site.name, "Commands")}
              {processes.map((p) => tab(`${site.name}/${p.name}`, p.name, statuses[`${site.name}/${p.name}`]?.state ?? "stopped"))}
            </span>
          }
          footer={shown === site.name ? <CommandPrompt site={site} status={status} /> : undefined}
        />
      </div>
      <div className="grid grid-cols-1 items-start gap-4 lg:grid-cols-2">
        <ProjectProcesses site={site} settings={settings} offer={offer} onOfferTaken={() => setOffer(null)} showing={shown} unread={unread} onShowOutput={setShowing} />
        <RecentRuns site={site} status={status} onShow={() => setShowing(site.name)} />
      </div>
    </div>
  );
}

const duration = (ms: number) => (ms < 1000 ? `${(ms / 1000).toFixed(1)}s` : ms < 60_000 ? `${Math.round(ms / 1000)}s` : `${Math.floor(ms / 60_000)}m ${Math.round((ms % 60_000) / 1000)}s`);

/** The project's last command runs: how each ended and how long it took, one click to run it again. */
function RecentRuns({ site, status, onShow }: { site: Site; status?: AppStatus; onShow: () => void }) {
  const queryClient = useQueryClient();
  const notify = useAppStore((s) => s.notify);
  const now = useNow();
  const history = useQuery({ queryKey: ["command-history", site.name], queryFn: () => api.commandHistory(site.name) });
  const running = status?.task ?? null;
  // A run just ended: it's in the history now.
  const wasRunning = useRef(running);
  useEffect(() => {
    if (wasRunning.current && !running) queryClient.invalidateQueries({ queryKey: ["command-history", site.name] });
    wasRunning.current = running;
  }, [running, site.name, queryClient]);
  const again = useMutation({
    mutationFn: (r: CommandRun) => api.runProjectCommand(site.name, r.label, r.command),
    onSuccess: onShow,
    onError: (e) => notify("error", String(e)),
  });
  const runs = (history.data ?? []).slice(0, 8);

  return (
    <Card className="overflow-hidden">
      <div className="border-b border-line px-5 py-3">
        <h3 className="text-sm font-bold">Recent runs</h3>
        <p className="text-xs text-muted">How the last commands ended, kept after the output is cleared.</p>
      </div>
      {runs.length === 0 ? (
        <p className="px-5 py-3 text-xs text-muted">Nothing run yet.</p>
      ) : (
        <ul className="divide-y divide-line">
          {runs.map((r) => {
            const ok = r.exitCode === 0;
            return (
              <li key={`${r.started}`} className="group flex items-center gap-3 px-5 py-2">
                <span
                  className={clsx("flex size-5 shrink-0 items-center justify-center rounded-full", ok ? "bg-ok/10 text-ok" : r.exitCode == null ? "bg-surface-2 text-muted" : "bg-danger/10 text-danger-text")}
                  title={ok ? "Finished" : r.exitCode == null ? "Stopped" : `Failed with exit code ${r.exitCode}`}
                >
                  {ok ? <Check size={11} /> : r.exitCode == null ? <Square size={8} className="fill-current" /> : <X size={11} />}
                </span>
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-sm font-medium">{r.label}</span>
                  <span className="block truncate font-mono text-[11px] text-muted" title={r.command}>
                    {r.command}
                  </span>
                </span>
                <span className="shrink-0 text-right text-[11px] text-muted">
                  <span className="block">{r.exitCode != null && !ok ? `exit ${r.exitCode} · ` : ""}{duration(r.durationMs)}</span>
                  <span className="block" title={new Date(r.started).toLocaleString()}>
                    {formatAgo(r.started, now)}
                  </span>
                </span>
                <Button size="sm" variant="ghost" onClick={() => again.mutate(r)} disabled={!!running || again.isPending} aria-label={`Run ${r.label} again`} title="Run again">
                  <RotateCcw size={13} />
                </Button>
              </li>
            );
          })}
        </ul>
      )}
    </Card>
  );
}

/** phpMyAdmin opened on the project's database, on the default database server. */
function databaseUrl(db: string, info?: SitesInfo, settings?: Settings) {
  const base = toolUrl("phpmyadmin", info, settings?.ports.mailUi);
  return base ? `${base}?server=1&db=${encodeURIComponent(db)}` : null;
}
