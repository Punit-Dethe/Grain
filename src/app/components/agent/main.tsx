import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "@/i18n";
import { AgentPanel } from "./AgentPanel";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <AgentPanel />
  </StrictMode>,
);
