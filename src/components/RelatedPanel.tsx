import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Download, Link2 } from "lucide-react";
import { api, type InstalledRuntime, type Requirement, type Runtime } from "../lib/api";
import { infoFor, labelFor, meet, needName, plan, unmet, useCatalog } from "../lib/catalog";
import { installKey, useAppStore } from "../lib/store";
import { tileFor } from "./runtimes";
import { Button, Card } from "./ui";

/**
 * What a service needs (in the versions that fit it, from the backend) and what goes well
 * with it, each with an install button and one "Install all". Things already in place
 * aren't listed; the panel hides when nothing is left.
 */
export default function RelatedPanel({ runtime, installed, onSelect }: { runtime: Runtime; installed?: InstalledRuntime[]; onSelect: (r: Runtime) => void }) {
  const queryClient = useQueryClient();
  const notify = useAppStore((s) => s.notify);
  const installs = useAppStore((s) => s.installs);
  const catalog = useCatalog();
  const info = infoFor(catalog.data, runtime);
  const has = (r: string) => !!installed?.find((i) => i.runtime === r)?.active;
  const requirements = useQuery({
    queryKey: ["requirements", runtime],
    queryFn: () => api.runtimeRequirements(runtime),
    enabled: (info?.needs.length ?? 0) > 0,
  });
  const needs = unmet(requirements.data);
  // Companions only make sense once the service itself is there.
  const suggests = has(runtime) ? (info?.suggests ?? []).filter((r) => !has(r)) : [];

  const install = useMutation({
    // One at a time, needs first, so pgAdmin finds its Python already installed.
    mutationFn: async ({ needs, suggests }: { needs: Requirement[]; suggests: Runtime[] }) => {
      await meet(needs);
      for (const r of suggests) {
        const catalog = await api.runtimeCatalog(r);
        const release = catalog.find((c) => c.label === "LTS") ?? catalog[0];
        if (!release) throw new Error(`No ${labelFor(undefined, r)} download for this platform`);
        await api.installRuntime(release);
      }
    },
    onSuccess: (_, { needs, suggests }) => notify("info", `Set up ${[...needs.map((n) => n.label), ...suggests.map((r) => labelFor(catalog.data, r))].join(", ")}`),
    onError: (e) => notify("error", String(e)),
    onSettled: () => {
      for (const key of ["installed", "services", "settings", "sites", "requirements"]) queryClient.invalidateQueries({ queryKey: [key] });
    },
  });

  if (needs.length === 0 && suggests.length === 0) return null;
  const busy = (id: string) => install.isPending && (install.variables?.suggests.includes(id as Runtime) || install.variables?.needs.some((n) => n.runtime === id));
  const progress = (id: string) => Object.keys(installs).some((k) => k.startsWith(installKey(id as Runtime, "")));
  const blocked = needs.some((n) => n.action.kind === "unavailable");

  const item = (id: string, name: string, title: string | undefined, onInstall: () => void, disabled = false) => (
    <li key={id} className="flex items-center gap-1 rounded-lg border border-line bg-surface-2/50 py-1 pr-1 pl-2" title={title}>
      <button
        onClick={() => id !== "node" && onSelect(id as Runtime)}
        className="flex items-center gap-2 text-sm font-medium hover:text-accent-text"
        title={`Show ${labelFor(catalog.data, id)}`}
      >
        <span className={`flex size-6 items-center justify-center rounded-md ${tileFor(id).className}`}>{tileFor(id).icon(14)}</span>
        {name}
      </button>
      <Button size="sm" variant="ghost" disabled={install.isPending || disabled} onClick={onInstall} aria-label={`Install ${name}`}>
        {busy(id) || progress(id) ? "Installing…" : <Download size={14} />}
      </Button>
    </li>
  );

  return (
    <Card className="mt-4 overflow-hidden">
      <div className="flex flex-wrap items-center gap-3 border-b border-line px-5 py-3">
        <Link2 size={16} className="text-muted" aria-hidden />
        <h3 className="min-w-0 flex-1 text-sm font-bold">Related services</h3>
        <Button size="sm" variant="soft" disabled={install.isPending || blocked} onClick={() => install.mutate({ needs, suggests })}>
          <Download size={14} /> {install.isPending ? "Installing…" : `Install all (${needs.length + suggests.length})`}
        </Button>
      </div>
      {needs.length > 0 && (
        <div className="px-5 py-3">
          <div className="mb-2 text-xs font-bold tracking-wider text-muted uppercase">Needs</div>
          <p className="mb-3 text-xs text-muted">
            {labelFor(catalog.data, runtime)} can't run without {needs.length === 1 ? "this" : "these"}, in the versions that fit it.
          </p>
          <ul className="flex flex-wrap gap-2">
            {needs.map((n) => item(n.runtime, needName(n), plan(n), () => install.mutate({ needs: [n], suggests: [] }), n.action.kind === "unavailable"))}
          </ul>
        </div>
      )}
      {suggests.length > 0 && (
        <div className="px-5 py-3">
          <div className="mb-2 text-xs font-bold tracking-wider text-muted uppercase">Goes well with</div>
          <p className="mb-3 text-xs text-muted">Tools to look at its data and work with it.</p>
          <ul className="flex flex-wrap gap-2">
            {suggests.map((r) => item(r, labelFor(catalog.data, r), undefined, () => install.mutate({ needs: [], suggests: [r] })))}
          </ul>
        </div>
      )}
    </Card>
  );
}
