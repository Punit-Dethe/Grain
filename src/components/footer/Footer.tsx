import React, { useState, useEffect } from "react";
import { getVersion } from "@tauri-apps/api/app";

import ModelSelector from "../model-selector";
import UpdateChecker from "../update-checker";

const Footer: React.FC = () => {
  const [version, setVersion] = useState("");

  useEffect(() => {
    const fetchVersion = async () => {
      try {
        const appVersion = await getVersion();
        setVersion(appVersion);
      } catch (error) {
        console.error("Failed to get app version:", error);
        setVersion("0.1.2");
      }
    };

    fetchVersion();
  }, []);

  return (
    <div className="w-full border-t border-line bg-paper pt-2.5">
      <div className="flex justify-between items-center text-xs px-4 pb-2.5 text-ink-soft">
        <div className="flex items-center gap-4">
          <ModelSelector />
        </div>

        {/* Update status — mono, quiet. A small solid accent tick bookends the
            wordmark's mark so the top and bottom of the window rhyme. */}
        <div className="flex items-center gap-2 font-mono text-[0.65rem] text-ink-faint">
          <span className="w-1.5 h-1.5 rounded-[1px] bg-accent/70" aria-hidden />
          <UpdateChecker />
          <span className="text-ink-faint/60">/</span>
          {/* eslint-disable-next-line i18next/no-literal-string */}
          <span>v{version}</span>
        </div>
      </div>
    </div>
  );
};

export default Footer;
