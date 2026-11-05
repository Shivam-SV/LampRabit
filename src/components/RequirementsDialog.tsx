import { CheckCircle2, CircleAlert, Download, Link2 } from "lucide-react";
import type { Requirement } from "../lib/api";
import { needName, plan } from "../lib/catalog";
import Dialog from "./Dialog";
import { Button } from "./ui";

/**
 * Asked before installing something that needs other services: what each need is, which
 * version will be used, and one "Install all". Mount only while open.
 */
export default function RequirementsDialog({
  name,
  requirements,
  busy,
  onConfirm,
  onClose,
}: {
  /** "Tomcat 11.0.14" */
  name: string;
  requirements: Requirement[];
  busy: boolean;
  onConfirm: () => void;
  onClose: () => void;
}) {
  const blocked = requirements.filter((r) => r.action.kind === "unavailable");
  return (
    <Dialog
      open
      onClose={() => !busy && onClose()}
      title={`${name} needs more`}
      icon={<Link2 size={18} />}
      footer={
        <>
          <Button onClick={onClose} disabled={busy}>
            Cancel
          </Button>
          <Button variant="primary" onClick={onConfirm} disabled={busy || blocked.length > 0} data-autofocus>
            <Download size={14} /> {busy ? "Installing…" : "Install all"}
          </Button>
        </>
      }
    >
      <p className="text-sm text-muted">
        {blocked.length > 0 ? `${name} can't be set up here yet:` : `These are set up first, in this order, then ${name}:`}
      </p>
      <ul className="mt-3 space-y-2">
        {requirements.map((r) => {
          const ok = r.action.kind === "ready";
          const bad = r.action.kind === "unavailable";
          return (
            <li key={r.runtime} className="flex gap-3 rounded-lg border border-line bg-surface-2/50 px-3 py-2.5">
              {bad ? (
                <CircleAlert size={16} className="mt-0.5 shrink-0 text-danger-text" aria-hidden />
              ) : (
                <CheckCircle2 size={16} className={ok ? "mt-0.5 shrink-0 text-accent-text" : "mt-0.5 shrink-0 text-muted"} aria-hidden />
              )}
              <div className="min-w-0">
                <div className="text-sm font-semibold">{needName(r)}</div>
                <div className="text-xs text-muted">{r.why}</div>
                <div className={bad ? "mt-1 text-xs text-danger-text" : "mt-1 text-xs"}>{plan(r)}</div>
              </div>
            </li>
          );
        })}
      </ul>
    </Dialog>
  );
}
