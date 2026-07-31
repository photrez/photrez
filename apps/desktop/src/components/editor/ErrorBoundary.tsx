import {
  createSignal,
  ErrorBoundary as SolidErrorBoundary,
  JSX,
  onCleanup,
  onMount,
  Show,
} from "solid-js";

interface ErrorBoundaryProps {
  /** Rendered when an error is caught. `reset` clears the error and re-renders children. */
  fallback: (error: Error, reset: () => void) => JSX.Element;
  children: JSX.Element;
}

/**
 * Renders a fallback UI when the child tree throws, while the rest of the
 * app (tabs, toolbar, panels) keeps functioning.
 *
 * Two error sources are handled:
 * - Render errors: Solid's native `ErrorBoundary` catches errors thrown while
 *   evaluating the initial child tree.
 * - Async/event-handler errors: a window-level `error` listener stores the
 *   error in a signal and a `Show` gate swaps the children for the fallback.
 *   (Solid's native boundary does not catch errors from reactive updates, so
 *   the async path is rendered declaratively instead of rethrowing.)
 *
 * The window listener also cleans up errors already handled by the native
 * boundary: `preventDefault()` keeps errors that reach the boundary from
 * being re-reported.
 *
 * Errors thrown from the fallback are caught by a parent ErrorBoundary.
 */
export function ErrorBoundary(props: ErrorBoundaryProps) {
  const [asyncError, setAsyncError] = createSignal<Error | null>(null);

  onMount(() => {
    const handleError = (e: ErrorEvent) => {
      // Only capture errors that escaped the tree (async/event-handler).
      // Render errors are already handled by the native boundary, and errors
      // caught by it do not reach the window error event.
      e.preventDefault();
      setAsyncError(e.error instanceof Error ? e.error : new Error(e.message));
    };
    window.addEventListener("error", handleError);
    onCleanup(() => window.removeEventListener("error", handleError));
  });

  const fallback = (err: unknown, reset: () => void) =>
    props.fallback(err instanceof Error ? err : new Error(String(err)), () => {
      setAsyncError(null);
      reset();
    });

  return (
    <SolidErrorBoundary fallback={(err, reset) => fallback(err, reset)}>
      <Show when={asyncError()} fallback={props.children}>
        {(err) => fallback(err, () => setAsyncError(null))}
      </Show>
    </SolidErrorBoundary>
  );
}
