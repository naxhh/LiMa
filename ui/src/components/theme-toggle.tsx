import { Moon, Sun } from "lucide-react";
import { useTheme } from "next-themes";

import { Button } from "@/components/ui/button";

/** Flips between dark and light (the initial default follows the system). */
export function ThemeToggle() {
  const { resolvedTheme, setTheme } = useTheme();
  const dark = resolvedTheme !== "light";
  return (
    <Button
      variant="outline"
      size="icon"
      aria-label={dark ? "Switch to light theme" : "Switch to dark theme"}
      title={dark ? "Light theme" : "Dark theme"}
      onClick={() => setTheme(dark ? "light" : "dark")}
      className="text-muted-foreground"
    >
      {dark ? <Moon /> : <Sun />}
    </Button>
  );
}
