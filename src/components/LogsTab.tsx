import { Terminal } from "lucide-react";
import { LogView } from "./LogView";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "./Card";
import type { LogLine } from "@/lib/types";

export function LogsTab({
  logs,
  onClear,
}: {
  logs: LogLine[];
  onClear: () => void;
}) {
  return (
    <div className="mx-auto flex w-full max-w-5xl flex-col gap-4 p-6">
      <Card>
        <CardHeader>
          <div className="space-y-1">
            <CardTitle className="flex items-center gap-2">
              <Terminal className="h-4 w-4 text-muted-foreground" />
              Live logs
            </CardTitle>
            <CardDescription>
              stdout / stderr of the sing-box process. Errors are
              highlighted in red. Rolling buffer capped at 500 lines.
            </CardDescription>
          </div>
        </CardHeader>
        <CardContent className="p-3 pt-0">
          <LogView logs={logs} className="h-[60vh]" onClear={onClear} />
        </CardContent>
      </Card>
    </div>
  );
}
