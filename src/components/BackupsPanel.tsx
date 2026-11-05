import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { openPath } from "@tauri-apps/plugin-opener";
import { ArchiveRestore, DatabaseBackup, FolderOpen, Trash2 } from "lucide-react";
import { api, runtimeLabels, type Backup, type DbEngine } from "../lib/api";
import { useAppStore } from "../lib/store";
import Dialog from "./Dialog";
import { Button, Card, inputClass } from "./ui";

const size = (bytes: number) =>
  bytes < 1024 * 1024 ? `${Math.max(1, Math.round(bytes / 1024))} KB` : `${(bytes / 1024 / 1024).toFixed(1)} MB`;
const when = (ms: number) => new Date(ms).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });

/**
 * Backups of one database server: a Back up button per database, and the dumps in
 * backups/<engine>/ with Restore (into the same or another database) and Delete. SQLite's
 * databases are the .sqlite/.db files in the projects, so there's no server to start.
 */
export default function BackupsPanel({ engine }: { engine: DbEngine }) {
  const queryClient = useQueryClient();
  const notify = useAppStore((s) => s.notify);
  const label = runtimeLabels[engine];
  const services = useQuery({ queryKey: ["services"], queryFn: api.listServices });
  const paths = useQuery({ queryKey: ["paths"], queryFn: api.envPaths });
  const files = engine === "sqlite";
  const running = files || services.data?.find((s) => s.kind === engine)?.state === "running";
  const databases = useQuery({ queryKey: ["databases", engine], queryFn: () => api.listDatabases(engine), enabled: running });
  const backups = useQuery({ queryKey: ["backups"], queryFn: api.listBackups });
  const [restoring, setRestoring] = useState<Backup | null>(null);
  const [target, setTarget] = useState("");
  // Same rules as the backend: a name, or for SQLite a file inside a project.
  const validTarget = files
    ? /^[^~]+\.(sqlite3?|db)$/i.test(target) && !target.split(/[\\/]/).some((part) => part === ".." || part === "")
    : /^[A-Za-z0-9_-]{1,64}$/.test(target);
  const [deleting, setDeleting] = useState<string | null>(null);

  const onError = (e: unknown) => notify("error", String(e));
  const refresh = () => {
    queryClient.invalidateQueries({ queryKey: ["backups"] });
    queryClient.invalidateQueries({ queryKey: ["databases", engine] });
  };
  const backup = useMutation({
    mutationFn: (database: string) => api.backupDatabase(engine, database),
    onSuccess: (b) => {
      notify("info", `Backed up ${b.database} (${size(b.size)})`);
      refresh();
    },
    onError,
  });
  const restore = useMutation({
    mutationFn: ({ path, database }: { path: string; database: string }) => api.restoreBackup(engine, path, database),
    onSuccess: (_, { database }) => {
      notify("info", `Restored into ${database}`);
      setRestoring(null);
      refresh();
    },
    onError,
  });
  const remove = useMutation({ mutationFn: api.deleteBackup, onSuccess: refresh, onError, onSettled: () => setDeleting(null) });

  const mine = (backups.data ?? []).filter((b) => b.engine === engine);
  const sep = paths.data?.os === "windows" ? "\\" : "/";
  const folder = paths.data ? `${paths.data.root}${sep}backups${sep}${engine}` : null;

  return (
    <Card className="mt-4 overflow-hidden">
      <div className="flex flex-wrap items-center gap-3 border-b border-line px-5 py-3">
        <div className="min-w-0 flex-1">
          <h3 className="text-sm font-bold">Backups</h3>
          <p className="text-xs text-muted">Kept in the backups folder, which uninstalling LampRabit keeps too.</p>
        </div>
        {folder && mine.length > 0 && (
          <Button size="sm" variant="ghost" onClick={() => openPath(folder).catch(onError)}>
            <FolderOpen size={14} /> Folder
          </Button>
        )}
      </div>

      <div className="border-b border-line px-5 py-3">
        {!running ? (
          <p className="text-sm text-muted">Start {label} to list its databases and back them up.</p>
        ) : databases.isLoading ? (
          <p className="text-sm text-muted">Loading databases…</p>
        ) : databases.isError ? (
          <p className="text-sm text-danger-text">{String(databases.error)}</p>
        ) : (databases.data ?? []).length === 0 ? (
          <p className="text-sm text-muted">{files ? "No SQLite databases in your projects yet (.sqlite, .sqlite3 or .db files in www)." : "No databases yet."}</p>
        ) : (
          <ul className="flex flex-wrap gap-2">
            {databases.data!.map((d) => (
              <li key={d}>
                <Button size="sm" variant="soft" disabled={backup.isPending} onClick={() => backup.mutate(d)} title={`Back up ${d}`}>
                  <DatabaseBackup size={14} /> <span className={files ? "font-mono" : undefined}>{backup.isPending && backup.variables === d ? "Backing up…" : d}</span>
                </Button>
              </li>
            ))}
          </ul>
        )}
      </div>

      {mine.length === 0 ? (
        <p className="px-5 py-3 text-sm text-muted">No backups yet. Click a database above to back it up.</p>
      ) : (
        <ul>
          {mine.map((b) => (
            <li key={b.path} className="flex flex-wrap items-center gap-3 border-b border-line px-5 py-2.5 last:border-b-0">
              <div className="min-w-0 flex-1">
                <div className="truncate font-mono text-sm">{b.database}</div>
                <div className="text-xs text-muted">
                  {when(b.created)} · {size(b.size)}
                </div>
              </div>
              <Button
                size="sm"
                disabled={!running}
                title={running ? undefined : `Start ${label} to restore`}
                onClick={() => {
                  setTarget(b.database);
                  setRestoring(b);
                }}
              >
                <ArchiveRestore size={14} /> Restore
              </Button>
              {deleting === b.path ? (
                <Button size="sm" variant="danger" disabled={remove.isPending} onClick={() => remove.mutate(b.path)}>
                  Delete?
                </Button>
              ) : (
                <Button size="sm" variant="ghost" aria-label={`Delete the backup of ${b.database} from ${when(b.created)}`} onClick={() => setDeleting(b.path)}>
                  <Trash2 size={14} />
                </Button>
              )}
            </li>
          ))}
        </ul>
      )}

      {restoring && (
        <Dialog
          open
          onClose={() => !restore.isPending && setRestoring(null)}
          title={`Restore ${restoring.database}`}
          icon={<ArchiveRestore size={18} />}
          footer={
            <>
              <Button onClick={() => setRestoring(null)} disabled={restore.isPending}>
                Cancel
              </Button>
              <Button
                variant="primary"
                disabled={restore.isPending || !validTarget}
                onClick={() => restore.mutate({ path: restoring.path, database: target })}
              >
                {restore.isPending ? "Restoring…" : "Restore"}
              </Button>
            </>
          }
        >
          <p className="text-sm text-muted">
            {files
              ? `The backup from ${when(restoring.created)} replaces this file in www (or creates it). Stop the app that uses it first, or the copy may fail.`
              : `The backup from ${when(restoring.created)} goes into this ${label} database. It's created if it doesn't exist; tables that are in the backup are replaced.`}
          </p>
          <label className="mt-4 block">
            <span className="mb-1.5 block text-sm font-semibold">{files ? "File in www" : "Database"}</span>
            <input data-autofocus className={`${inputClass} font-mono`} value={target} onChange={(e) => setTarget(e.target.value)} aria-label="Database to restore into" />
          </label>
        </Dialog>
      )}
    </Card>
  );
}
