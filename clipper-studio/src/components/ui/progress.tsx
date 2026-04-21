import { cn } from "@/lib/utils";

interface ProgressProps {
  value: number;
  className?: string;
  indicatorClassName?: string;
}

export function Progress({ value, className, indicatorClassName }: ProgressProps) {
  const pct = Math.max(0, Math.min(100, value));
  return (
    <div
      className={cn(
        "relative h-2 w-full overflow-hidden rounded-full bg-secondary",
        className
      )}
    >
      <div
        className={cn(
          "h-full bg-primary transition-all duration-200 ease-out",
          indicatorClassName
        )}
        style={{ width: `${pct}%` }}
      />
    </div>
  );
}
