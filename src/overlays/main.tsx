/**
 * Overlay window entry point.
 *
 * Shares the store, IPC bridge and design tokens with the main window; only the
 * shell differs, because the overlay must be transparent and sized to content.
 */
import React from "react";
import ReactDOM from "react-dom/client";
import { Overlay } from "@/overlays/Overlay";
import { APP_NAME } from "@/identity";
import "@/styles/globals.css";

const container = document.getElementById("root");
if (!container) {
  throw new Error(`${APP_NAME} overlay could not find its root element.`);
}

// The overlay window is transparent; the panel provides the surface.
document.documentElement.style.background = "transparent";
document.body.style.background = "transparent";

ReactDOM.createRoot(container).render(
  <React.StrictMode>
    <Overlay />
  </React.StrictMode>
);
