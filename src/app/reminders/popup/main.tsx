import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "@fontsource-variable/ibm-plex-sans/wght.css";
import { ReminderPopup } from "./ReminderPopup";
import "./popup.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <ReminderPopup />
  </StrictMode>,
);
