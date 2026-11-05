import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ChevronDown, Cpu, Pencil, Play, Plus, RotateCw, Sparkles, Square, SquareTerminal, Trash2 } from "lucide-react";
import clsx from "clsx";
import { api, type ProjectProcess, type RestartPolicy, type Settings, type Site } from "../lib/api";
import { useAppStore } from "../lib/store";
import Dialog from "./Dialog";
import { FolderSelect } from "./DomainTargetPicker";
import { AppState, isActive, useAppStatuses } from "./ProjectRun";
import { Button, Card, inputClass, Switch } from "./ui";

/** What a process goes by in statuses and output: "shop/queue". */
export const processKey = (project: string, process: string) => `${project}/${process}`;

const blank: ProjectProcess = { name: "", command: "", cwd: "", port: null, env: {}, restart: "on-failure", withApp: true, atLaunch: false, schedule: "" };

/** A command from the Commands list as a new process that keeps running ("queue:work" → queue-work). */
export function processFrom(name: string, command: string): ProjectProcess {
  const slug = name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "").slice(0, 40) || "process";
  return { ...blank, name: slug, command };
}

/** Common long-running commands for the project's framework, offered as one click. */
function presets(site: Site): ProjectProcess[] {
  const p = (name: string, command: string, port: number | null = null): ProjectProcess => ({ ...blank, name, command, port });
  const list: ProjectProcess[] = [];
  if (site.framework === "Laravel") {
    list.push(p("queue", "php artisan queue:work --tries=3"), p("scheduler", "php artisan schedule:work"));
  }
  if (site.framework === "Symfony") list.push(p("messenger", "php bin/console messenger:consume async -vv"));
  // PHP projects' package.json dev script is their asset server (Vite).
  if (site.nodeProject && site.language === "php") list.push(p("vite", "npm run dev"));
  return list;
}

const policies: { value: RestartPolicy; label: string }[] = [
  { value: "on-failure", label: "Restart after a crash" },
  { value: "always", label: "Restart whenever it exits" },
  { value: "never", label: "Don't restart" },
];

const envText = (env: Record<string, string>) =>
  Object.entries(env)
    .map(([k, v]) => `${k}=${v}`)
    .join("\n");
const envFrom = (text: string) =>
  Object.fromEntries(
    text
      .split("\n")
      .map((l) => l.trim())
      .filter((l) => l && !l.startsWith("#") && l.includes("="))
      .map((l) => [l.slice(0, l.indexOf("=")).trim(), l.slice(l.indexOf("=") + 1).trim()]),
  );

const narrow = (width: string) => clsx(inputClass.replace("w-full", ""), width);
const fieldLabel = "mb-1 block text-xs font-semibold text-muted";

/** When a process starts by itself; stored as withApp, atLaunch and schedule. */
type Start = "manual" | "project" | "launch" | "schedule";

const starts: { value: Start; label: string }[] = [
  { value: "project", label: "With the project" },
  { value: "launch", label: "When LampRabit starts" },
  { value: "schedule", label: "On a schedule (cron)" },
  { value: "manual", label: "Only by hand" },
];

const startOf = (p: ProjectProcess): Start => (p.schedule.trim() ? "schedule" : p.atLaunch ? "launch" : p.withApp ? "project" : "manual");

/** Common schedules, as one click. */
const schedulePresets: [string, string][] = [
  ["Every minute", "* * * * *"],
  ["Every 15 min", "*/15 * * * *"],
  ["Hourly", "0 * * * *"],
  ["Daily 3:00", "0 3 * * *"],
  ["Weekdays 9:00", "0 9 * * 1-5"],
];

/** Short words for a process's start, shown on its row. */
export function startLabel(p: ProjectProcess): string | null {
  switch (startOf(p)) {
    case "project":
      return "with the project";
    case "launch":
      return "at launch";
    case "schedule":
      return `cron ${p.schedule.trim()}`;
    case "manual":
      return null;
  }
}

/** "at 03:00" today, "Tue 09:00" within a week, else a date. */
function nextLabel(ms: number): string {
  const at = new Date(ms);
  const time = at.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  const days = (new Date(at).setHours(0, 0, 0, 0) - new Date().setHours(0, 0, 0, 0)) / 86_400_000;
  if (days === 0) return `at ${time}`;
  if (days === 1) return `tomorrow ${time}`;
  if (days < 7) return `${at.toLocaleDateString([], { weekday: "short" })} ${time}`;
  return `${at.toLocaleDateString([], { month: "short", day: "numeric" })} ${time}`;
}

/** Why a process can't be saved yet, or null. */
function problem(p: ProjectProcess, others: ProjectProcess[], start: Start, needsPort: boolean): string | null {
  if (!p.name) return "Give it a name, like queue or vite.";
  if (!/^[a-z0-9][a-z0-9-]*$/.test(p.name)) return "Names use lowercase letters, digits and dashes.";
  if (others.some((o) => o.name === p.name)) return `There's already a process called ${p.name}.`;
  if (!p.command.trim()) return "Enter the command it runs.";
  if (start === "schedule" && p.schedule.trim().split(/\s+/).length !== 5) return "A schedule has five fields: minute hour day month weekday.";
  if (needsPort && p.port == null) return "Enter a port, or pick a free one.";
  if (p.port != null && !(p.port >= 1024 && p.port <= 65535)) return "Ports go from 1024 to 65535.";
  if (p.port != null && others.some((o) => o.port === p.port)) return `Port ${p.port} is another process's.`;
  return null;
}

/** Adding or editing one process, in a small dialog: the essentials, with the folder and environment under a toggle. */
function ProcessDialog({ site, initial, others, busy, onSave, onClose, onDelete }: {
  site: Site;
  initial: ProjectProcess;
  others: ProjectProcess[];
  busy: boolean;
  onSave: (p: ProjectProcess) => void;
  onClose: () => void;
  onDelete?: () => void;
}) {
  const notify = useAppStore((s) => s.notify);
  const [p, setP] = useState(initial);
  const [env, setEnv] = useState(envText(initial.env));
  const [more, setMore] = useState(!!initial.cwd || Object.keys(initial.env).length > 0);
  const [start, setStart] = useState<Start>(startOf(initial));
  const [needsPort, setNeedsPort] = useState(initial.port != null);
  const set = (patch: Partial<ProjectProcess>) => setP((cur) => ({ ...cur, ...patch }));
  const issue = problem(p, others, start, needsPort);
  const freePort = () => api.appFreePort().then((port) => set({ port }), (e) => notify("error", String(e)));
  const save = () => {
    if (issue) return;
    onSave({
      ...p,
      env: envFrom(env),
      port: needsPort ? p.port : null,
      withApp: start === "project",
      atLaunch: start === "launch",
      schedule: start === "schedule" ? p.schedule.trim().replace(/\s+/g, " ") : "",
    });
  };

  return (
    <Dialog
      open
      onClose={() => !busy && onClose()}
      title={onDelete ? `Edit ${initial.name}` : "Keep a command running"}
      icon={<Cpu size={18} />}
      footer={
        <>
          {onDelete && (
            <Button variant="ghost" onClick={onDelete} disabled={busy} className="mr-auto text-danger-text">
              <Trash2 size={14} /> Delete
            </Button>
          )}
          <Button onClick={onClose} disabled={busy}>
            Cancel
          </Button>
          <Button variant="primary" onClick={save} disabled={busy || !!issue}>
            {busy ? "Saving…" : "Save"}
          </Button>
        </>
      }
    >
      <form
        className="space-y-3"
        onSubmit={(e) => {
          e.preventDefault();
          save();
        }}
      >
        <label className="block">
          <span className={fieldLabel}>Name</span>
          <input
            className={clsx(inputClass, "font-mono")}
            value={p.name}
            placeholder="queue"
            onChange={(e) => set({ name: e.target.value.toLowerCase().replace(/[^a-z0-9-]/g, "-") })}
            data-autofocus={!initial.name ? true : undefined}
          />
        </label>
        <label className="block">
          <span className={fieldLabel}>Command</span>
          <input className={clsx(inputClass, "font-mono")} value={p.command} placeholder="php artisan queue:work" onChange={(e) => set({ command: e.target.value })} />
        </label>
        <div className="grid gap-3 sm:grid-cols-2">
          <label className="block">
            <span className={fieldLabel}>Starts</span>
            <select
              className={inputClass}
              value={start}
              onChange={(e) => {
                const next = e.target.value as Start;
                setStart(next);
                if (next === "schedule" && !p.schedule.trim()) set({ schedule: "0 * * * *" });
              }}
            >
              {starts.map((o) => (
                <option key={o.value} value={o.value}>
                  {o.label}
                </option>
              ))}
            </select>
          </label>
          <label className="block">
            <span className={fieldLabel}>When it exits</span>
            <select className={inputClass} value={p.restart} onChange={(e) => set({ restart: e.target.value as RestartPolicy })}>
              {policies.map((o) => (
                <option key={o.value} value={o.value}>
                  {o.label}
                </option>
              ))}
            </select>
          </label>
        </div>
        {start === "schedule" && (
          <div className="space-y-1.5 rounded-lg border border-line bg-surface-2/40 p-3">
            <input
              className={clsx(inputClass, "font-mono")}
              value={p.schedule}
              onChange={(e) => set({ schedule: e.target.value })}
              placeholder="0 3 * * *"
              aria-label="Schedule (crontab)"
            />
            <p className="text-[11px] text-muted">minute · hour · day · month · weekday. Started at those times unless it's already running; LampRabit has to be open.</p>
            <div className="flex flex-wrap gap-1.5">
              {schedulePresets.map(([label, expr]) => (
                <button
                  key={expr}
                  type="button"
                  onClick={() => set({ schedule: expr })}
                  className={clsx("rounded-md border px-2 py-0.5 text-xs", p.schedule.trim() === expr ? "border-accent bg-accent-soft text-accent-text" : "border-line text-muted hover:text-fg")}
                >
                  {label}
                </button>
              ))}
            </div>
          </div>
        )}
        {start === "launch" && <p className="-mt-1 text-[11px] text-muted">With "Start LampRabit when I sign in" (Settings), it runs from sign-in.</p>}

        <div className="flex min-h-9 flex-wrap items-center gap-x-3 gap-y-2">
          <span className="flex items-center gap-2 text-sm">
            <Switch
              checked={needsPort}
              onChange={(v) => {
                setNeedsPort(v);
                if (v && p.port == null) freePort();
              }}
              label="It serves on a port"
            />
            It serves on a port
          </span>
          {needsPort && (
            <span className="ml-auto flex items-center gap-1.5">
              <input
                type="number"
                min={1024}
                max={65535}
                className={narrow("w-24 font-mono")}
                value={p.port ?? ""}
                onChange={(e) => set({ port: e.target.value ? Number(e.target.value) : null })}
                aria-label="Port"
              />
              <Button type="button" size="sm" variant="ghost" onClick={freePort}>
                Pick free
              </Button>
            </span>
          )}
        </div>

        <button type="button" onClick={() => setMore(!more)} className="flex items-center gap-1 text-xs font-medium text-muted hover:text-fg" aria-expanded={more}>
          <ChevronDown size={13} className={clsx("transition-transform", !more && "-rotate-90")} /> Folder and environment
        </button>
        {more && (
          <div className="space-y-3">
            <label className="block">
              <span className={fieldLabel}>Runs in</span>
              <FolderSelect project={site.name} value={p.cwd} onChange={(cwd) => set({ cwd })} none="Project folder" label="Folder it runs in" />
            </label>
            <label className="block">
              <span className={fieldLabel}>Environment (KEY=value per line; the port is also $PORT and {"{port}"})</span>
              <textarea className={clsx(inputClass, "min-h-14 font-mono")} rows={2} value={env} onChange={(e) => setEnv(e.target.value)} placeholder="QUEUE_CONNECTION=database" />
            </label>
          </div>
        )}

        <p className={clsx("text-xs", issue ? "text-danger-text" : "text-muted")}>{issue ?? "Saving doesn't restart it; use Restart to apply changes."}</p>
        {/* Enter in a field saves. */}
        <button type="submit" hidden />
      </form>
    </Dialog>
  );
}

/**
 * A project's processes: queue workers, schedulers, Vite next to PHP, an API. Each runs in
 * a folder of the project with its Node and LampRabit's commands, can start with the
 * project, restarts after a crash if asked, and has its own output. One with a port can
 * get a subdomain (Domains). Added and edited in a dialog, so the list stays compact.
 */
export default function ProjectProcesses({ site, settings, offer, onOfferTaken, showing, unread = {}, onShowOutput }: {
  site: Site;
  settings?: Settings;
  /** A command picked in the Commands list to keep running: opens as a new process. */
  offer?: ProjectProcess | null;
  onOfferTaken?: () => void;
  /** The output shown next to the commands ("shop/queue"), and new lines per process. */
  showing?: string;
  unread?: Record<string, number>;
  onShowOutput?: (key: string) => void;
}) {
  const queryClient = useQueryClient();
  const notify = useAppStore((s) => s.notify);
  const statuses = useAppStatuses();
  const own = settings?.sites[site.name];
  const saved = own?.processes ?? [];
  // The process in the dialog: an existing one's name, or a new one's starting values.
  const [editing, setEditing] = useState<string | ProjectProcess | null>(null);
  const onError = (e: unknown) => notify("error", String(e));
  const scheduled = saved.some((p) => p.schedule.trim());
  const nextRuns = useQuery({
    queryKey: ["process-next-runs", site.name, saved.map((p) => p.schedule).join("|")],
    queryFn: () => api.processNextRuns(site.name),
    enabled: scheduled,
    refetchInterval: 60_000,
  });

  const save = useMutation({
    mutationFn: (list: ProjectProcess[]) => api.setSiteProcesses(site.name, list),
    onSuccess: () => {
      setEditing(null);
      queryClient.invalidateQueries({ queryKey: ["settings"] });
    },
    onError,
  });
  const act = useMutation({
    mutationFn: async ({ name, what }: { name: string; what: "start" | "stop" | "restart" }) => {
      if (what === "stop") await api.processStop(site.name, name);
      else await (what === "start" ? api.processStart(site.name, name) : api.processRestart(site.name, name));
    },
    onError,
  });
  const all = useMutation({ mutationFn: (start: boolean) => api.processesAll(site.name, start), onError });

  useEffect(() => {
    if (!offer) return;
    // A taken name gets a number, so the dialog opens valid.
    let name = offer.name;
    for (let n = 2; saved.some((s) => s.name === name); n++) name = `${offer.name}-${n}`;
    setEditing({ ...offer, name });
    onOfferTaken?.();
  }, [offer]);

  const offers = presets(site).filter((p) => !saved.some((s) => s.name === p.name));
  const anyRunning = saved.some((p) => isActive(statuses[processKey(site.name, p.name)]));
  // Subdomains that point at each process, shown on its row.
  const domainsOf = (name: string) => (own?.domains ?? []).filter((d) => d.target.kind === "process" && d.target.name === name).map((d) => d.host);
  const edited = typeof editing === "string" ? saved.find((p) => p.name === editing) : null;

  return (
    <Card className="overflow-hidden">
      <div className="flex flex-wrap items-center gap-3 border-b border-line px-5 py-3">
        <div className="min-w-0 flex-1">
          <h3 className="text-sm font-bold">Keep running</h3>
          <p className="text-xs text-muted">Queue workers, schedulers, Vite, an API: restarted if they crash.</p>
        </div>
        {saved.length > 0 && (
          <Button size="sm" variant="ghost" onClick={() => all.mutate(!anyRunning)} disabled={all.isPending}>
            {anyRunning ? <Square size={11} className="fill-current" /> : <Play size={12} className="fill-current" />} {anyRunning ? "Stop all" : "Start all"}
          </Button>
        )}
        <Button size="sm" variant="soft" onClick={() => setEditing({ ...blank })}>
          <Plus size={14} /> Add
        </Button>
      </div>

      {saved.length === 0 ? (
        <p className="px-5 py-3 text-xs text-muted">Nothing yet. Add one, or use ∞ on a command above to keep it running.</p>
      ) : (
        <ul className="divide-y divide-line">
          {saved.map((p) => {
            const key = processKey(site.name, p.name);
            const status = statuses[key];
            const active = isActive(status);
            const domains = domainsOf(p.name);
            const next = nextRuns.data?.[p.name];
            return (
              <li key={p.name}>
                <div className="flex items-center gap-3 px-5 py-2.5">
                  <AppState status={status} className="shrink-0" />
                  <div className="min-w-0 flex-1">
                    <div className="flex min-w-0 items-center gap-2">
                      <span className="text-sm font-semibold">{p.name}</span>
                      {p.port != null && <span className="font-mono text-[11px] text-muted">:{p.port}</span>}
                      {startLabel(p) && (
                        <span className="shrink-0 rounded bg-surface-2 px-1.5 py-0.5 text-[11px] text-muted" title={next ? `Next run ${new Date(next).toLocaleString()}` : undefined}>
                          {next ? `next ${nextLabel(next)}` : startLabel(p)}
                        </span>
                      )}
                      {domains.map((d) => (
                        <span key={d} className="truncate rounded bg-accent-soft px-1.5 py-0.5 font-mono text-[11px] text-accent-text">
                          {d}
                        </span>
                      ))}
                    </div>
                    <div className="truncate font-mono text-[11px] text-muted" title={p.command}>
                      {p.command}
                    </div>
                  </div>
                  {active ? (
                    <>
                      <Button size="sm" variant="ghost" onClick={() => act.mutate({ name: p.name, what: "restart" })} aria-label={`Restart ${p.name}`} title="Restart">
                        <RotateCw size={14} />
                      </Button>
                      <Button size="sm" onClick={() => act.mutate({ name: p.name, what: "stop" })} aria-label={`Stop ${p.name}`}>
                        <Square size={11} className="fill-current" /> Stop
                      </Button>
                    </>
                  ) : (
                    <Button size="sm" variant="soft" onClick={() => act.mutate({ name: p.name, what: "start" })} aria-label={`${next ? "Run" : "Start"} ${p.name} now`}>
                      <Play size={12} className="fill-current" /> {next ? "Run now" : "Start"}
                    </Button>
                  )}
                  <Button size="sm" variant="ghost" onClick={() => setEditing(p.name)} aria-label={`Edit ${p.name}`} title="Edit">
                    <Pencil size={14} />
                  </Button>
                  <button
                    type="button"
                    onClick={() => onShowOutput?.(key)}
                    className={clsx("relative flex size-8 shrink-0 items-center justify-center rounded-lg hover:bg-surface-2", showing === key ? "text-accent-text" : "text-muted hover:text-fg")}
                    aria-label={`Show ${p.name}'s output${unread[key] ? ` (${unread[key]} new lines)` : ""}`}
                    title="Show its output"
                  >
                    <SquareTerminal size={15} />
                    {!!unread[key] && (
                      <span className="absolute -top-0.5 -right-0.5 min-w-4 rounded-full bg-accent px-1 text-center text-[9px] leading-4 font-bold text-accent-fg">{unread[key] > 99 ? "99+" : unread[key]}</span>
                    )}
                  </button>
                </div>
              </li>
            );
          })}
        </ul>
      )}

      {offers.length > 0 && (
        <div className="flex flex-wrap items-center gap-2 border-t border-line px-5 py-2">
          <span className="flex items-center gap-1.5 text-xs font-semibold text-muted">
            <Sparkles size={13} aria-hidden /> Suggested
          </span>
          {offers.map((o) => (
            <Button key={o.name} size="sm" variant="ghost" onClick={() => setEditing(o)} title={o.command}>
              <Plus size={13} /> {o.name}
            </Button>
          ))}
        </div>
      )}

      {edited && (
        <ProcessDialog
          site={site}
          initial={edited}
          others={saved.filter((o) => o.name !== edited.name)}
          busy={save.isPending}
          onSave={(next) => save.mutate(saved.map((o) => (o.name === edited.name ? next : o)))}
          onClose={() => setEditing(null)}
          onDelete={() => save.mutate(saved.filter((o) => o.name !== edited.name))}
        />
      )}
      {editing && typeof editing === "object" && (
        <ProcessDialog site={site} initial={editing} others={saved} busy={save.isPending} onSave={(next) => save.mutate([...saved, next])} onClose={() => setEditing(null)} />
      )}
    </Card>
  );
}
