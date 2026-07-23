import * as HoverCardPrimitive from '@radix-ui/react-hover-card';

export default function CopyableHoverText({ children, value, triggerClassName = 'block max-w-full' }) {
  return (
    <HoverCardPrimitive.Root openDelay={120} closeDelay={180}>
      <HoverCardPrimitive.Trigger asChild>
        <span className={triggerClassName}>
          {children}
        </span>
      </HoverCardPrimitive.Trigger>
      <HoverCardPrimitive.Portal>
        <HoverCardPrimitive.Content
          side="top"
          align="start"
          sideOffset={8}
          className="z-50 max-w-[min(36rem,calc(100vw-2rem))] rounded-md border bg-popover p-3 text-popover-foreground shadow-xl shadow-black/10 outline-none"
        >
          <div className="select-text break-all font-mono text-sm leading-6">
            {value}
          </div>
          <HoverCardPrimitive.Arrow className="fill-popover" />
        </HoverCardPrimitive.Content>
      </HoverCardPrimitive.Portal>
    </HoverCardPrimitive.Root>
  );
}
