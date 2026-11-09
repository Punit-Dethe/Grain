import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "@/i18n";
import { commands } from "@/bindings";
import { AgentPanel } from "./AgentPanel";

async function mount() {
  const root = document.getElementById("root")!;
  const rect = root.querySelector(".agent-notch")!.getBoundingClientRect();
  const scale = window.devicePixelRatio;
  await commands
    .agentShowNotch([
      Math.ceil(rect.width * scale),
      Math.ceil(rect.height * scale),
    ])
    .catch(console.error);

  createRoot(root).render(
    <StrictMode>
      <AgentPanel />
    </StrictMode>,
  );
}

void mount();
