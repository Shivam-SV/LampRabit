import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { FileCode, Plus, Trash2 } from "lucide-react";
import clsx from "clsx";
import { api } from "../lib/api";
import { useAppStore } from "../lib/store";
import Dialog from "./Dialog";
import { Button, Segmented, Switch, inputClass } from "./ui";

interface ProcessRow {
  name: string;
  command: string;
  port: boolean;
  autostart: boolean;
}

interface Form {
  name: string;
  /** Set by hand; otherwise follows the name. */
  id: string;
  idTouched: boolean;
  description: string;
  source: "git" | "empty";
  gitUrl: string;
  php: string;
  node: boolean;
  database: "none" | "optional" | "required";
  composer: boolean;
  copyEnv: boolean;
  laravel: boolean;
  npm: boolean;
  processes: ProcessRow[];
  domains: { name: string; root: string }[];
}

const blank: Form = {
  name: "",
  id: "",
  idTouched: false,
  description: "",
  source: "git",
  gitUrl: "",
  php: "8.3",
  node: false,
  database: "optional",
  composer: true,
  copyEnv: true,
  laravel: false,
  npm: false,
  processes: [],
  domains: [],
};

const slug = (text: string) =>
  text
    .toLowerCase()
    .replace(/[^a-z0-9_]+/g, "-")
    .replace(/^-+|-+$/g, "");
/** A YAML single-quoted string. */
const q = (text: string) => `'${text.replace(/'/g, "''")}'`;

/** The guided form as template YAML (docs/templates.md has the format). */
function toYaml(f: Form): string {
  const id = f.idTouched ? f.id : slug(f.name);
  const lines = ["schema: 1", `id: ${id || "my-template"}`, `name: ${q(f.name || "My template")}`];
  if (f.description.trim()) lines.push(`description: ${q(f.description.trim())}`);
  if (f.php.trim() || f.node) {
    lines.push("requires:");
    if (f.php.trim()) lines.push(`  php: ${q(f.php.trim())}`);
    if (f.node) lines.push("  node: true");
  }
  if (f.database !== "none") lines.push(`database: ${f.database}`);
  lines.push("steps:");
  if (f.database !== "none") lines.push("  - create_database: {}");
  if (f.source === "git" && f.gitUrl.trim()) {
    lines.push("  - name: Getting the code", "    cwd: www", "    needs: [git]", `    run: ${q(`git clone ${f.gitUrl.trim()} "{{project}}"`)}`);
  } else {
    lines.push("  - name: Writing a README", "    write:", "      path: README.md", '      content: "# {{project}}\\n"');
  }
  if (f.composer) lines.push("  - name: Installing PHP packages", "    composer: [install, --no-interaction]");
  if (f.copyEnv) lines.push("  - name: Creating .env", "    copy: { from: .env.example, to: .env }");
  if (f.laravel) {
    lines.push("  - name: Configuring the environment", "    env:", "      values:", '        APP_URL: "{{url}}"', '        MAIL_HOST: "{{mail.host}}"', '        MAIL_PORT: "{{mail.port}}"');
    if (f.database !== "none") {
      lines.push(
        "  - name: Connecting the database",
        "    if: database",
        "    env:",
        "      values:",
        '        DB_CONNECTION: "{{db.engine}}"',
        '        DB_HOST: "{{db.host}}"',
        '        DB_PORT: "{{db.port}}"',
        '        DB_DATABASE: "{{database}}"',
        '        DB_USERNAME: "{{db.user}}"',
        '        DB_PASSWORD: "{{db.password}}"',
      );
    }
    lines.push("  - name: Generating the app key", "    php: [artisan, key:generate]");
    if (f.database !== "none") lines.push("  - name: Running database migrations", "    if: database", "    php: [artisan, migrate, --force]");
  }
  if (f.npm) lines.push("  - name: Installing Node packages", "    needs: [node]", '    run: "{{pm.install}}"');
  const processes = f.processes.filter((p) => p.name.trim() && p.command.trim());
  if (processes.length) {
    lines.push("processes:");
    for (const p of processes) {
      lines.push(`  - name: ${slug(p.name)}`, `    command: ${q(p.command.trim())}`);
      if (p.port) lines.push("    port: auto");
      if (!p.autostart) lines.push("    autostart: false");
    }
  }
  const domains = f.domains.filter((d) => d.name.trim());
  if (domains.length) {
    lines.push("domains:");
    for (const d of domains) {
      lines.push(`  - name: ${slug(d.name)}`);
      if (d.root.trim()) lines.push(`    root: ${q(d.root.trim())}`);
    }
  }
  return lines.join("\n") + "\n";
}

/**
 * Making a template without writing YAML by hand: a guided form for the common parts
 * (where the code comes from, what it needs, its install steps, processes and domains),
 * and the YAML itself to adjust anything else. "Save as template" on a project opens
 * straight on its generated YAML. Saved into the templates folder after the same checks
 * loading a template does.
 */
export default function TemplateEditor() {
  const queryClient = useQueryClient();
  const notify = useAppStore((s) => s.notify);
  const editor = useAppStore((s) => s.templateEditor);
  const close = useAppStore((s) => s.closeTemplateEditor);
  const setNewProjectOpen = useAppStore((s) => s.setNewProjectOpen);
  if (!editor) return null;
  return (
    <Editor
      initialYaml={editor.yaml}
      onClose={(saved) => {
        close();
        if (saved) queryClient.invalidateQueries({ queryKey: ["templates"] });
        // Back to the wizard it was opened from, with the new template in its list.
        if (editor.fromWizard) setNewProjectOpen(true);
        if (saved) notify("info", `Saved the template ${saved}. It's in New project's list.`);
      }}
    />
  );
}

function Editor({ initialYaml, onClose }: { initialYaml?: string; onClose: (saved?: string) => void }) {
  const [form, setForm] = useState<Form>(blank);
  const [tab, setTab] = useState<"guided" | "yaml">(initialYaml ? "yaml" : "guided");
  const [yaml, setYaml] = useState(initialYaml ?? toYaml(blank));
  /** The YAML was changed by hand: the guided form would overwrite it. */
  const [yamlEdited, setYamlEdited] = useState(!!initialYaml);
  const [taken, setTaken] = useState(false);
  const update = (patch: Partial<Form>) => {
    const next = { ...form, ...patch };
    setForm(next);
    setYaml(toYaml(next));
    setYamlEdited(false);
    setTaken(false);
  };
  const save = useMutation({
    mutationFn: (replace: boolean) => api.saveTemplate(yaml, replace),
    onSuccess: (id) => onClose(id),
    onError: (e) => setTaken(String(e).includes("already have a template")),
  });
  const field = "mb-1 block text-xs font-semibold text-muted";
  const check = (key: "composer" | "copyEnv" | "laravel" | "npm" | "node", label: string, hint?: string) => (
    <label className="flex items-start gap-2 text-sm">
      <input type="checkbox" className="mt-0.5 size-4 accent-(--accent)" checked={form[key]} onChange={(e) => update({ [key]: e.target.checked })} />
      <span>
        {label}
        {hint && <span className="block text-xs text-muted">{hint}</span>}
      </span>
    </label>
  );

  return (
    <Dialog
      open
      onClose={() => !save.isPending && onClose()}
      title={initialYaml ? "Save as a template" : "New template"}
      icon={<FileCode size={18} />}
      width="max-w-2xl"
      footer={
        <>
          {save.error && <span className="mr-auto max-w-sm truncate text-xs text-danger-text" title={String(save.error)}>{String(save.error)}</span>}
          <Button onClick={() => onClose()} disabled={save.isPending}>
            Cancel
          </Button>
          {taken && (
            <Button variant="danger" onClick={() => save.mutate(true)} disabled={save.isPending}>
              Replace it
            </Button>
          )}
          <Button variant="primary" onClick={() => save.mutate(false)} disabled={save.isPending}>
            {save.isPending ? "Saving…" : "Save template"}
          </Button>
        </>
      }
    >
      <div className="space-y-4">
        <Segmented
          value={tab}
          onChange={setTab}
          options={[
            { value: "guided", label: "Guided" },
            { value: "yaml", label: "YAML" },
          ]}
          className="w-56"
        />

        {tab === "guided" ? (
          <div className="space-y-4 text-sm">
            {yamlEdited && <p className="rounded-lg bg-warn/10 px-3 py-2 text-xs text-warn">You changed the YAML by hand; changing anything here replaces those edits.</p>}
            <div className="grid gap-3 sm:grid-cols-2">
              <label className="block">
                <span className={field}>Name</span>
                <input className={inputClass} value={form.name} placeholder="Shop" onChange={(e) => update({ name: e.target.value })} data-autofocus />
              </label>
              <label className="block">
                <span className={field}>Id (its file name)</span>
                <input className={clsx(inputClass, "font-mono")} value={form.idTouched ? form.id : slug(form.name)} placeholder="shop" onChange={(e) => update({ id: slug(e.target.value), idTouched: true })} />
              </label>
            </div>
            <label className="block">
              <span className={field}>Description</span>
              <input className={inputClass} value={form.description} placeholder="Our shop, ready to work on" onChange={(e) => update({ description: e.target.value })} />
            </label>

            <fieldset className="space-y-2">
              <legend className={field}>The code comes from</legend>
              <Segmented
                value={form.source}
                onChange={(source) => update({ source })}
                options={[
                  { value: "git", label: "A Git repository" },
                  { value: "empty", label: "An empty folder" },
                ]}
                className="w-80"
              />
              {form.source === "git" && <input className={clsx(inputClass, "font-mono")} value={form.gitUrl} placeholder="https://github.com/you/shop.git" onChange={(e) => update({ gitUrl: e.target.value })} aria-label="Repository URL" />}
            </fieldset>

            <div className="grid gap-3 sm:grid-cols-2">
              <label className="block">
                <span className={field}>Lowest PHP version (empty: no PHP)</span>
                <input className={clsx(inputClass, "font-mono")} value={form.php} placeholder="8.3" onChange={(e) => update({ php: e.target.value })} />
              </label>
              <label className="block">
                <span className={field}>Database</span>
                <select className={inputClass} value={form.database} onChange={(e) => update({ database: e.target.value as Form["database"] })}>
                  <option value="none">None</option>
                  <option value="optional">Offered, on by default</option>
                  <option value="required">Always</option>
                </select>
              </label>
            </div>

            <fieldset className="space-y-2">
              <legend className={field}>After getting the code</legend>
              {check("composer", "composer install")}
              {check("copyEnv", "Copy .env.example to .env")}
              {check("laravel", "Laravel: fill in .env, generate the key, run migrations", "APP_URL, mail and the database go into .env.")}
              {check("node", "It uses Node.js")}
              {form.node && check("npm", "Install the Node packages")}
            </fieldset>

            <fieldset className="space-y-2">
              <legend className={field}>Processes that keep running</legend>
              {form.processes.map((p, i) => {
                const set = (patch: Partial<ProcessRow>) => update({ processes: form.processes.map((x, j) => (j === i ? { ...x, ...patch } : x)) });
                return (
                  <div key={i} className="flex flex-wrap items-center gap-2">
                    <input className={clsx(inputClass.replace("w-full", ""), "w-28 font-mono")} value={p.name} placeholder="queue" onChange={(e) => set({ name: e.target.value })} aria-label="Process name" />
                    <input className={clsx(inputClass.replace("w-full", ""), "min-w-0 flex-1 font-mono")} value={p.command} placeholder="php artisan queue:work" onChange={(e) => set({ command: e.target.value })} aria-label="Command" />
                    <span className="flex items-center gap-1.5 text-xs text-muted" title="A free port, as {port} and $PORT">
                      <Switch checked={p.port} onChange={(port) => set({ port })} label="Gets a port" /> port
                    </span>
                    <span className="flex items-center gap-1.5 text-xs text-muted" title="Starts with the project">
                      <Switch checked={p.autostart} onChange={(autostart) => set({ autostart })} label="Starts with the project" /> auto
                    </span>
                    <button type="button" onClick={() => update({ processes: form.processes.filter((_, j) => j !== i) })} className="rounded p-1 text-muted hover:text-danger-text" aria-label="Remove process">
                      <Trash2 size={14} />
                    </button>
                  </div>
                );
              })}
              <Button size="sm" variant="ghost" onClick={() => update({ processes: [...form.processes, { name: "", command: "", port: false, autostart: true }] })}>
                <Plus size={13} /> Add a process
              </Button>
            </fieldset>

            <fieldset className="space-y-2">
              <legend className={field}>Subdomains</legend>
              {form.domains.map((d, i) => {
                const set = (patch: Partial<Form["domains"][number]>) => update({ domains: form.domains.map((x, j) => (j === i ? { ...x, ...patch } : x)) });
                return (
                  <div key={i} className="flex flex-wrap items-center gap-2">
                    <input className={clsx(inputClass.replace("w-full", ""), "w-28 font-mono")} value={d.name} placeholder="admin" onChange={(e) => set({ name: e.target.value })} aria-label="Subdomain" />
                    <span className="font-mono text-xs text-muted">.project.test →</span>
                    <input className={clsx(inputClass.replace("w-full", ""), "min-w-0 flex-1 font-mono")} value={d.root} placeholder="admin/public (empty: like the main domain)" onChange={(e) => set({ root: e.target.value })} aria-label="Folder" />
                    <button type="button" onClick={() => update({ domains: form.domains.filter((_, j) => j !== i) })} className="rounded p-1 text-muted hover:text-danger-text" aria-label="Remove subdomain">
                      <Trash2 size={14} />
                    </button>
                  </div>
                );
              })}
              <Button size="sm" variant="ghost" onClick={() => update({ domains: [...form.domains, { name: "", root: "" }] })}>
                <Plus size={13} /> Add a subdomain
              </Button>
            </fieldset>
          </div>
        ) : (
          <div className="space-y-2">
            <p className="text-xs text-muted">The template as it's saved. Everything the format offers is described in docs/templates.md (variables, conditions, more step kinds).</p>
            <textarea
              className={clsx(inputClass, "h-96 font-mono text-xs leading-relaxed")}
              value={yaml}
              spellCheck={false}
              onChange={(e) => {
                setYaml(e.target.value);
                setYamlEdited(true);
                setTaken(false);
              }}
              aria-label="Template YAML"
            />
          </div>
        )}
      </div>
    </Dialog>
  );
}
