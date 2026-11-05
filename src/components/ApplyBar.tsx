import { Check, Undo2 } from "lucide-react";
import { Button } from "./ui";
import { usePageActive } from "../lib/pageActive";

/**
 * Floats at the bottom right while a form has unsaved changes, above the tab bar, so
 * Apply is in reach wherever the page is scrolled. Running services that the change
 * affects are restarted by the backend when it's applied.
 */
export default function ApplyBar({
  dirty,
  busy,
  onApply,
  onDiscard,
  note = "Running services restart with the new settings.",
}: {
  dirty: boolean;
  busy: boolean;
  onApply: () => void;
  onDiscard: () => void;
  note?: string;
}) {
  // Its page may be kept in the background; the bar shows only over its own page.
  const pageActive = usePageActive();
  if ((!dirty && !busy) || !pageActive) return null;
  return (
    <div
      role="region"
      aria-label="Unsaved changes"
      className="motion-toast fixed right-4 bottom-20 z-20 flex max-w-[calc(100vw-2rem)] items-center gap-3 rounded-xl border border-line bg-surface px-4 py-3 shadow-lg"
    >
      <span className="size-2 shrink-0 rounded-full bg-warn" aria-hidden />
      <span className="min-w-0">
        <span className="block text-sm font-semibold">Unsaved changes</span>
        <span className="block truncate text-xs text-muted">{note}</span>
      </span>
      <Button size="sm" onClick={onDiscard} disabled={busy}>
        <Undo2 size={15} /> Discard
      </Button>
      <Button size="sm" variant="primary" onClick={onApply} disabled={busy}>
        <Check size={15} /> {busy ? "Applying…" : "Apply changes"}
      </Button>
    </div>
  );
}
