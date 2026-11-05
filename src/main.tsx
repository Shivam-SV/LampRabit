import React from "react";
import ReactDOM from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import App from "./App";
import { loadLabels } from "./lib/api";
import "./lib/motion";
import "./index.css";

// The webview's own right-click menu (Back, Reload, Inspect…) is a browser's, not the
// app's; it stays in development for inspecting the UI. Text fields keep it, for
// cut, copy and paste.
if (!import.meta.env.DEV) {
  document.addEventListener("contextmenu", (e) => {
    const target = e.target as HTMLElement | null;
    if (!target?.closest("input, textarea, [contenteditable='true']")) e.preventDefault();
  });
}

const queryClient = new QueryClient({
  defaultOptions: { queries: { refetchOnWindowFocus: false, retry: false } },
});

// Runtime and service names come from the backend; without them ids show until a reload.
loadLabels()
  .catch(() => {})
  .finally(() =>
    ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
      <React.StrictMode>
        <QueryClientProvider client={queryClient}>
          <App />
        </QueryClientProvider>
      </React.StrictMode>,
    ),
  );
