import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { App } from "./App";
import { SuperiApiProvider } from "./api-context";
import "./styles.css";

const root = document.querySelector("#app");
if (!(root instanceof HTMLElement)) {
  throw new Error("Superi application root is missing");
}

createRoot(root).render(
  <StrictMode>
    <SuperiApiProvider transport={null}>
      <App />
    </SuperiApiProvider>
  </StrictMode>,
);
