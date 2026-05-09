import { Moon, Sun, Search, Bell, Github } from "lucide-react";
import { Button } from "./ui/button";
import { Input } from "./ui/input";
import { useTheme } from "./theme-provider";
import { Avatar, AvatarFallback } from "./ui/avatar";

export function Topbar({ value, onChange }: { value: string; onChange: (q: string) => void }) {
  const { theme, toggle } = useTheme();
  return (
    <header className="h-16 border-b border-border bg-background/80 backdrop-blur flex items-center px-6 gap-4 sticky top-0 z-10">
      <div className="relative flex-1 max-w-xl">
        <Search className="absolute left-3 top-1/2 -translate-y-1/2 size-4 text-muted-foreground" />
        <Input
          value={value}
          placeholder="法令名・法令番号・条文を検索..."
          className="pl-9 h-9"
          onChange={e => onChange(e.target.value)}
        />
        <kbd className="absolute right-3 top-1/2 -translate-y-1/2 text-xs text-muted-foreground border border-border rounded px-1.5 py-0.5 bg-muted">⌘K</kbd>
      </div>
      <div className="flex items-center gap-1 ml-auto">
        <Button variant="ghost" size="icon" className="size-9" asChild>
          <a href="https://github.com/bokuweb/lawrenceanum" target="_blank" rel="noreferrer">
            <Github className="size-4" />
          </a>
        </Button>
        <Button variant="ghost" size="icon" className="size-9 relative">
          <Bell className="size-4" />
          <span className="absolute top-2 right-2 size-2 rounded-full bg-primary" />
        </Button>
        <Button variant="ghost" size="icon" className="size-9" onClick={toggle}>
          {theme === "light" ? <Moon className="size-4" /> : <Sun className="size-4" />}
        </Button>
        <Avatar className="size-8 ml-2">
          <AvatarFallback className="text-xs">JP</AvatarFallback>
        </Avatar>
      </div>
    </header>
  );
}
