import * as React from "react";
import { cn } from "../../lib/utils";

const InlineCode = React.forwardRef(({ className, ...props }, ref) => (
  <code
    ref={ref}
    className={cn(
      "rounded bg-muted px-2 py-1 font-mono text-xs break-all",
      className
    )}
    {...props}
  />
));
InlineCode.displayName = "InlineCode";

const CodeBlock = React.forwardRef(({ className, ...props }, ref) => (
  <pre
    ref={ref}
    className={cn(
      "rounded-lg border bg-muted/30 p-3 font-mono text-[0.72rem] leading-relaxed whitespace-pre-wrap break-words overflow-auto",
      className
    )}
    {...props}
  />
));
CodeBlock.displayName = "CodeBlock";

export { InlineCode, CodeBlock };
