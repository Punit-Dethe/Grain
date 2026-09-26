import type { ExtensionCard } from "@/bindings";
import {
  capabilityLabel,
  promptLayerScope,
  type ApprovalRequest,
} from "./extensionRuntime";

const PERMISSION_COPY =
  "This extension runs code on your device and is asking to:";
const PROMPT_COPY =
  "It supplies these instructions when you dictate. Replacement instructions take the named prompt position; Grain's Prompt Record stays private and takes priority.";
const RECOMMENDATION_COPY =
  "When you pick this extension in Extension Mode, Grain hands it everything you said in that request, word for word.";
const SEMANTIC_COPY =
  "This extension also needs a one-time download of an approximately 130 MB understanding model.";
const AUTH_DOMAINS_LABEL = "Sign-in domains";
const ACTIONS_COPY = "It can do these things when you ask out loud:";
const ACTION_CONFIRM_COPY = "asks you first";

export function ExtensionApprovalDetails({
  card,
  approval,
}: {
  card: ExtensionCard;
  approval: ApprovalRequest;
}) {
  const authentication = approval.authentication;
  const recommendationPurpose = card.recommend?.purpose
    ? `It asks to be offered for: ${card.recommend.purpose}`
    : null;
  const authenticationCopy = authentication
    ? `Connect ${authentication.provider_name} for this extension. Grain will request ${authentication.scopes.join(", ") || "no extra scopes"}. Sign-in happens in your browser when you connect the account. The extension can use the connection only with ${authentication.api_hosts.join(", ")}; it cannot read the token.`
    : "";
  return (
    <>
      {approval.permissions.length > 0 && (
        <>
          <p>{PERMISSION_COPY}</p>
          <ul>
            {approval.permissions.map((permission) => (
              <li key={permission}>{capabilityLabel(permission)}</li>
            ))}
          </ul>
        </>
      )}
      {approval.promptLayers.length > 0 && (
        <>
          <p>{PROMPT_COPY}</p>
          <ul className="extension-prompt-layers">
            {approval.promptLayers.map((layer) => (
              <li key={layer.id}>
                <span className="extension-prompt-layer-scope">
                  {promptLayerScope(layer)}
                </span>
                <q>{layer.text}</q>
              </li>
            ))}
          </ul>
        </>
      )}
      {(approval.recommendation || card.kind === "searchable") && (
        <div className="extension-confirm-disclosure">
          <p>{RECOMMENDATION_COPY}</p>
          {recommendationPurpose && <p>{recommendationPurpose}</p>}
        </div>
      )}
      {card.needs.includes("semantic") && (
        <div className="extension-confirm-disclosure">
          <p>{SEMANTIC_COPY}</p>
        </div>
      )}
      {authentication && (
        <div className="extension-confirm-disclosure">
          <p>{authenticationCopy}</p>
          <details>
            <summary>{AUTH_DOMAINS_LABEL}</summary>
            <p>
              {`Authorization: ${authentication.authorization_host}`}
              <br />
              {`Token: ${authentication.token_host}`}
            </p>
          </details>
        </div>
      )}
      {approval.actions.length > 0 && (
        <>
          <p>{ACTIONS_COPY}</p>
          <ul className="extension-actions">
            {approval.actions.map((action) => (
              <li key={action.id}>
                <span className="extension-action-titles">{action.title}</span>
                {action.confirms && (
                  <span className="extension-action-confirms">
                    {ACTION_CONFIRM_COPY}
                  </span>
                )}
              </li>
            ))}
          </ul>
        </>
      )}
    </>
  );
}
