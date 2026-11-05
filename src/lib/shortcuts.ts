import { usePageActive } from "./pageActive";
import { useEffect, useRef } from "react";

/** Every shortcut, as shown in the "?" dialog. Handlers live with the screens they act on. */
export const SHORTCUTS: { group: string; items: { keys: string[]; label: string }[] }[] = [
  {
    group: "Anywhere",
    items: [
      { keys: ["?"], label: "Show keyboard shortcuts" },
      { keys: ["1"], label: "Home" },
      { keys: ["2"], label: "Sites" },
      { keys: ["3"], label: "Services" },
      { keys: ["4"], label: "Logs" },
      { keys: ["5"], label: "Settings" },
      { keys: ["Shift", "S"], label: "Start all (or the active profile)" },
      { keys: ["Shift", "X"], label: "Stop all" },
      { keys: ["Shift", "R"], label: "Restart the running services" },
      { keys: ["N"], label: "New project" },
      { keys: ["T"], label: "Open terminal" },
      { keys: ["W"], label: "Open the www folder" },
    ],
  },
  {
    group: "Home: services",
    items: [
      { keys: ["↓", "J"], label: "Next service" },
      { keys: ["↑", "K"], label: "Previous service" },
      { keys: ["Space"], label: "Select or unselect the service" },
      { keys: ["A"], label: "Select all, or none" },
      { keys: ["Enter"], label: "Start or stop the service" },
      { keys: ["S"], label: "Start selected (or this service)" },
      { keys: ["X"], label: "Stop selected (or this service)" },
      { keys: ["R"], label: "Restart the service" },
      { keys: ["O"], label: "Open it (browser, inbox, database tool)" },
      { keys: ["C"], label: "Copy its connection details" },
      { keys: ["E"], label: "Switch between URL and .env" },
      { keys: ["P"], label: "Choose a stack profile" },
      { keys: ["L"], label: "Its logs" },
    ],
  },
  {
    group: "Sites: a project",
    items: [
      { keys: ["["], label: "Previous tab (Overview, Commands, Domains, Settings)" },
      { keys: ["]"], label: "Next tab" },
    ],
  },

];

/** "shift+s", "?", "arrowdown", " "… for a key event; null when a modifier other than Shift is held. */
function combo(e: KeyboardEvent): string | null {
  if (e.ctrlKey || e.metaKey || e.altKey) return null;
  const key = e.key.toLowerCase();
  // Shift matters for letters only; "?" already is shift+/.
  return e.shiftKey && /^[a-z]$/.test(key) ? `shift+${key}` : key;
}

function typing(target: EventTarget | null) {
  const el = target as HTMLElement | null;
  if (!el) return false;
  if (el instanceof HTMLInputElement) return !["checkbox", "radio", "button"].includes(el.type);
  return el.isContentEditable || ["TEXTAREA", "SELECT"].includes(el.tagName);
}

/**
 * Runs `handlers[combo]` for key presses on the page. Keys are ignored while typing or while
 * a dialog or menu is open, and Enter/Space are left to buttons and links that have focus,
 * so normal keyboard use keeps working.
 */
export function useHotkeys(handlers: Record<string, (e: KeyboardEvent) => void>, enabled = true) {
  const ref = useRef(handlers);
  ref.current = handlers;
  // A page kept mounted in the background doesn't take keys.
  const pageActive = usePageActive();
  const on = enabled && pageActive;
  useEffect(() => {
    if (!on) return;
    const onKey = (e: KeyboardEvent) => {
      // A page's own handler (registered before the app-wide one) has taken the key.
      // Holding an arrow key keeps moving; holding anything else doesn't repeat the action.
      if (e.defaultPrevented || (e.repeat && e.key !== "ArrowDown" && e.key !== "ArrowUp")) return;
      // Open dialogs and menus handle their own keys.
      if (typing(e.target) || document.querySelector("[role=dialog], [role=menu]")) return;
      const key = combo(e);
      if (!key) return;
      const control = (e.target as HTMLElement | null)?.closest?.("button, a, input, [role=menuitem], [role=switch], [role=radio]");
      if ((key === "enter" || key === " ") && control) return;
      const handler = ref.current[key];
      if (handler) {
        e.preventDefault();
        handler(e);
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [on]);
}
