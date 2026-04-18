import { Check } from "lucide-react";
import { cn } from "@/lib/utils";

export interface WizardStepperItem {
  key: string;
  title: string;
  description?: string;
}

interface WizardStepperProps {
  steps: WizardStepperItem[];
  current: number; // 0-based index of the active step
}

export function WizardStepper({ steps, current }: WizardStepperProps) {
  return (
    <ol className="flex w-full items-start justify-between gap-2">
      {steps.map((step, idx) => {
        const state: "done" | "active" | "todo" =
          idx < current ? "done" : idx === current ? "active" : "todo";
        const isLast = idx === steps.length - 1;
        return (
          <li
            key={step.key}
            className={cn(
              "flex items-start gap-3",
              !isLast && "flex-1"
            )}
          >
            <div className="flex flex-col items-center">
              <div
                className={cn(
                  "flex h-8 w-8 items-center justify-center rounded-full border text-sm font-medium transition-colors",
                  state === "done" &&
                    "border-primary bg-primary text-primary-foreground",
                  state === "active" &&
                    "border-primary bg-background text-primary",
                  state === "todo" &&
                    "border-muted-foreground/30 bg-background text-muted-foreground"
                )}
              >
                {state === "done" ? (
                  <Check className="h-4 w-4" />
                ) : (
                  idx + 1
                )}
              </div>
            </div>
            <div className="flex-1 pt-0.5">
              <div
                className={cn(
                  "text-sm font-medium",
                  state === "todo" && "text-muted-foreground"
                )}
              >
                {step.title}
              </div>
              {step.description && (
                <div className="text-xs text-muted-foreground">
                  {step.description}
                </div>
              )}
            </div>
            {!isLast && (
              <div
                className={cn(
                  "mt-4 h-px flex-1 self-start",
                  idx < current
                    ? "bg-primary"
                    : "bg-muted-foreground/20"
                )}
              />
            )}
          </li>
        );
      })}
    </ol>
  );
}
