import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { AlertTriangle, Check, FolderInput, FolderOpen, Loader2, Square } from "lucide-react";
import clsx from "clsx";
import { api, type MoveCheck } from "../lib/api";
import { useAppStore } from "../lib/store";
import Dialog from "./Dialog";
import { Button, inputClass } from "./ui";

const human = (bytes: number) => (bytes >= 1024 ** 3 ? `${(bytes / 1024 ** 3).toFixed(1)} GB` : `${Math.round(bytes / 1024 ** 2)} MB`);

type Phase = { kind: "pick" } | { kind: "moving"; done: number; total: number } | { kind: "done"; to: string } | { kind: "failed"; error: string };

/**
 * Moves the LampRabit folder: pick a folder, see what's in the way (problems with the
 * target, programs using the current folder), then move with progress. Mounted only while
 * open, so it starts fresh each time.
 */
export default function MoveEnvironmentDialog({ current, onClose }: { current: string; onClose: () => void }) {
  const queryClient = useQueryClient();
  const notify = useAppStore((s) => s.notify);
  const [target, setTarget] = useState("");
  const [check, setCheck] = useState<MoveCheck | null>(null);
  const [checking, setChecking] = useState(false);
  const [phase, setPhase] = useState<Phase>({ kind: "pick" });
  const holders = useQuery({ queryKey: ["environment-in-use"], queryFn: api.environmentInUse, refetchInterval: phase.kind === "pick" ? 3000 : false });

  // Checks the folder shortly after typing stops.
  useEffect(() => {
    if (!target.trim()) {
      setCheck(null);
      return;
    }
    setChecking(true);
    const timer = setTimeout(() => {
      api
        .checkMoveTarget(target.trim())
        .then(setCheck)
        .catch((e) => setCheck({ problems: [String(e)], size: 0, free: null, sameDrive: false, fileSystem: null }))
        .finally(() => setChecking(false));
    }, 400);
    return () => clearTimeout(timer);
  }, [target]);

  useEffect(() => {
    const off = listen<{ done: number; total: number }>("move-progress", ({ payload }) =>
      setPhase((p) => (p.kind === "moving" ? { kind: "moving", done: payload.done, total: payload.total } : p)),
    );
    return () => {
      off.then((f) => f());
    };
  }, []);

  const browse = async () => {
    const dir = await open({ directory: true, title: "Choose where the LampRabit folder goes" });
    if (typeof dir === "string") setTarget(dir);
  };
  const stop = (pid: number) =>
    api
      .stopEnvironmentHolder(pid)
      .then(() => holders.refetch())
      .catch((e) => notify("error", String(e)));
  const move = async () => {
    setPhase({ kind: "moving", done: 0, total: check?.size ?? 0 });
    try {
      const settings = await api.moveEnvironment(target.trim());
      setPhase({ kind: "done", to: settings.rootDir });
      queryClient.invalidateQueries();
    } catch (e) {
      setPhase({ kind: "failed", error: String(e) });
      holders.refetch();
    }
  };

  const busy = holders.data ?? [];
  const ready = !!check && check.problems.length === 0 && !checking && busy.length === 0;
  const moving = phase.kind === "moving";

  return (
    <Dialog
      open
      onClose={() => !moving && onClose()}
      title="Move the LampRabit folder"
      icon={<FolderInput size={18} />}
      width="max-w-xl"
      footer={
        phase.kind === "done" ? (
          <Button variant="primary" onClick={onClose}>
            Done
          </Button>
        ) : (
          <>
            <Button onClick={onClose} disabled={moving}>
              Cancel
            </Button>
            <Button variant="primary" onClick={move} disabled={!ready || moving}>
              {moving ? <Loader2 size={15} className="animate-spin" /> : <FolderInput size={15} />} {moving ? "Moving…" : "Move"}
            </Button>
          </>
        )
      }
    >
      {phase.kind === "done" ? (
        <div className="space-y-2 text-sm">
          <p className="flex items-center gap-2 font-semibold text-accent-text">
            <Check size={16} /> Moved to <span className="font-mono">{phase.to}</span>
          </p>
          <p className="text-muted">Services were stopped for the move; start the ones you need. Their configs follow the new folder on their own.</p>
        </div>
      ) : (
        <div className="space-y-4 text-sm">
          <p className="text-muted">
            Everything moves together: runtimes, configs, your projects (www), databases (data), backups and templates. Services and running projects stop first. Now at{" "}
            <span className="font-mono text-fg">{current}</span>.
          </p>
          <label className="block">
            <span className="mb-1.5 block font-semibold">New location</span>
            <span className="flex gap-2">
              <input data-autofocus className={clsx(inputClass, "font-mono")} value={target} onChange={(e) => setTarget(e.target.value)} placeholder="D:\LampRabit" disabled={moving} />
              <Button className="h-auto" onClick={browse} disabled={moving}>
                <FolderOpen size={15} /> Browse…
              </Button>
            </span>
          </label>

          {checking && <p className="flex items-center gap-2 text-muted"><Loader2 size={14} className="animate-spin" /> Checking that folder…</p>}
          {check && !checking && (
            check.problems.length > 0 ? (
              <ul className="space-y-1.5 rounded-lg bg-danger-soft px-3 py-2 text-danger-text">
                {check.problems.map((p) => (
                  <li key={p} className="flex items-start gap-2">
                    <AlertTriangle size={14} className="mt-0.5 shrink-0" aria-hidden /> {p}
                  </li>
                ))}
              </ul>
            ) : (
              <p className="flex items-start gap-2 rounded-lg bg-surface-2 px-3 py-2">
                <Check size={14} className="mt-0.5 shrink-0 text-accent-text" aria-hidden />
                <span>
                  {human(check.size)} to move{check.fileSystem && ` to a ${check.fileSystem} drive`}
                  {check.free !== null && ` with ${human(check.free)} free`}.{" "}
                  {check.sameDrive ? "Same drive: it's a quick rename." : "Another drive: it's copied, checked, then removed from here."}
                </span>
              </p>
            )
          )}

          {busy.length > 0 && (
            <div className="rounded-lg border border-warn/40 bg-warn/10 p-3">
              <p className="font-semibold">Programs using the folder</p>
              <p className="text-xs text-muted">They have to stop before the folder can move. Close them yourself, or stop them here (unsaved work in them is lost).</p>
              <ul className="mt-2 space-y-1.5">
                {busy.map((h) => (
                  <li key={h.pid} className="flex items-center gap-3">
                    <span className="min-w-0 flex-1">
                      <span className="font-medium">{h.name}</span> <span className="text-xs text-muted">PID {h.pid}</span>
                      <span className="block truncate font-mono text-xs text-muted" title={h.path}>{h.path}</span>
                    </span>
                    <Button size="sm" variant="danger" onClick={() => stop(h.pid)}>
                      <Square size={12} /> Stop it
                    </Button>
                  </li>
                ))}
              </ul>
            </div>
          )}

          {phase.kind === "moving" && (
            <div>
              <div className="h-2 overflow-hidden rounded-full bg-surface-2">
                <div className="h-full bg-accent transition-[width]" style={{ width: `${phase.total ? Math.min(100, (phase.done / phase.total) * 100) : 5}%` }} />
              </div>
              <p className="mt-1 text-xs text-muted">{phase.total ? `${human(phase.done)} of ${human(phase.total)}` : "Moving…"}</p>
            </div>
          )}
          {phase.kind === "failed" && <p className="rounded-lg bg-danger-soft px-3 py-2 whitespace-pre-wrap text-danger-text">{phase.error}</p>}
        </div>
      )}
    </Dialog>
  );
}
