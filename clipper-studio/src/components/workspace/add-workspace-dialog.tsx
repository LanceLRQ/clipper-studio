import { useState } from "react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  WorkspaceStep,
  type WorkspaceStepMode,
} from "@/components/onboarding/WorkspaceStep";

interface AddWorkspaceDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onCreated?: (workspaceId: string) => void;
}

export function AddWorkspaceDialog({
  open,
  onOpenChange,
  onCreated,
}: AddWorkspaceDialogProps) {
  const [mode, setMode] = useState<WorkspaceStepMode>("choose");

  const handleOpenChange = (next: boolean) => {
    if (!next) setMode("choose");
    onOpenChange(next);
  };

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="max-w-xl sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>添加工作区</DialogTitle>
        </DialogHeader>
        {open && (
          <WorkspaceStep
            mode={mode}
            onModeChange={setMode}
            onCreated={(wsId) => {
              onCreated?.(wsId);
              handleOpenChange(false);
            }}
          />
        )}
      </DialogContent>
    </Dialog>
  );
}
