import { useState } from "react";
import { DayPicker, type DateRange } from "react-day-picker";
import { zhCN } from "react-day-picker/locale";
import { CalendarIcon, XIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Popover, PopoverTrigger, PopoverContent } from "@/components/ui/popover";

import "react-day-picker/style.css";

function formatDate(date: Date): string {
  const y = date.getFullYear();
  const m = date.getMonth() + 1;
  const d = date.getDate();
  return `${y}-${String(m).padStart(2, "0")}-${String(d).padStart(2, "0")}`;
}

function formatDisplay(date: Date): string {
  return `${date.getMonth() + 1}月${date.getDate()}日`;
}

function parseDate(str: string): Date | undefined {
  if (!str) return undefined;
  const match = str.match(/^(\d{4})-(\d{2})-(\d{2})$/);
  if (!match) return undefined;
  return new Date(parseInt(match[1]), parseInt(match[2]) - 1, parseInt(match[3]));
}

export function DateRangePicker({
  dateFrom,
  dateTo,
  onChange,
}: {
  dateFrom: string;
  dateTo: string;
  onChange: (from: string | undefined, to: string | undefined) => void;
}) {
  const [open, setOpen] = useState(false);

  const selected: DateRange | undefined =
    dateFrom || dateTo
      ? { from: parseDate(dateFrom), to: parseDate(dateTo) }
      : undefined;

  const handleSelect = (range: DateRange | undefined) => {
    onChange(
      range?.from ? formatDate(range.from) : undefined,
      range?.to ? formatDate(range.to) : undefined
    );
  };

  const hasValue = dateFrom || dateTo;

  const displayText = hasValue
    ? `${dateFrom ? formatDisplay(parseDate(dateFrom)!) : "?"} ~ ${dateTo ? formatDisplay(parseDate(dateTo)!) : "?"}`
    : "日期筛选";

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger
        render={
          <Button
            variant="outline"
            size="sm"
            className={hasValue ? "" : "text-muted-foreground"}
          />
        }
      >
        <CalendarIcon className="h-4 w-4 mr-1" />
        {displayText}
        {hasValue && (
          <span
            className="ml-1 hover:text-red-500"
            onClick={(e) => {
              e.stopPropagation();
              onChange(undefined, undefined);
            }}
          >
            <XIcon className="h-3 w-3" />
          </span>
        )}
      </PopoverTrigger>
      <PopoverContent className="w-auto p-2">
        <DayPicker
          mode="range"
          locale={zhCN}
          selected={selected}
          onSelect={handleSelect}
          numberOfMonths={2}
          classNames={{
            root: "text-sm",
            month_caption: "flex justify-center py-1 font-medium",
            nav: "flex items-center justify-between",
            button_previous:
              "h-7 w-7 inline-flex items-center justify-center rounded-md hover:bg-accent",
            button_next:
              "h-7 w-7 inline-flex items-center justify-center rounded-md hover:bg-accent",
            weekday: "text-muted-foreground text-xs font-normal w-8 text-center",
            day: "h-8 w-8 text-center text-sm [&.rdp-selected>.rdp-day_button]:bg-primary [&.rdp-selected>.rdp-day_button]:text-primary-foreground [&.rdp-selected>.rdp-day_button]:hover:bg-primary",
            day_button:
              "h-8 w-8 inline-flex items-center justify-center rounded-md hover:bg-accent transition-colors",
            range_start: "rounded-l-md",
            range_end: "rounded-r-md",
            range_middle: "[&>.rdp-day_button]:bg-accent [&>.rdp-day_button]:text-accent-foreground",
            today: "font-bold",
            outside: "text-muted-foreground opacity-50",
          }}
        />
      </PopoverContent>
    </Popover>
  );
}
