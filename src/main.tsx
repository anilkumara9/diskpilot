import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { applyTheme } from "./lib/theme";
import "./styles/globals.css";

// Apply the OS theme before first paint to avoid a flash; the store
// refines this once saved settings load.
applyTheme("system");

const rootElement = document.getElementById("root");
if (!rootElement) throw new Error("Root element #root was not found.");

createRoot(rootElement).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
