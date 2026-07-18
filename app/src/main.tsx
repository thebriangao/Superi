import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { App } from "./App";
import { SuperiApiProvider } from "./api-context";
import { DesktopSuperiTransport } from "./transport";
import "./styles.css";

const root = document.querySelector("#app");
if (!(root instanceof HTMLElement)) {
  throw new Error("Superi application root is missing");
}

const transport = new DesktopSuperiTransport();
window.addEventListener("beforeunload", () => void transport.dispose(), {
  once: true,
});

createRoot(root).render(
  <StrictMode>
    <SuperiApiProvider transport={transport}>
      <App />
    </SuperiApiProvider>
  </StrictMode>,
);
