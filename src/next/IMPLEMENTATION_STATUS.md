# Grain UI 2.0 implementation status

| Surface    | Control/capability                                      | Visual state      | Functional state    | Reason / next owner                                       |
| ---------- | ------------------------------------------------------- | ----------------- | ------------------- | --------------------------------------------------------- |
| Shell      | Window chrome, theme, navigation                        | Complete          | Working             | Uses real Tauri window and theme APIs.                    |
| Overview   | Open Notes                                              | Complete          | Working             | Routes to the real Notes workspace.                       |
| Overview   | Recent transcriptions                                   | Complete          | Working             | Shares one live History controller with History.          |
| Overview   | Start Flow                                              | Visible, disabled | Intentionally inert | No renderer-callable capture command; backend/API owner.  |
| Overview   | Standard capture                                        | Visible, disabled | Intentionally inert | Shortcut exists, no renderer-callable capture command.    |
| Overview   | Flow capture                                            | Visible, disabled | Intentionally inert | Shortcut exists, no renderer-callable capture command.    |
| Overview   | Quick note                                              | Visible, disabled | Intentionally inert | Notes CRUD is not the capture workflow.                   |
| Overview   | Quick Agent                                             | Visible, disabled | Intentionally inert | Agent summon is not renderer-callable.                    |
| Overview   | Edit actions                                            | Visible, disabled | Intentionally inert | No customization API.                                     |
| Overview   | Design panel                                            | Visible, disabled | Intentionally inert | Prototype-only design tool.                               |
| Shell      | Quick panel                                             | Visible, disabled | Deferred            | Deferred to Overview/cutover consolidation.               |
| Notes      | Workspace, search, folders, editor, reminders, calendar | Complete          | Working             | Re-hosts the existing Grain Space engine.                 |
| Notes      | Collections-first sidebar and incremental recent notes  | Complete          | Working             | Starts at five notes; View more reveals five at a time.   |
| Notes      | Ask Grain and source navigation                         | Complete          | Working             | Existing ChatRail retained with UI 2.0 styling.           |
| Notes      | Feature-off on-ramp and Notes settings                  | Complete          | Working             | Preserves zero-cost-off behavior and settings/MCP bridge. |
| History    | Archive, actions, audio, pagination                     | Complete          | Working             | Uses the shared live History controller.                  |
| History    | Flow / Standard filter pills                            | Complete          | Cosmetic only       | History entries do not expose capture-mode metadata yet.  |
| Settings   | Surfaced preference sections                            | In progress       | Working             | Existing surfaced settings use the prototype style.       |
| Settings   | Appearance (system / light / dark)                      | Complete          | Working             | Three-mode row; the legacy two-state toggle is next-only. |
| About      | Own tab: language, version, locations, acknowledgments  | Complete          | Working             | Re-skins the real About surface; parity gap closed.       |
| Tools      | Dictionary and automatic dictionary                     | Complete          | Working             | Real validation, persistence, and update states retained. |
| Tools      | Snippets, Context, and Agent                            | Complete          | Working             | Real feature switches, editors, controls, and anchors.    |
| Tools      | Contextual extension recommendations                    | Complete          | Working             | Uses the live bounded store catalogue and install flow.   |
| Extensions | Installed collection, routing, enable and uninstall     | Complete          | Working             | Permission approval and slot takeover semantics retained. |
| Extensions | Store search, filters, install/update and status        | Complete          | Working             | Uses the verified catalogue and honest offline state.     |
| Extensions | Preview drawer, media carousel, README, permissions     | Complete          | Working             | Media and README load lazily and are dropped on close.    |
| Extensions | Import pack and developer tools                         | Complete          | Working             | Uses native import and existing developer-mode tooling.   |
