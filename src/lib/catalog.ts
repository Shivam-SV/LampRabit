import { useQuery } from "@tanstack/react-query";
import { api, runtimeLabels, type Requirement, type Runtime, type RuntimeInfo } from "./api";

/** The backend's runtime list: groups, labels, sources, platform gaps and companions. */
export function useCatalog() {
  return useQuery({ queryKey: ["runtime-info"], queryFn: api.runtimeInfo, staleTime: Infinity });
}

export function infoFor(catalog: { runtimes: RuntimeInfo[] } | undefined, id: string): RuntimeInfo | undefined {
  return catalog?.runtimes.find((r) => r.id === id);
}

/** The backend's label, falling back to the frontend's while the catalog loads. */
export function labelFor(catalog: { runtimes: RuntimeInfo[] } | undefined, id: string): string {
  if (id === "node") return "Node.js";
  return infoFor(catalog, id)?.label ?? runtimeLabels[id as Runtime] ?? id;
}

/** Needs that something has to happen for (the rest are met already). */
export const unmet = (list: Requirement[] | undefined) => (list ?? []).filter((r) => r.action.kind !== "ready");

/** "Java 17 or newer" */
export const needName = (r: Requirement) => (r.range ? `${r.label} ${r.range}` : r.label);

/** What "Install all" does for one need, in a few words. */
export function plan(r: Requirement): string {
  switch (r.action.kind) {
    case "ready":
      return `${r.label} ${r.action.version} is in use`;
    case "activate":
      return `Switch to the installed ${r.label} ${r.action.version}`;
    case "install":
      return `Install ${r.label} ${r.action.release.version} and use it`;
    case "installNode":
      return `Install Node.js ${r.action.version} with NVM and make it the default`;
    case "useNode":
      return `Make the installed Node.js ${r.action.version} NVM's default`;
    case "unavailable":
      return r.action.reason;
  }
}

/** One sentence for a service's card: what's missing, or nothing when all is met. */
export function needsNote(list: Requirement[] | undefined): string | undefined {
  const missing = unmet(list);
  if (missing.length === 0) return undefined;
  return `Needs ${missing.map(needName).join(" and ")}. Installing it sets ${missing.length === 1 ? "that" : "them"} up too.`;
}

/**
 * Meets each need in order: switches or installs the version that fits and makes it the
 * one in use, so the runtime installed next starts on it.
 */
export async function meet(list: Requirement[]): Promise<void> {
  for (const r of list) {
    const a = r.action;
    switch (a.kind) {
      case "activate":
        await api.setActiveVersion(r.runtime as Runtime, a.version);
        break;
      case "install":
        await api.installRuntime(a.release);
        await api.setActiveVersion(a.release.runtime, a.release.version);
        break;
      case "installNode":
        await api.nodeInstall(a.version);
        await api.nodeUse(a.version);
        break;
      case "useNode":
        await api.nodeUse(a.version);
        break;
      case "unavailable":
        throw new Error(`${needName(r)}: ${a.reason}`);
    }
  }
}
