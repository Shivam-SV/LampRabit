import { createContext, useContext } from "react";

/**
 * Whether the page a component belongs to is the one on screen. Pages stay mounted when
 * you switch away (so they keep their state), so their keyboard shortcuts and floating
 * bars check this. Outside a page (the title bar, the tab bar) it's always true.
 */
export const PageActiveContext = createContext(true);

export const usePageActive = () => useContext(PageActiveContext);
