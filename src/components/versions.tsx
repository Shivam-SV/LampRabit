import { useState, type ReactNode } from "react";
import { ArrowDownToLine, Check, Trash2 } from "lucide-react";
import clsx from "clsx";
import { Button } from "./ui";

/**
 * Shared pieces of a runtime card on the Services page: the version in use as a large
 * tile, the other installed versions as smaller tiles, and the versions to download.
 */

/** The version in use. `children` are its actions (open folder…). */
export function ActiveTile({ version, details, children }: { version: string; details?: ReactNode; children?: ReactNode }) {
  return (
    <div className="flex flex-col rounded-2xl border border-line bg-surface p-5 shadow-card">
      <div className="flex items-center justify-between gap-3 border-b border-line pb-3">
        <h4 className="text-xs font-bold tracking-wider text-muted uppercase">Installed version</h4>
        <span className="inline-flex items-center gap-1.5 rounded-full border border-ok/20 bg-ok/10 px-2 py-1 text-xs font-semibold text-ok">
          <span className="size-1.5 rounded-full bg-ok" aria-hidden /> Active
        </span>
      </div>
      <div className="mt-4 font-mono text-3xl font-bold tracking-tight text-fg">{version}</div>
      {details && <div className="mt-2 space-y-1 text-xs text-muted">{details}</div>}
      {children && <div className="mt-auto flex flex-wrap justify-end gap-2 pt-5">{children}</div>}
    </div>
  );
}

export function NoActiveTile({ text }: { text: string }) {
  return (
    <div className="flex flex-col justify-center rounded-2xl border border-dashed border-line p-4 text-sm text-muted">
      <div className="font-semibold text-fg">Not installed</div>
      {text}
    </div>
  );
}

/** Another installed version: switch to it, or remove it (asks first). */
export function InstalledTile({
  version,
  label,
  name,
  busy,
  onUse,
  onRemove,
}: {
  version: string;
  label?: string | null;
  /** For screen readers: "PHP 8.3.36". */
  name: string;
  busy?: boolean;
  onUse: () => void;
  onRemove: () => void;
}) {
  const [confirm, setConfirm] = useState(false);
  return (
    <li className="relative flex h-28 w-40 flex-col items-center justify-center gap-2 rounded-xl border border-line bg-surface px-3 text-center">
      {confirm ? (
        <>
          <span className="text-xs text-muted">Delete {version}?</span>
          <div className="flex gap-1.5">
            <Button size="sm" variant="danger" onClick={onRemove} disabled={busy} autoFocus>
              Delete
            </Button>
            <Button size="sm" onClick={() => setConfirm(false)}>
              Keep
            </Button>
          </div>
        </>
      ) : (
        <>
          <button
            onClick={() => setConfirm(true)}
            aria-label={`Remove ${name}`}
            title="Remove"
            className="absolute top-1.5 right-1.5 rounded-md p-1.5 text-muted hover:bg-danger-soft hover:text-danger-text"
          >
            <Trash2 size={13} />
          </button>
          <div>
            <div className="font-mono text-lg font-semibold">{version}</div>
            {label && <div className="text-[11px] text-muted">{label}</div>}
          </div>
          <Button size="sm" variant="soft" onClick={onUse} disabled={busy} aria-label={`Use ${name}`}>
            Use
          </Button>
        </>
      )}
    </li>
  );
}

export function InstalledTiles({ children }: { children: ReactNode }) {
  return (
    <section>
      <h4 className="mb-2 text-xs font-bold tracking-wider text-muted uppercase">Also installed</h4>
      <ul className="flex flex-wrap gap-3">{children}</ul>
    </section>
  );
}

export interface AvailableItem {
  version: string;
  label?: string | null;
  /** Download progress, 0–100, or null while it can't be measured. Undefined when idle. */
  progress?: number | null;
  /** Shown instead of a percentage while not downloading ("verifying", "extracting"). */
  stage?: string;
}

/** Versions that can be installed, newest first. */
export function AvailableList({
  title,
  items,
  name,
  onInstall,
  loading,
  error,
  limit,
}: {
  title: string;
  items: AvailableItem[];
  /** For screen readers: "PHP". */
  name: string;
  onInstall: (version: string) => void;
  loading?: boolean;
  error?: unknown;
  /** Show this many until "Show all" (long lists such as Node's). */
  limit?: number;
}) {
  const [all, setAll] = useState(false);
  const shown = limit && !all ? items.slice(0, limit) : items;
  return (
    <section>
      <h4 className="mb-2 text-xs font-bold tracking-wider text-muted uppercase">{title}</h4>
      {loading && <p className="text-sm text-muted">Fetching versions…</p>}
      {!!error && <p className="text-sm text-danger-text">{String(error)}</p>}
      {!loading && !error && items.length === 0 && (
        title === "Available to download" ? (
          <div className="flex min-h-40 flex-col items-center justify-center rounded-xl border-2 border-dashed border-line bg-surface px-5 py-7 text-center">
            <span className="mb-3 flex size-10 items-center justify-center rounded-full bg-accent-soft text-accent-text"><Check size={20} /></span>
            <p className="font-semibold text-fg">All versions downloaded</p>
            <p className="mt-1 max-w-xs text-sm text-muted">You have every available version of {name} installed.</p>
          </div>
        ) : (
          <div className="rounded-xl border border-dashed border-line px-4 py-5 text-center text-sm text-muted">No versions are available right now.</div>
        )
      )}
      {shown.length > 0 && (
        <ul className="divide-y divide-line rounded-xl border border-line">
          {shown.map((r) => (
            <li key={r.version} className="flex items-center gap-3 px-3 py-2">
              <span className="w-24 shrink-0 font-mono text-sm font-semibold">{r.version}</span>
              <span className="min-w-0 flex-1 truncate text-xs text-muted">{r.label ?? ""}</span>
              {r.progress !== undefined ? (
                <div className="flex w-40 items-center gap-2" role="progressbar" aria-valuenow={r.progress ?? undefined} aria-label={`Installing ${name} ${r.version}`}>
                  <div className="h-1.5 flex-1 overflow-hidden rounded-full bg-surface-2">
                    <div
                      className={clsx("h-full rounded-full bg-accent transition-[width]", r.progress === null && "animate-pulse")}
                      style={{ width: r.progress === null ? "100%" : `${r.progress}%` }}
                    />
                  </div>
                  <span className="w-20 text-right text-xs text-muted capitalize">{r.progress === null ? (r.stage ?? "Installing") : `${r.progress}%`}</span>
                </div>
              ) : (
                <Button size="sm" variant="soft" onClick={() => onInstall(r.version)} aria-label={`Install ${name} ${r.version}`}>
                  <ArrowDownToLine size={13} /> Install
                </Button>
              )}
            </li>
          ))}
        </ul>
      )}
      {limit && items.length > limit && (
        <button onClick={() => setAll((a) => !a)} className="mt-2 text-xs font-semibold text-accent-text hover:underline">
          {all ? "Show fewer" : `Show all ${items.length} versions`}
        </button>
      )}
    </section>
  );
}

/** The two-column body every runtime card shares. */
export function VersionsLayout({ active, children }: { active: ReactNode; children: ReactNode }) {
  return (
    <div className="grid gap-5 p-6 lg:grid-cols-[minmax(15rem,18rem)_minmax(0,1fr)]">
      {active}
      <div className="min-w-0 space-y-4">{children}</div>
    </div>
  );
}
