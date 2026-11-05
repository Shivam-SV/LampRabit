import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { FolderInput, FolderOpen } from "lucide-react";
import { api } from "../lib/api";
import { useAppStore } from "../lib/store";
import Dialog from "./Dialog";
import MoveEnvironmentDialog from "./MoveEnvironmentDialog";
import { Button } from "./ui";

/**
 * The first run asks where the LampRabit folder goes, before anything is downloaded into
 * it. Choosing another place is the same move Settings → General offers (with its checks:
 * write access, free space, FAT drives…), done while the folder is still nearly empty.
 */
export default function FirstRunDialog() {
  const queryClient = useQueryClient();
  const notify = useAppStore((s) => s.notify);
  const first = useQuery({ queryKey: ["first-run"], queryFn: api.firstRun, staleTime: Infinity });
  const settings = useQuery({ queryKey: ["settings"], queryFn: api.getSettings, enabled: !!first.data });
  const [moving, setMoving] = useState(false);
  const finish = useMutation({
    mutationFn: api.finishFirstRun,
    onSuccess: () => queryClient.setQueryData(["first-run"], false),
    onError: (e) => notify("error", String(e)),
  });
  if (!first.data || !settings.data) return null;
  const root = settings.data.rootDir;

  if (moving) {
    return (
      <MoveEnvironmentDialog
        current={root}
        onClose={() => {
          setMoving(false);
          // A finished move changed the folder; show where it is now.
          queryClient.invalidateQueries({ queryKey: ["settings"] });
        }}
      />
    );
  }
  return (
    <Dialog
      open
      onClose={() => finish.mutate()}
      title="Where should LampRabit keep its files?"
      icon={<FolderOpen size={18} />}
      width="max-w-lg"
      footer={
        <>
          <Button onClick={() => setMoving(true)} className="mr-auto">
            <FolderInput size={15} /> Choose another folder…
          </Button>
          <Button variant="primary" onClick={() => finish.mutate()} disabled={finish.isPending} data-autofocus>
            Use this folder
          </Button>
        </>
      }
    >
      <div className="space-y-3 text-sm">
        <p className="text-muted">
          Everything goes in one folder: the runtimes LampRabit downloads, their configs, your projects (<span className="font-mono">www</span>) and databases (
          <span className="font-mono">data</span>). It can grow to several gigabytes, so pick a drive with room.
        </p>
        <p className="rounded-lg bg-surface-2 px-3 py-2 font-mono break-all text-fg">{root}</p>
        <p className="text-xs text-muted">You can move it later in Settings → General.</p>
      </div>
    </Dialog>
  );
}
