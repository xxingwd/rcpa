import * as React from "react";
import { cn } from "../../lib/utils";

const Switch = React.forwardRef(({ className, checked, onCheckedChange, disabled, ...props }, ref) => (
  <button
    type="button"
    role="switch"
    aria-checked={checked}
    disabled={disabled}
    ref={ref}
    onClick={() => onCheckedChange?.(!checked)}
    className={cn(
      "relative inline-flex h-6 w-11 shrink-0 items-center rounded-full border transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30 disabled:cursor-not-allowed disabled:opacity-50",
      checked ? "border-emerald-500 bg-emerald-500" : "border-input bg-muted",
      className
    )}
    {...props}
  >
    <span
      className={cn(
        "pointer-events-none block h-5 w-5 rounded-full bg-white shadow-sm transition-transform",
        checked ? "translate-x-5" : "translate-x-0.5"
      )}
    />
  </button>
));
Switch.displayName = "Switch";

export { Switch };
