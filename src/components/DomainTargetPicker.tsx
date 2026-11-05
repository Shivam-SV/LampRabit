import { useQuery } from "@tanstack/react-query";
import clsx from "clsx";
import { api, type DomainTarget } from "../lib/api";
import { inputClass } from "./ui";

type Kind = DomainTarget["kind"];

const narrow = (width: string) => clsx(inputClass.replace("w-full", ""), width);

/** The project's folders, for the folder pickers; refreshed when a picker opens again. */
export function useProjectFolders(project: string) {
  return useQuery({ queryKey: ["project-folders", project], queryFn: () => api.projectFolders(project), staleTime: 10_000 });
}

/** A folder of the project: the default (labelled `none`) or one of its folders. */
export function FolderSelect({
  project,
  value,
  onChange,
  none,
  label,
}: {
  project: string;
  value: string;
  onChange: (v: string) => void;
  /** What "" means here: "Project folder", "Same folder as the main domain (public/)". */
  none: string;
  label: string;
}) {
  const folders = useProjectFolders(project).data ?? [];
  // A saved folder that's gone still shows, so the choice isn't silently changed.
  const options = value && !folders.includes(value) ? [value, ...folders] : folders;
  return (
    <select className={narrow("w-56 font-mono")} value={value} onChange={(e) => onChange(e.target.value)} aria-label={label}>
      <option value="">{none}</option>
      {options.map((f) => (
        <option key={f} value={f}>
          {f}/
        </option>
      ))}
    </select>
  );
}

/** What each choice does, shown under the picker. */
export function targetHint(target: DomainTarget, automatic: boolean): string {
  switch (target.kind) {
    case "main":
      return automatic
        ? "Follows what's running: the dev server while it listens, the project's files otherwise."
        : "Goes wherever the main domain goes right now.";
    case "files":
      return "Served from the project's files by the web server, PHP included.";
    case "port":
      return "Forwarded to a local port, whoever listens on it.";
    case "process":
      return "Forwarded to one of the project's processes, at its port.";
  }
}

/**
 * Where a domain goes: the same choices for the main domain (with "Automatic") and extra
 * ones (with "Same as main"), then the folder, port or process that goes with the choice.
 */
export default function DomainTargetPicker({
  project,
  value,
  onChange,
  processes,
  automatic,
  label,
}: {
  project: string;
  value: DomainTarget;
  onChange: (v: DomainTarget) => void;
  /** Process names that have a port. */
  processes: string[];
  /** The main domain: "main" reads as Automatic. */
  automatic: boolean;
  label: string;
}) {
  const kinds: { value: Kind; label: string }[] = [
    { value: "main", label: automatic ? "Automatic" : "Same as main" },
    { value: "files", label: "Files" },
    { value: "port", label: "Port" },
    { value: "process", label: processes.length ? "Process" : "Process (none with a port)" },
  ];
  const change = (kind: Kind) => {
    if (kind === value.kind) return;
    if (kind === "main") onChange({ kind });
    else if (kind === "files") onChange({ kind, root: "" });
    else if (kind === "port") onChange({ kind, port: 3001 });
    else onChange({ kind, name: processes[0] ?? "" });
  };
  return (
    <div className="flex flex-wrap items-center gap-2">
      <select className={narrow("w-44")} value={value.kind} onChange={(e) => change(e.target.value as Kind)} aria-label={label}>
        {kinds.map((k) => (
          <option key={k.value} value={k.value} disabled={k.value === "process" && processes.length === 0}>
            {k.label}
          </option>
        ))}
      </select>
      {value.kind === "files" && (
        <FolderSelect
          project={project}
          value={value.root}
          onChange={(root) => onChange({ kind: "files", root })}
          none={automatic ? "Project's web folder" : "Same folder as the main domain"}
          label={`${label}: folder`}
        />
      )}
      {value.kind === "port" && (
        <input
          type="number"
          min={1}
          max={65535}
          className={narrow("w-28 font-mono")}
          value={value.port || ""}
          onChange={(e) => onChange({ kind: "port", port: Number(e.target.value) })}
          aria-label={`${label}: port`}
        />
      )}
      {value.kind === "process" && (
        <select className={narrow("w-40")} value={value.name} onChange={(e) => onChange({ kind: "process", name: e.target.value })} aria-label={`${label}: process`}>
          {processes.map((p) => (
            <option key={p} value={p}>
              {p}
            </option>
          ))}
        </select>
      )}
    </div>
  );
}
