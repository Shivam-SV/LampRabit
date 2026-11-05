import { useEffect, useMemo, useState, type ReactNode } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { openPath, openUrl } from "@tauri-apps/plugin-opener";
import {
  CheckCircle2,
  CircleSlash,
  Database,
  ExternalLink,
  Feather,
  FolderInput,
  FolderOpen,
  Layers,
  LifeBuoy,
  Mail,
  ScrollText,
  Monitor,
  Moon,
  Palette,
  Plug,
  Power,
  RefreshCw,
  ServerCog,
  Settings2,
  Share2,
  Sparkles,
  Stethoscope,
  Sun,
  SquareTerminal,
  Trash2,
  Wind,
} from "lucide-react";
import clsx from "clsx";
import { allServers, api, runtimeLabels, type EnvPaths, type PortStatus, type Runtime, type Server, type Settings as SettingsT } from "../lib/api";
import { useAppStore } from "../lib/store";
import { useTheme } from "../lib/theme";
import { useMotion } from "../lib/motion";
import { installAppUpdate, useAppUpdate } from "../lib/updates";
import { getVersion } from "@tauri-apps/api/app";
import { disable as disableAutostart, enable as enableAutostart, isEnabled as isAutostartEnabled } from "@tauri-apps/plugin-autostart";
import { runtimeTiles } from "../components/runtimes";
import UninstallDialog from "../components/UninstallDialog";
import MoveEnvironmentDialog from "../components/MoveEnvironmentDialog";
import ApplyBar from "../components/ApplyBar";
import { portFields, portShown, serverNotes } from "../lib/settingsFields";
import { Button, Card, PageHeader, Segmented, Switch, inputClass } from "../components/ui";

type Os = EnvPaths["os"];

/** Settings → section index; ids are the anchors. */
const sections = [
  { id: "general", label: "General", icon: Settings2 },
  { id: "startup", label: "Startup", icon: Power },
  { id: "terminal", label: "Terminal", icon: SquareTerminal },
  { id: "web", label: "Web server", icon: ServerCog },
  { id: "databases", label: "Databases", icon: Database },
  { id: "versions", label: "Active versions", icon: Layers },
  { id: "ports", label: "Ports", icon: Plug },
  { id: "email", label: "Email", icon: Mail },
  { id: "logs", label: "Logs", icon: ScrollText },
  { id: "sharing", label: "Sharing", icon: Share2 },
  { id: "appearance", label: "Appearance", icon: Palette },
  { id: "updates", label: "Updates", icon: Sparkles },
  { id: "help", label: "Troubleshooting", icon: LifeBuoy },
] as const;

const hostsFile: Record<Os, string> = {
  windows: String.raw`C:\Windows\System32\drivers\etc\hosts`,
  macos: "/etc/hosts",
  linux: "/etc/hosts",
};

type Versions = Record<Runtime, string | null>;

/** Port and count fields: the input style without its full width. */
const numberInput = clsx(inputClass.replace("w-full", ""), "w-24 font-mono tabular-nums");

export default function Settings() {
  const queryClient = useQueryClient();
  const notify = useAppStore((s) => s.notify);
  const navigate = useAppStore((s) => s.navigate);
  const { theme, setTheme } = useTheme();
  const { motion, setMotion } = useMotion();
  const settings = useQuery({ queryKey: ["settings"], queryFn: api.getSettings });
  const installed = useQuery({ queryKey: ["installed"], queryFn: api.installedRuntimes });
  const os = useQuery({ queryKey: ["paths"], queryFn: api.envPaths }).data?.os;
  const ports = useQuery({ queryKey: ["port-status"], queryFn: api.portStatus, refetchInterval: 5000 });
  const services = useQuery({ queryKey: ["services"], queryFn: api.listServices });
  const openDiagnostics = useAppStore((s) => s.setDiagnosticsOpen);
  const [uninstalling, setUninstalling] = useState(false);
  const [moving, setMoving] = useState(false);
  const [draft, setDraft] = useState<SettingsT | null>(null);
  const [versions, setVersions] = useState<Versions | null>(null);
  const [current, setCurrent] = useState<string>("general");

  const activeVersions = useMemo<Versions | null>(
    () => (installed.data ? (Object.fromEntries(installed.data.map((i) => [i.runtime, i.active])) as Versions) : null),
    [installed.data],
  );

  useEffect(() => {
    if (settings.data) setDraft(settings.data);
  }, [settings.data]);
  useEffect(() => {
    if (activeVersions) setVersions(activeVersions);
  }, [activeVersions]);

  // Highlight the section being read in the index: the last one whose heading has
  // scrolled past the top third of the window.
  useEffect(() => {
    const update = () => {
      const line = window.innerHeight / 3;
      let found = "general";
      document.querySelectorAll<HTMLElement>("section[id^=settings-]").forEach((el) => {
        if (el.getBoundingClientRect().top <= line) found = el.id.replace("settings-", "");
      });
      setCurrent(found);
    };
    update();
    document.addEventListener("scroll", update, true);
    return () => document.removeEventListener("scroll", update, true);
  }, [draft !== null]);

  const dirty =
    !!draft &&
    !!settings.data &&
    (JSON.stringify({ ...draft, versions: null, sites: null, profiles: null, catchDumps: null }) !==
      JSON.stringify({ ...settings.data, versions: null, sites: null, profiles: null, catchDumps: null }) ||
      JSON.stringify(versions) !== JSON.stringify(activeVersions));

  const save = useMutation({
    mutationFn: async () => {
      if (!draft || !settings.data) return;
      // Versions are switched separately so running services restart onto them; per-site
      // options, profiles and the dump catcher (it restarts PHP) are owned by their own
      // controls, so they come from the latest copy.
      const latest = await api.getSettings();
      await api.saveSettings({ ...draft, versions: latest.versions, sites: latest.sites, profiles: latest.profiles, catchDumps: latest.catchDumps });
      for (const runtime of Object.keys(versions ?? {}) as Runtime[]) {
        const version = versions?.[runtime];
        if (version && version !== activeVersions?.[runtime]) await api.setActiveVersion(runtime, version);
      }
    },
    onSuccess: () => {
      for (const key of ["settings", "installed", "services", "paths", "sites"]) {
        queryClient.invalidateQueries({ queryKey: [key] });
      }
      notify("info", "Settings applied. Running services that use them were restarted.");
    },
    onError: (e) => notify("error", String(e)),
  });


  if (!draft || !versions) return null;
  const set = (patch: Partial<SettingsT>) => setDraft({ ...draft, ...patch });
  const setPort = (key: keyof SettingsT["ports"], value: number) => setDraft({ ...draft, ports: { ...draft.ports, [key]: value } });
  const installedServices = (services.data ?? []).filter((s) => s.installed && !s.external);
  const dbInstalled = { mariadb: !!activeVersions?.mariadb, mysql: !!activeVersions?.mysql };

  return (
    <>
      <PageHeader
        title="Settings"
        subtitle="Configure your local environment."
        actions={
          <div className="flex items-center gap-4">
            {dirty ? (
              <span className="flex items-center gap-2 text-sm text-warn" role="status">
                <span className="size-2 rounded-full bg-warn" /> Unsaved changes
              </span>
            ) : (
              <span className="flex items-center gap-2 text-sm text-muted" role="status">
                <CheckCircle2 size={17} className="text-accent-text" /> All changes saved
              </span>
            )}
          </div>
        }
      />

      <div className="grid items-start gap-8 lg:grid-cols-[12rem_minmax(0,1fr)]">
        <nav aria-label="Settings sections" className="sticky top-4 hidden lg:block">
          <ul className="space-y-0.5">
            {sections.map((s) => (
              <li key={s.id}>
                <button
                  onClick={() => document.getElementById(`settings-${s.id}`)?.scrollIntoView({ behavior: "smooth", block: "start" })}
                  aria-current={current === s.id ? "true" : undefined}
                  className={clsx(
                    "flex w-full items-center gap-2.5 rounded-lg px-3 py-1.5 text-left text-sm",
                    current === s.id ? "bg-accent-soft font-semibold text-accent-text" : "text-muted hover:bg-surface-2 hover:text-fg",
                  )}
                >
                  <s.icon size={15} aria-hidden /> {s.label}
                </button>
              </li>
            ))}
          </ul>
        </nav>

        <div className="motion-stagger min-w-0 space-y-6">
          <Section id="general" title="General" subtitle="Where your environment lives and how sites are named.">
            <Row label="LampRabit folder" hint="Runtimes, configs, your projects (www), databases and logs. Moving it takes everything along." wide>
              <div className="flex items-center gap-2">
                <span className="min-w-0 flex-1 truncate rounded-lg bg-surface-2 px-3 py-2 font-mono text-sm" title={draft.rootDir}>
                  {settings.data?.rootDir ?? draft.rootDir}
                </span>
                <Button className="h-auto" onClick={() => settings.data && openPath(settings.data.rootDir).catch(() => {})}>
                  <FolderOpen size={15} /> Open
                </Button>
                <Button className="h-auto" onClick={() => setMoving(true)}>
                  <FolderInput size={15} /> Move…
                </Button>
              </div>
            </Row>
            <Row label="Site domain suffix" hint={`Folders in www are served as <name>.${draft.tld || "test"}`}>
              <input
                className={clsx(inputClass, "font-mono")}
                value={draft.tld}
                onChange={(e) => set({ tld: e.target.value.replace(/^\.+/, "") })}
                aria-label="Site domain suffix"
              />
            </Row>
            <Row label="Editor command" hint='Used by "Open in editor": code, cursor, phpstorm, subl, zed…'>
              <input
                className={clsx(inputClass, "font-mono")}
                list="editor-presets"
                value={draft.editor}
                onChange={(e) => set({ editor: e.target.value })}
                aria-label="Editor command"
              />
              <datalist id="editor-presets">
                {["code", "cursor", "phpstorm", "subl", "zed", "codium"].map((e) => (
                  <option key={e} value={e} />
                ))}
              </datalist>
            </Row>
            <ToggleRow
              label="HTTPS by default"
              hint="Sites use HTTPS with a locally trusted certificate unless you turn it off for a site."
              checked={draft.httpsByDefault}
              onChange={(v) => set({ httpsByDefault: v })}
            />
            {draft.tld === "localhost" ? (
              <Row label="Hosts file" hint=".localhost domains resolve on their own, so the hosts file is left untouched." />
            ) : (
              <ToggleRow
                label="Update the hosts file automatically"
                hint={`Adds your sites to ${os ? hostsFile[os] : "the hosts file"} when the web server starts; your system asks for approval when the list changes.`}
                checked={draft.manageHosts}
                onChange={(v) => set({ manageHosts: v })}
              />
            )}
          </Section>

          <Section id="startup" title="Startup" subtitle="What happens when you sign in, open or close LampRabit.">
            <AutostartToggle />
            <ToggleRow
              label="Start services when LampRabit opens"
              hint="Starts every installed service, so your sites are up right after sign-in."
              checked={draft.startServicesOnLaunch}
              onChange={(v) => set({ startServicesOnLaunch: v })}
            />
            <ToggleRow
              label="Keep running in the tray when closed"
              hint="Closing the window leaves LampRabit and your services running; quit from the tray icon."
              checked={draft.closeToTray}
              onChange={(v) => set({ closeToTray: v })}
            />
            <Row
              label="Restart after a crash"
              hint="Starts a service again when it stops unexpectedly. After three crashes within five minutes it stays stopped, so the cause shows in Logs."
              wide
            >
              <div className="flex flex-wrap gap-x-6 gap-y-2">
                {installedServices.length === 0 && <span className="text-sm text-muted">Install a service to choose here.</span>}
                {installedServices.map((s) => (
                  <label key={s.kind} className="flex items-center gap-2 text-sm">
                    <Switch
                      checked={draft.autoRestart.includes(s.kind)}
                      onChange={(on) =>
                        set({ autoRestart: on ? [...draft.autoRestart, s.kind].sort() : draft.autoRestart.filter((k) => k !== s.kind) })
                      }
                      label={`Restart ${s.label} after a crash`}
                    />
                    {s.label}
                  </label>
                ))}
              </div>
            </Row>
          </Section>

          <Section id="terminal" title="Terminal" subtitle="LampRabit's commands always run your active versions with its config.">
            <ToggleRow
              label="Add LampRabit's commands to PATH"
              hint={
                os === "windows"
                  ? "Applies to terminals opened after saving. No administrator rights needed."
                  : "Adds a marked block to your shell profile (.zshrc / .bashrc); applies to new terminals."
              }
              checked={draft.addToPath}
              onChange={(v) => set({ addToPath: v })}
            />
            <Row label="Commands" hint="Also available in the terminal that the title bar's terminal button opens." wide>
              <div className="flex flex-wrap gap-1.5">
                {["php", "composer", "mysql", "mysqldump", "mysqladmin", "mariadb", "mariadb-dump", "redis-cli"].map((c) => (
                  <code key={c} className="rounded-md bg-surface-2 px-2 py-0.5 font-mono text-xs">
                    {c}
                  </code>
                ))}
              </div>
            </Row>
          </Section>

          <Section id="web" title="Web servers" subtitle="Which servers run your sites. Several can run side by side, each site on one of them.">
            {allServers.map((server) => {
              const on = draft.webServers.includes(server);
              const only = on && draft.webServers.length === 1;
              return (
                <Row key={server} label={runtimeLabels[server]} hint={only ? `${serverNotes[server]} At least one server stays on.` : serverNotes[server]}>
                  <Switch
                    checked={on}
                    disabled={only}
                    onChange={(v) => set({ webServers: v ? [...draft.webServers, server] : draft.webServers.filter((s) => s !== server) })}
                    label={`Run ${runtimeLabels[server]}`}
                  />
                </Row>
              );
            })}
            {draft.webServers.length > 1 && (
              <Row
                label="Primary server"
                hint={`It has ports ${draft.ports.http}/${draft.ports.https}, localhost and the developer tools; the others use their own ports (below). Pick each site's server on the Sites page.`}
              >
                <Segmented
                  value={draft.webServers[0]}
                  onChange={(primary: Server) => set({ webServers: [primary, ...draft.webServers.filter((s) => s !== primary)] })}
                  options={draft.webServers.map((s) => ({ value: s, label: runtimeLabels[s] }))}
                />
              </Row>
            )}
            {os === "windows" && (
              <Row label="PHP workers per version" hint="Each handles one request at a time. 4 suits most projects; raise it for parallel requests.">
                <input
                  type="number"
                  min={1}
                  max={10}
                  className={numberInput}
                  value={draft.phpWorkers}
                  onChange={(e) => set({ phpWorkers: Math.max(1, Math.min(10, Number(e.target.value))) })}
                  aria-label="PHP workers per version"
                />
              </Row>
            )}
          </Section>

          <Section id="databases" title="Databases" subtitle="MariaDB and MySQL can be installed and run side by side.">
            <Row
              label="Default database"
              hint="Used by the mysql, mysqldump and mysqladmin commands, new projects, and listed first in phpMyAdmin and Adminer (which offer both). The mariadb commands always mean MariaDB."
            >
              <Segmented
                value={draft.defaultDatabase}
                onChange={(defaultDatabase) => set({ defaultDatabase })}
                options={[
                  { value: "mariadb", label: dbInstalled.mariadb ? "MariaDB" : "MariaDB (not installed)" },
                  { value: "mysql", label: dbInstalled.mysql ? "MySQL" : "MySQL (not installed)" },
                ]}
              />
            </Row>
            {!dbInstalled[draft.defaultDatabase] && (dbInstalled.mariadb || dbInstalled.mysql) && (
              <p className="px-5 py-3 text-xs text-warn">
                {draft.defaultDatabase === "mysql" ? "MySQL" : "MariaDB"} isn't installed, so {dbInstalled.mariadb ? "MariaDB" : "MySQL"} is used until it is.
              </p>
            )}
          </Section>

          <Section id="versions" title="Active versions" subtitle="Which installed version each service runs. Sites can pick their own PHP.">
            {(["apache", "nginx", "caddy", "frankenphp", "php", "mariadb", "mysql", "composer", "java", "tomcat"] as Runtime[])
              .filter((r) => !(allServers as Runtime[]).includes(r) || draft.webServers.includes(r as Server))
              .map((runtime) => {
                const options = installed.data?.find((i) => i.runtime === runtime)?.versions ?? [];
                return (
                  <Row
                    key={runtime}
                    label={
                      <span className="flex items-center gap-2.5">
                        <span className={clsx("flex size-7 items-center justify-center rounded-lg", runtimeTiles[runtime].className)} aria-hidden>
                          {runtimeTiles[runtime].icon(15)}
                        </span>
                        {runtimeLabels[runtime]}
                      </span>
                    }
                  >
                    {options.length > 0 ? (
                      <select
                        className={inputClass}
                        value={versions[runtime] ?? ""}
                        onChange={(e) => setVersions({ ...versions, [runtime]: e.target.value })}
                        aria-label={`${runtimeLabels[runtime]} version`}
                      >
                        {options.map((v) => (
                          <option key={v}>{v}</option>
                        ))}
                      </select>
                    ) : (
                      <button
                        onClick={() => navigate("services")}
                        className="w-full rounded-lg border border-dashed border-line px-3 py-2 text-left text-sm text-muted hover:border-accent hover:text-accent-text"
                      >
                        Not installed. Install from Services
                      </button>
                    )}
                  </Row>
                );
              })}
          </Section>

          <Section id="ports" title="Ports" subtitle="Network ports used by local services. Running services restart when you apply a change.">
            <div className="grid sm:grid-cols-2">
              {portFields
                .filter((f) => portShown(f, draft, os))
                .map(({ key, label, hint }) => (
                  <div key={key} className="flex items-center gap-4 border-b border-line px-5 py-3 sm:odd:border-r">
                    <div className="min-w-0 flex-1">
                      <label htmlFor={`port-${key}`} className="block text-sm font-semibold">
                        {label}
                      </label>
                      <PortState port={draft.ports[key]} saved={settings.data?.ports[key]} status={ports.data?.find((p) => p.key === key)} hint={hint} />
                    </div>
                    <input
                      id={`port-${key}`}
                      type="number"
                      min={1}
                      max={65535}
                      className={numberInput}
                      value={draft.ports[key]}
                      onChange={(e) => setPort(key, Number(e.target.value))}
                    />
                  </div>
                ))}
            </div>
          </Section>

          <Section id="email" title="Email" subtitle="Mail your apps send lands in Mailpit instead of real inboxes.">
            {!draft.versions.mailpit && (
              <Row label="Mailpit" hint="Not installed yet.">
                <Button size="sm" variant="soft" onClick={() => navigate("services")}>
                  Install from Services
                </Button>
              </Row>
            )}
            <ToggleRow
              label="Send PHP mail() to Mailpit"
              hint={
                (os === "windows"
                  ? "Sets SMTP and smtp_port in php.ini; applies when the web server restarts."
                  : "Sets sendmail_path to Mailpit's sendmail mode; applies when PHP restarts.") +
                " Frameworks send over SMTP instead: copy their settings from Home's Connect panel."
              }
              checked={draft.routeMail}
              onChange={(v) => set({ routeMail: v })}
            />
            <Row label="SMTP port" hint="Where apps send mail.">
              <input
                type="number"
                min={1}
                max={65535}
                className={numberInput}
                value={draft.ports.mailSmtp}
                onChange={(e) => setPort("mailSmtp", Number(e.target.value))}
                aria-label="Mailpit SMTP port"
              />
            </Row>
            <Row label="Inbox port" hint="Mailpit's web inbox.">
              <input
                type="number"
                min={1}
                max={65535}
                className={numberInput}
                value={draft.ports.mailUi}
                onChange={(e) => setPort("mailUi", Number(e.target.value))}
                aria-label="Mailpit inbox port"
              />
            </Row>
          </Section>

          <Section id="logs" title="Logs" subtitle="How big log files may grow, and what PHP's dump() does.">
            <ToggleRow
              label="Catch dump() and dd()"
              hint="Laravel and Symfony dumps from web requests show in Logs (as a tree you can fold) instead of in the page. Console commands like tinker still print in the terminal. Applies right away; PHP restarts."
              checked={draft.catchDumps}
              onChange={(v) =>
                api.setCatchDumps(v).then(
                  () => {
                    setDraft({ ...draft, catchDumps: v });
                    queryClient.invalidateQueries({ queryKey: ["settings"] });
                  },
                  (e) => notify("error", String(e)),
                )
              }
            />
            <Row label="Rotate a log at" hint="A bigger file is copied aside and started afresh; checked about once a minute.">
              <span className="flex items-center gap-2 text-sm text-muted">
                <input type="number" min={1} max={2048} className={numberInput} value={draft.logMaxMb} onChange={(e) => set({ logMaxMb: Math.max(1, Number(e.target.value)) })} aria-label="Rotate a log at (MB)" />
                MB
              </span>
            </Row>
            <Row label="Old copies to keep" hint="Per log file, as name.1.log, name.2.log…; 0 keeps none.">
              <input type="number" min={0} max={20} className={numberInput} value={draft.logKeep} onChange={(e) => set({ logKeep: Math.min(20, Math.max(0, Number(e.target.value))) })} aria-label="Old copies to keep" />
            </Row>
            <Row label="Logs folder limit" hint="Over this, the oldest copies are deleted first.">
              <span className="flex items-center gap-2 text-sm text-muted">
                <input type="number" min={10} max={100000} className={numberInput} value={draft.logTotalMb} onChange={(e) => set({ logTotalMb: Math.max(10, Number(e.target.value)) })} aria-label="Logs folder limit (MB)" />
                MB
              </span>
            </Row>
          </Section>

          <SharingSection />

          <Section id="appearance" title="Appearance" subtitle="How LampRabit looks and moves. Applies instantly.">
            <Row label="Theme" hint="Match your system, or always light or dark.">
              <Segmented
                value={theme}
                onChange={setTheme}
                options={[
                  { value: "system", label: "System", icon: <Monitor size={15} /> },
                  { value: "light", label: "Light", icon: <Sun size={15} /> },
                  { value: "dark", label: "Dark", icon: <Moon size={15} /> },
                ]}
              />
            </Row>
            <Row
              label="Motion"
              hint={
                motion === "system"
                  ? "Follows your system's reduce-motion setting."
                  : motion === "full"
                    ? "Transitions, the animated logo and live diagram flows."
                    : motion === "reduced"
                      ? "Gentle fades only: nothing slides, bounces or plays on its own."
                      : "No animation. Loading spinners keep turning so you can tell work is happening."
              }
            >
              <Segmented
                value={motion}
                onChange={setMotion}
                options={[
                  { value: "system", label: "System", icon: <Monitor size={15} /> },
                  { value: "full", label: "Full", icon: <Wind size={15} /> },
                  { value: "reduced", label: "Reduced", icon: <Feather size={15} /> },
                  { value: "off", label: "Off", icon: <CircleSlash size={15} /> },
                ]}
              />
            </Row>
          </Section>

          <UpdatesSection checkUpdates={draft.checkUpdates} onCheckUpdatesChange={(v) => set({ checkUpdates: v })} />

          <Section id="help" title="Troubleshooting" subtitle="When something doesn't start or behaves oddly.">
            <Row
              label="Diagnostics report"
              hint="Versions, service health, who uses each port, config tests, hosts and certificate status, and recent errors. Copy it into an issue."
            >
              <Button variant="primary" onClick={() => openDiagnostics(true)}>
                <Stethoscope size={15} /> Run diagnostics
              </Button>
            </Row>
            <Row
              label="Remove LampRabit from this computer"
              hint="Deletes runtimes, configs, logs, certificates, hosts entries and settings. Your projects (www) and databases (data) are kept."
            >
              <Button variant="danger" onClick={() => setUninstalling(true)}>
                <Trash2 size={15} /> Remove…
              </Button>
            </Row>
          </Section>
          {uninstalling && <UninstallDialog onClose={() => setUninstalling(false)} />}
        </div>
      </div>
      {moving && settings.data && <MoveEnvironmentDialog current={settings.data.rootDir} onClose={() => setMoving(false)} />}
      <ApplyBar
        dirty={dirty}
        busy={save.isPending}
        onApply={() => save.mutate()}
        onDiscard={() => {
          setDraft(settings.data ?? null);
          setVersions(activeVersions);
        }}
      />
    </>
  );
}

/** A settings group: a heading and rows separated by lines. */
function Section({ id, title, subtitle, children }: { id: string; title: string; subtitle?: string; children: ReactNode }) {
  return (
    <section id={`settings-${id}`} aria-labelledby={`settings-${id}-title`} className="scroll-mt-4">
      <h2 id={`settings-${id}-title`} className="text-base font-bold tracking-tight">
        {title}
      </h2>
      {subtitle && <p className="mt-0.5 text-sm text-muted">{subtitle}</p>}
      <Card className="mt-3 divide-y divide-line overflow-hidden">{children}</Card>
    </section>
  );
}

/** Label and hint on the left, the control on the right; `wide` puts the control underneath. */
function Row({ label, hint, wide = false, children }: { label: ReactNode; hint?: string; wide?: boolean; children?: ReactNode }) {
  return (
    <div className={clsx("px-5 py-3.5", wide ? "space-y-2.5" : "flex flex-wrap items-center justify-between gap-x-6 gap-y-2")}>
      <div className="min-w-0 flex-1 basis-64">
        <div className="text-sm font-semibold">{label}</div>
        {hint && <p className="mt-0.5 text-xs leading-relaxed text-muted">{hint}</p>}
      </div>
      {children && <div className={clsx(wide ? "" : "flex w-full shrink-0 sm:w-auto sm:min-w-64 sm:max-w-md sm:justify-end")}>{children}</div>}
    </div>
  );
}

function ToggleRow({ label, hint, checked, onChange }: { label: string; hint: string; checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <div className="flex items-center justify-between gap-6 px-5 py-3.5">
      <div className="min-w-0">
        <div className="text-sm font-semibold">{label}</div>
        <p className="mt-0.5 text-xs leading-relaxed text-muted">{hint}</p>
      </div>
      <Switch checked={checked} onChange={onChange} label={label} />
    </div>
  );
}

/** Live status of a port field: free, LampRabit's own, or taken by another program. */
function PortState({ port, saved, status, hint }: { port: number; saved?: number; status?: PortStatus; hint: string }) {
  const edited = saved !== undefined && port !== saved;
  const check = useQuery({
    queryKey: ["check-port", port],
    queryFn: () => api.checkPort(port),
    enabled: edited && port > 0 && port < 65536,
  });

  let tone: "ok" | "muted" | "warn" = "muted";
  let text = hint;
  if (edited) {
    if (check.data !== undefined) {
      tone = check.data ? "ok" : "warn";
      text = check.data ? "Available" : "In use by another program";
    }
  } else if (status?.free) {
    tone = "ok";
    text = `${hint} · available`;
  } else if (status?.usedByService) {
    text = `${hint} · used by ${status.usedByService}`;
  } else if (status) {
    tone = "warn";
    text = status.owner ? `Taken by ${status.owner.name} (PID ${status.owner.pid})` : "Taken by another program";
  }

  return (
    <span
      className={clsx("mt-0.5 flex items-center gap-1.5 text-xs", tone === "ok" && "text-muted", tone === "muted" && "text-muted", tone === "warn" && "text-warn")}
      title={status?.owner?.note ?? undefined}
    >
      <span className={clsx("size-1.5 shrink-0 rounded-full", tone === "ok" ? "bg-ok" : tone === "warn" ? "bg-warn" : "bg-muted/60")} />
      <span className="truncate">{text}</span>
    </span>
  );
}

/** Ngrok's authtoken lives in the system keychain, never in LampRabit's files. */
function SharingSection() {
  const queryClient = useQueryClient();
  const notify = useAppStore((s) => s.notify);
  const saved = useQuery({ queryKey: ["ngrok-token"], queryFn: api.ngrokTokenSaved });
  const [token, setToken] = useState("");
  const save = useMutation({
    mutationFn: (t: string | null) => api.setNgrokToken(t),
    onSuccess: (_, t) => {
      setToken("");
      queryClient.invalidateQueries({ queryKey: ["ngrok-token"] });
      notify("info", t ? "Ngrok authtoken saved to your system keychain" : "Ngrok authtoken removed");
    },
    onError: (e) => notify("error", String(e)),
  });

  return (
    <Section id="sharing" title="Sharing" subtitle="Put a project on a public URL to test webhooks or show a client. Share from a project's card on the Sites page.">
      <Row
        label="Ngrok authtoken"
        hint={`${saved.data ? "A token is saved in your system keychain. " : ""}Ngrok needs a free account: dashboard.ngrok.com → Your Authtoken. The free plan gives one random URL at a time.`}
        wide
      >
        <div className="flex gap-2">
          <input
            type="password"
            className={inputClass}
            placeholder={saved.data ? "Saved. Paste a new one to replace it" : "Paste your token"}
            value={token}
            onChange={(e) => setToken(e.target.value)}
            autoComplete="off"
            aria-label="Ngrok authtoken"
          />
          <Button variant="primary" className="h-auto" onClick={() => save.mutate(token)} disabled={!token.trim() || save.isPending}>
            Save
          </Button>
          {saved.data && (
            <Button className="h-auto" onClick={() => save.mutate(null)} disabled={save.isPending}>
              Remove
            </Button>
          )}
        </div>
      </Row>
      <Row label="Cloudflare quick tunnels" hint="No account or token: install Cloudflare Tunnel from Services. Each share gets a random trycloudflare.com address until you stop it." />
    </Section>
  );
}

function UpdatesSection({ checkUpdates, onCheckUpdatesChange }: { checkUpdates: boolean; onCheckUpdatesChange: (v: boolean) => void }) {
  const notify = useAppStore((s) => s.notify);
  const version = useQuery({ queryKey: ["app-version"], queryFn: getVersion });
  const { available, checking, error, checked, checkNow } = useAppUpdate();
  const [progress, setProgress] = useState<number | null | undefined>(undefined);

  const install = async () => {
    if (!available) return;
    setProgress(null);
    try {
      await installAppUpdate(available, setProgress);
    } catch (e) {
      setProgress(undefined);
      notify("error", String(e));
    }
  };

  const status = available ? (
    <span className="font-semibold text-accent-text">
      Version {available.version} is available.{" "}
      <button
        className="inline-flex items-center gap-1 font-normal hover:underline"
        onClick={() => openUrl(`https://github.com/Shivam-SV/LampRabit/releases/tag/v${available.version}`).catch(() => {})}
      >
        What's new <ExternalLink size={12} />
      </button>
    </span>
  ) : checking ? (
    "Checking for updates…"
  ) : error ? (
    error
  ) : checked ? (
    "You're on the latest version."
  ) : (
    "Updates are signed and installed in place; your environment is kept."
  );

  return (
    <Section id="updates" title="Updates" subtitle={`LampRabit ${version.data ?? ""}`}>
      <div className="flex flex-wrap items-center justify-between gap-x-6 gap-y-2 px-5 py-3.5">
        <div className="min-w-0 flex-1 basis-64 text-sm text-muted">{status}</div>
        {available ? (
          <Button variant="primary" onClick={install} disabled={progress !== undefined}>
            {progress === undefined ? "Install and restart" : progress === null ? "Downloading…" : `Downloading ${progress}%`}
          </Button>
        ) : (
          <Button onClick={() => checkNow()} disabled={checking}>
            <RefreshCw size={15} className={clsx(checking && "animate-spin")} /> Check for updates
          </Button>
        )}
      </div>
      <ToggleRow
        label="Check when LampRabit starts"
        hint="Looks for a new release on GitHub; nothing is downloaded until you choose to install."
        checked={checkUpdates}
        onChange={onCheckUpdatesChange}
      />
    </Section>
  );
}

/** Start with the OS. Applied immediately through the OS (no Save needed). */
function AutostartToggle() {
  const notify = useAppStore((s) => s.notify);
  const enabled = useQuery({ queryKey: ["autostart"], queryFn: isAutostartEnabled });
  const queryClient = useQueryClient();
  return (
    <ToggleRow
      label="Start LampRabit when I sign in"
      hint="Opens in the tray, out of the way. Applies immediately."
      checked={!!enabled.data}
      onChange={async (v) => {
        try {
          await (v ? enableAutostart() : disableAutostart());
          queryClient.invalidateQueries({ queryKey: ["autostart"] });
        } catch (e) {
          notify("error", String(e));
        }
      }}
    />
  );
}
