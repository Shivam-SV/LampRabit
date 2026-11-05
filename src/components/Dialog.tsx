import { useEffect, useRef, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { X } from "lucide-react";

/**
 * A modal dialog: Escape and the backdrop close it, focus moves into it when it opens
 * and back to where it was when it closes, and Tab stays inside it.
 */
export default function Dialog({
  open,
  onClose,
  title,
  icon,
  children,
  footer,
  width = "max-w-lg",
}: {
  open: boolean;
  onClose: () => void;
  title: string;
  icon?: ReactNode;
  children: ReactNode;
  footer?: ReactNode;
  width?: string;
}) {
  const panel = useRef<HTMLDivElement>(null);
  // Callers often pass a new function each render; reading it through a ref keeps the
  // effect below from re-running (and moving focus) on every render.
  const close = useRef(onClose);
  close.current = onClose;

  useEffect(() => {
    if (!open) return;
    const before = document.activeElement as HTMLElement | null;
    const focusables = () =>
      Array.from(
        panel.current?.querySelectorAll<HTMLElement>("button, [href], input, select, textarea, [tabindex]:not([tabindex='-1'])") ?? [],
      ).filter((el) => !el.hasAttribute("disabled"));
    // A marked element, else the first field, else the first control after the close button.
    const first =
      panel.current?.querySelector<HTMLElement>("[data-autofocus]") ??
      panel.current?.querySelector<HTMLElement>("input, select, textarea") ??
      focusables()[1] ??
      focusables()[0];
    first?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        close.current();
      } else if (e.key === "Tab") {
        const list = focusables();
        if (list.length === 0) return;
        const [head, tail] = [list[0], list[list.length - 1]];
        if (e.shiftKey && document.activeElement === head) {
          e.preventDefault();
          tail.focus();
        } else if (!e.shiftKey && document.activeElement === tail) {
          e.preventDefault();
          head.focus();
        }
      }
    };
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("keydown", onKey);
      before?.focus?.();
    };
  }, [open]);

  if (!open) return null;
  return createPortal(
    <div className="motion-backdrop fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-6" onMouseDown={onClose}>
      <div
        ref={panel}
        role="dialog"
        aria-modal="true"
        aria-label={title}
        className={`motion-dialog flex max-h-full w-full ${width} flex-col rounded-2xl border border-line bg-surface shadow-float`}
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className="flex items-center gap-3 border-b border-line px-6 py-4">
          {icon && <span className="text-accent-text">{icon}</span>}
          <h2 className="flex-1 text-lg font-bold tracking-tight">{title}</h2>
          <button
            onClick={onClose}
            aria-label="Close"
            className="flex size-8 items-center justify-center rounded-lg text-muted hover:bg-surface-2 hover:text-fg"
          >
            <X size={17} />
          </button>
        </div>
        <div className="min-h-0 overflow-y-auto px-6 py-5">{children}</div>
        {footer && <div className="flex justify-end gap-2 border-t border-line px-6 py-4">{footer}</div>}
      </div>
    </div>,
    document.body,
  );
}

/** A key cap, for shortcut hints. */
export function Kbd({ children }: { children: ReactNode }) {
  return (
    <kbd className="inline-flex h-6 min-w-6 items-center justify-center rounded-md border border-line bg-surface-2 px-1.5 font-mono text-[11px] font-semibold text-fg shadow-[inset_0_-1px_0_var(--line)]">
      {children}
    </kbd>
  );
}
