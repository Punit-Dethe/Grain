import { useTranslation } from "react-i18next";
import { ArrowUp, MessageSquare, Plus, Sparkles } from "lucide-react";

/**
 * [GRAIN] Chat rail — PURE SCAFFOLD by explicit instruction (TAURI-OVERLAY-PLAN
 * Phase C): a placeholder for a future Recall-in-panel, styled after Mem's chat
 * pane (Heads-Up / Chat tabs, welcome + suggestion cards, an input with a send
 * affordance). No commands, no state, no wiring — everything here is inert. The
 * clip shell animates width 0 ↔ 312px so it slides; the inner pane is
 * fixed-width so its content never reflows mid-slide.
 */
export function ChatRail({ open }: { open: boolean }) {
  const { t } = useTranslation();
  return (
    <div
      className={`gs-chat-clip${open ? " gs-chat-clip--open" : ""}`}
      aria-hidden={!open}
    >
      <div className="gs-chat">
        <div className="gs-chat-tabs">
          <span className="gs-chat-tab">
            <Sparkles width={13} height={13} />
            {t("grainSpaceOverlay.headsUp")}
          </span>
          <span className="gs-chat-tab gs-chat-tab--on">
            <MessageSquare width={13} height={13} />
            {t("grainSpaceOverlay.chat")}
          </span>
        </div>

        <div className="gs-chat-body">
          <div className="gs-chat-welcome">
            {t("grainSpaceOverlay.chatWelcome")}
          </div>
          <div className="gs-chat-card">
            <div className="gs-chat-card-title">
              {t("grainSpaceOverlay.chatCard1Title")}
            </div>
            <div className="gs-chat-card-sub">
              {t("grainSpaceOverlay.chatCard1Sub")}
            </div>
          </div>
          <div className="gs-chat-card">
            <div className="gs-chat-card-title">
              {t("grainSpaceOverlay.chatCard2Title")}
            </div>
            <div className="gs-chat-card-sub">
              {t("grainSpaceOverlay.chatCard2Sub")}
            </div>
          </div>
        </div>

        <div className="gs-chat-foot">
          <div className="gs-chat-input">
            <Plus width={14} height={14} />
            <input
              disabled
              placeholder={t("grainSpaceOverlay.chatPlaceholder")}
            />
            <span className="gs-chat-send">
              <ArrowUp width={13} height={13} />
            </span>
          </div>
        </div>
      </div>
    </div>
  );
}
