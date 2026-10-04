import React from "react";
import { createRoot } from "react-dom/client";
import "@/i18n";
import { AgentInput } from "./AgentInput";
import "@/overlay/overlay.css";
import "./agent-input.css";

createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <AgentInput />
  </React.StrictMode>,
);
