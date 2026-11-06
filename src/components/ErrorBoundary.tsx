import { Component, type ReactNode } from "react";
import { AlertTriangle } from "lucide-react";
import { Button } from "./ui/button";

/**
 * The last line of defence at the app root: a render error anywhere
 * below lands here instead of unmounting the whole tree into a
 * white window. The recovery screen says what happened in human
 * terms and offers the one action that reliably fixes a broken
 * render state — reload. Nothing has been deleted or changed by a
 * render failure; scans and history live in the shell.
 */
interface ErrorBoundaryState {
  error: Error | null;
}

export class ErrorBoundary extends Component<{ children: ReactNode }, ErrorBoundaryState> {
  state: ErrorBoundaryState = { error: null };

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { error };
  }

  componentDidCatch(error: Error): void {
    // Keep the detail available in the console for diagnostics;
    // the screen itself stays in plain language.
    console.error("DiskPilot render error:", error);
  }

  render(): ReactNode {
    const { error } = this.state;
    if (!error) return this.props.children;
    return (
      <main className="flex h-full flex-col items-center justify-center bg-background px-6 text-center text-foreground">
        <img src="/logo.svg" alt="" width={52} height={52} className="rounded-[12px]" aria-hidden="true" />
        <AlertTriangle className="mt-6 size-8 text-critical" strokeWidth={1.5} aria-hidden="true" />
        <h1 className="mt-3 text-[20px] font-semibold leading-6">Something went wrong on this screen</h1>
        <p className="mt-2 max-w-[440px] text-[13px] leading-[18px] text-muted">
          DiskPilot hit an unexpected problem while drawing this page. Your scans, settings, and
          history are stored by the app itself and are not affected — reloading rebuilds the
          screen from that saved state.
        </p>
        <p className="mono-path mt-3 max-w-[440px] text-subtle">{error.message}</p>
        <Button
          variant="primary"
          className="mt-5"
          onClick={() => window.location.reload()}
        >
          Reload DiskPilot
        </Button>
      </main>
    );
  }
}
