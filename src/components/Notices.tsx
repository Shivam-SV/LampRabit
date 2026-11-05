import { X } from "lucide-react";
import clsx from "clsx";
import { useAppStore } from "../lib/store";

export default function Notices() {
  const notices = useAppStore((s) => s.notices);
  const dismiss = useAppStore((s) => s.dismiss);

  return (
    <div className="pointer-events-none fixed right-4 bottom-4 flex w-80 flex-col gap-2">
      {notices.map((n) => (
        <div
          key={n.id}
          className={clsx(
            "motion-toast pointer-events-auto flex items-start gap-2 rounded-lg border px-3 py-2 text-sm shadow-lg",
            n.tone === "error"
              ? "border-danger/30 bg-danger-soft text-danger-text backdrop-blur"
              : "border-line bg-surface text-fg",
          )}
        >
          <span className="selectable min-w-0 flex-1 break-words">
            {n.text}
            {n.detail && n.detail.length > 0 && (
              <span className="mt-1.5 block rounded bg-terminal px-2 py-1.5 font-mono text-[11px] leading-snug break-all whitespace-pre-wrap text-slate-300">{n.detail.join("\n")}</span>
            )}
          </span>
          <button onClick={() => dismiss(n.id)} className="opacity-60 hover:opacity-100" aria-label="Dismiss">
            <X size={14} />
          </button>
        </div>
      ))}
    </div>
  );
}
