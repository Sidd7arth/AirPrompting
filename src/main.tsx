import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "@/App";
import { APP_NAME } from "@/identity";
import "@/styles/globals.css";

const container = document.getElementById("root");
if (!container) {
  throw new Error(`${APP_NAME} could not find its root element.`);
}

ReactDOM.createRoot(container).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
