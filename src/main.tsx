import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import QuickSearchApp from "./pages/QuickSearch";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { applyTheme } from "./lib/theme";
import "./styles/globals.css";

// Apply the OS theme before first paint to avoid a flash; the store
// refines this once saved settings load.
applyTheme("system");

// The Quick Search popup window loads this same bundle with
// `?window=quicksearch`; it renders its own surface instead of the
// main app (no sidebar, no store init).
const isQuickSearch =
  typeof window !== "undefined" &&
  new URLSearchParams(window.location.search).get("window") === "quicksearch";

const rootElement = document.getElementById("root");
if (!rootElement) throw new Error("Root element #root was not found.");

createRoot(rootElement).render(
  <StrictMode>
    <ErrorBoundary>{isQuickSearch ? <QuickSearchApp /> : <App />}</ErrorBoundary>
  </StrictMode>,
);
