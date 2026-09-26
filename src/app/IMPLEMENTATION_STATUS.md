# Grain UI 2.0 implementation status

| Surface    | Control/capability                                     | Visual state      | Functional state    | Reason / next owner                                       |
| ---------- | ------------------------------------------------------ | ----------------- | ------------------- | --------------------------------------------------------- |
| Shell      | Window chrome, theme, navigation                       | Complete          | Working             | Uses real Tauri window and theme APIs.                    |
| Overview   | Recent transcriptions                                  | Complete          | Working             | Shares one live History controller with History.          |
| Overview   | Studio shortcut                                        | Complete          | Working             | Opens the Dictionary section in Studio.                   |
| Overview   | Standard capture                                       | Visible, disabled | Intentionally inert | Shortcut exists, no renderer-callable capture command.    |
| Overview   | Flow capture                                           | Visible, disabled | Intentionally inert | Shortcut exists, no renderer-callable capture command.    |
| Overview   | Quick Agent                                            | Visible, disabled | Intentionally inert | Agent summon is not renderer-callable.                    |
| Overview   | Edit actions                                           | Visible, disabled | Intentionally inert | No customization API.                                     |
| Overview   | Design panel                                           | Visible, disabled | Intentionally inert | Prototype-only design tool.                               |
| Shell      | Quick panel                                            | Visible, disabled | Deferred            | Deferred to Overview/cutover consolidation.               |
| History    | Archive, actions, audio, pagination                    | Complete          | Working             | Uses the shared live History controller.                  |
| History    | All / Today / AI processed / Unprocessed filters       | Complete          | Working             | Filters on produced processed text, the one axis stored.  |
| Settings   | Surfaced preference sections                           | In progress       | Working             | Existing surfaced settings use the prototype style.       |
| Settings   | Appearance (system / light / dark)                     | Complete          | Working             | Three-mode row; the legacy two-state toggle is next-only. |
| About      | Own tab: language, version, locations, acknowledgments | Complete          | Working             | Re-skins the real About surface; parity gap closed.       |
| Tools      | Dictionary and automatic dictionary                    | Complete          | Working             | Real validation, persistence, and update states retained. |
| Tools      | Snippets, Context, and Agent                           | Complete          | Working             | Real feature switches, editors, controls, and anchors.    |
| Tools      | Contextual extension recommendations                   | Complete          | Working             | Uses the live bounded store catalogue and install flow.   |
| Extensions | Installed collection, routing, enable and uninstall    | Complete          | Working             | Permission approval and slot takeover semantics retained. |
| Extensions | Store search, filters, install/update and status       | Complete          | Working             | Uses the verified catalogue and honest offline state.     |
| Extensions | Preview drawer, media carousel, README, permissions    | Complete          | Working             | Media and README load lazily and are dropped on close.    |
| Extensions | Import pack and developer tools                        | Complete          | Working             | Uses native import and existing developer-mode tooling.   |
