import type { ExtensionCard } from "@/bindings";
import { capabilityLabel, type ApprovalRequest } from "./extensionRuntime";

const PERMISSION_COPY = "This extension is asking to:";
const AUTH_DOMAINS_LABEL = "Sign-in domains";
const ACTIONS_COPY = "It provides these tools:";
const ACTION_CONFIRM_COPY = "asks you first";

export function ExtensionApprovalDetails({
  approval,
}: {
  card: ExtensionCard;
  approval: ApprovalRequest;
}) {
  const authentication = approval.authentication;
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
