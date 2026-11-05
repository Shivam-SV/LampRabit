import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { allServers, api, type Server, type ServiceInfo, type ServiceKind, type Settings } from "./api";
import { useAppStore } from "./store";

/** Services shown (and started by "Start all") with these settings: web servers only while enabled. */
export function serverEnabled(kind: ServiceKind, servers: Settings["webServers"] | undefined) {
  if (!isServer(kind)) return true;
  return (servers ?? ["apache"]).includes(kind);
}

export const isServer = (kind: ServiceKind): kind is Server => (allServers as string[]).includes(kind);

/** The server with the main ports, localhost and the developer tools. */
export const primaryServer = (settings: Settings | undefined): Server => settings?.webServers[0] ?? "apache";

/**
 * Starting and stopping groups of services: the active stack profile, a selection, or
 * everything. Shared by the Home header, the title bar and the Shift+S / Shift+X / Shift+R keys.
 */
export function useStack() {
  const queryClient = useQueryClient();
  const notify = useAppStore((s) => s.notify);
  const activeName = useAppStore((s) => s.activeProfile);
  const services = useQuery({ queryKey: ["services"], queryFn: api.listServices });
  const settings = useQuery({ queryKey: ["settings"], queryFn: api.getSettings });
  const profile = settings.data?.profiles.find((p) => p.name === activeName) ?? null;

  const usable = (services.data ?? []).filter((s) => s.installed && serverEnabled(s.kind, settings.data?.webServers));
  const running = usable.filter((s) => s.state === "running" || s.state === "starting");
  // What Stop all can stop: not a service another app runs (Ollama's own app).
  const stoppable = running.filter((s) => !s.external);

  // A profile's project processes, and the ones other profiles start (stopped on a switch).
  const profileProcesses = profile?.processes ?? [];
  const otherProcesses = (settings.data?.profiles ?? []).flatMap((p) => p.processes ?? []).filter((k) => !profileProcesses.includes(k));

  const apply = useMutation({
    mutationFn: async ({ start, stop, processes }: { start: ServiceKind[]; stop: ServiceKind[]; processes?: { start: string[]; stop: string[] } }) => {
      const errors = await api.applyServices(start, stop);
      // Services first: a queue worker wants its database running.
      if (processes && (processes.start.length || processes.stop.length)) errors.push(...(await api.applyProcesses(processes.start, processes.stop)));
      return errors;
    },
    // Home shows each failure on its service's row; elsewhere a toast says it.
    onSuccess: (errors) => useAppStore.getState().page !== "overview" && errors.forEach((e) => notify("error", e)),
    onError: (e) => notify("error", String(e)),
    onSettled: () => queryClient.invalidateQueries({ queryKey: ["services"] }),
  });

  // Restarts one at a time, so each failure is reported for its own service.
  const restart = useMutation({
    mutationFn: async (list: ServiceKind[]) => {
      const errors: string[] = [];
      for (const kind of list) await api.restartService(kind).catch((e) => errors.push(String(e)));
      return errors;
    },
    onSuccess: (errors) => errors.forEach((e) => notify("error", e)),
    onSettled: () => queryClient.invalidateQueries({ queryKey: ["services"] }),
  });

  const kinds = (list: ServiceInfo[]) => list.map((s) => s.kind);
  const wanted = profile ? profile.services : kinds(usable);
  // Services not installed (or a disabled web server) can't run, so they don't count.
  const isUp = (list: ServiceKind[]) => {
    const relevant = list.filter((k) => usable.some((s) => s.kind === k));
    return relevant.length > 0 && relevant.every((k) => running.some((s) => s.kind === k));
  };
  return {
    profile,
    usable,
    running,
    stoppable,
    busy: apply.isPending || restart.isPending,
    /** Everything Start would start is already running: offer Restart instead. */
    up: isUp(wanted),
    /** Whether every installed service of `list` (all of them when empty) is running. */
    isUp: (list: ServiceKind[]) => isUp(list.length ? list : kinds(usable)),
    restartRunning: () => restart.mutate(kinds(running)),
    start: (list: ServiceKind[]) => apply.mutate({ start: list, stop: [] }),
    stop: (list: ServiceKind[]) => apply.mutate({ start: [], stop: list }),
    /** The active profile's services, stopping running ones outside it; otherwise everything. */
    startStack: () =>
      profile
        ? apply.mutate({
            start: profile.services,
            stop: kinds(running).filter((k) => !profile.services.includes(k)),
            processes: { start: profileProcesses, stop: otherProcesses },
          })
        : apply.mutate({ start: kinds(usable), stop: [] }),
    /** Every installed service, whatever profile is active. */
    startAll: () => apply.mutate({ start: kinds(usable), stop: [] }),
    startProfile: (services: ServiceKind[]) =>
      apply.mutate({ start: services, stop: kinds(running).filter((k) => !services.includes(k)) }),
    /** Every running service, and the active profile's processes. */
    stopAll: () => apply.mutate({ start: [], stop: kinds(stoppable), processes: { start: [], stop: profileProcesses } }),
  };
}
