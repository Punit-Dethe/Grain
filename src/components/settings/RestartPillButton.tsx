import React, { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "../ui/Button";
import { SettingContainer } from "../ui/SettingContainer";
import { RotateCw } from "lucide-react";

interface RestartPillButtonProps {
  grouped?: boolean;
}

export const RestartPillButton: React.FC<RestartPillButtonProps> = ({
  grouped = false,
}) => {
  const [isRestarting, setIsRestarting] = useState(false);

  const handleRestart = async () => {
    setIsRestarting(true);
    try {
      await invoke("restart_pill");
    } catch (e) {
      console.error("Failed to restart pill:", e);
    } finally {
      setTimeout(() => setIsRestarting(false), 1000);
    }
  };

  return (
    <SettingContainer
      label="Restart Overlay Pill"
      description="Force restart the dot-matrix overlay pill if it becomes unresponsive or disappears."
      grouped={grouped}
    >
      <Button
        variant="secondary"
        onClick={handleRestart}
        disabled={isRestarting}
        className="min-w-[120px] flex justify-center items-center gap-2"
      >
        <RotateCw
          className={`w-4 h-4 ${isRestarting ? "animate-spin" : ""}`}
        />
        {isRestarting ? "Restarting..." : "Restart"}
      </Button>
    </SettingContainer>
  );
};
