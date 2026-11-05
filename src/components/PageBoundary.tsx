import { Component, type ReactNode } from "react";
import { AlertTriangle, RotateCcw } from "lucide-react";
import { Button, Card } from "./ui";

/**
 * Keeps one page's error from blanking the whole window: the page shows what went wrong
 * and a way to try again, while the tab bar and the other pages keep working. Keyed by
 * page, so moving to another page starts fresh.
 */
export default class PageBoundary extends Component<{ children: ReactNode }, { error: Error | null }> {
  state = { error: null as Error | null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidCatch(error: Error) {
    console.error(error);
  }

  render() {
    if (!this.state.error) return this.props.children;
    return (
      <Card className="mx-auto mt-10 max-w-xl p-6">
        <div className="flex items-start gap-3">
          <AlertTriangle size={20} className="mt-0.5 shrink-0 text-danger-text" aria-hidden />
          <div className="min-w-0 flex-1">
            <h2 className="text-lg font-bold">This page ran into a problem</h2>
            <p className="mt-1 text-sm text-muted">The rest of LampRabit keeps working. Try again, or open another page.</p>
            <pre className="mt-3 max-h-40 overflow-auto rounded-lg bg-surface-2 p-3 font-mono text-xs whitespace-pre-wrap text-fg">{this.state.error.message}</pre>
            <Button className="mt-4" variant="primary" onClick={() => this.setState({ error: null })}>
              <RotateCcw size={15} /> Try again
            </Button>
          </div>
        </div>
      </Card>
    );
  }
}
