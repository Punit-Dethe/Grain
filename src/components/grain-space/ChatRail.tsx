import { useTranslation } from "react-i18next";
import { Sparkles } from "lucide-react";

/**
 * [GRAIN] Chat rail — PURE SCAFFOLD by explicit instruction (TAURI-OVERLAY-PLAN
 * Phase C): a placeholder for a future Recall-in-panel. Skeleton bubbles and a
 * disabled input; no commands, no state, no wiring. The clip shell animates
 * width 0 ↔ 300px so it slides; the inner pane is fixed-width so its content
 * never reflows mid-slide.
 */
export function ChatRail({ open }: { open: boolean }) {
  const { t } = useTranslation();
  return (
    <div
      className={`gs-chat-clip${open ? " gs-chat-clip--open" : ""}`}
      aria-hidden={!open}
    >
      <div className="gs-chat">
        <div className="gs-chat-head">
          <Sparkles width={13} height={13} />
          <span className="gs-chat-title">{t("grainSpaceOverlay.chat")}</span>
          <span className="gs-chat-tag">{t("grainSpaceOverlay.chatSoon")}</span>
        </div>
        <div className="gs-chat-body">
          <div className="gs-skel gs-skel--me" />
          <div className="gs-skel gs-skel--ai" />
          <div className="gs-skel gs-skel--me" />
          <div className="gs-skel gs-skel--ai" />
        </div>
        <div className="gs-chat-foot">
          <div className="gs-chat-input">
            <input
              disabled
              placeholder={t("grainSpaceOverlay.chatPlaceholder")}
            />
          </div>
        </div>
      </div>
    </div>
  );
}
