import { cva } from "class-variance-authority"

export const badgeVariants = cva(
  "inline-flex items-center justify-center gap-1 rounded-full border border-transparent bg-muted px-2 py-0.5 font-mono text-[10px] leading-normal tracking-[0.06em] whitespace-nowrap uppercase text-muted-foreground w-fit shrink-0 [&>svg]:size-3 [&>svg]:pointer-events-none focus-visible:border-ring focus-visible:ring-ring/50 focus-visible:ring-[3px] aria-invalid:ring-destructive/20 dark:aria-invalid:ring-destructive/40 aria-invalid:border-destructive transition-[color,box-shadow] overflow-hidden",
  {
    variants: {
      variant: {
        default: "",
        outline: "border-border bg-transparent",
        signal: "border-signal/35 bg-signal-soft text-signal",
        success: "border-success/35 bg-success/10 text-success",
        warn: "border-warn/35 bg-warn/10 text-warn",
        info: "border-info/35 bg-info/10 text-info",
        destructive: "border-destructive/35 bg-destructive/10 text-destructive",
      },
    },
    defaultVariants: {
      variant: "default",
    },
  }
)
