import { useTheme } from "next-themes";
import { Toaster as Sonner } from "sonner";

/** App-wide toasts (failed background changes), styled with the theme tokens. */
export function Toaster() {
  const { resolvedTheme } = useTheme();
  return (
    <Sonner
      theme={resolvedTheme === "light" ? "light" : "dark"}
      position="bottom-right"
      // Clear the phone tab bar.
      mobileOffset={{ bottom: 84 }}
      toastOptions={{
        classNames: {
          toast: "!bg-card !text-card-foreground !border !border-line-strong !rounded-xl !font-sans",
          description: "!text-muted-foreground",
          error: "[&_[data-icon]]:!text-destructive",
        },
      }}
    />
  );
}
