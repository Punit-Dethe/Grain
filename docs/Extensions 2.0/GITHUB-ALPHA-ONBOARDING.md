# GitHub MCP: tester-only alpha onboarding

## Scope and identity

The extension build is an experimental alpha for testers in direct contact with
the maintainer. The existing beta without extensions remains a separate delivery.
The current account, Grain name and repositories are temporary. Testers may need
to reinstall extensions or authorize a replacement registration later; do not
build registration, catalogue or settings migration machinery for them.

Reuse the published `Punit-Dethe/grain-github-extension` source and draft registry
submission. A new repository is not needed for account authentication. Publication,
OAuth registration and extension signing are separate responsibilities.

## Dedicated GitHub registration

Register a GitHub App under the existing maintainer account, using a unique name
such as `Grain Extensions Alpha` and this homepage:
`https://github.com/Punit-Dethe/grain-github-extension`.

Open [GitHub App registration](https://github.com/settings/apps/new) in your
usual browser and sign in to GitHub as `Punit-Dethe`. Being signed in to Google
does not sign you in to GitHub. Registration can be completed there without
sharing passwords or verification codes with an agent.

- Callback: `http://127.0.0.1:31938/mcp/oauth/callback`.
- Leave user token expiration enabled. Grain reuses the SDK refresh flow.
- Disable webhooks; no webhook service, app private key or installation-token
  minting service is needed for this user-authorization connection.
- Initial permissions: Contents read-only, Issues read/write, automatic Metadata
  read-only. Other tool permissions require their own justified acceptance.
- Start installation on one disposable repository. For other testers to install
  it, the app must permit installation on **Any account**; each tester chooses
  repositories and separately authorizes account access. This visibility does
  not automatically give the maintainer access to their accounts.
- Record the **Client ID**, not the numeric App ID. Keep generated client
  credentials out of chat, source, descriptors, submissions and logs.

The GitHub CLI is authenticated for repository operations. Its login does not
provide a browser session or a general endpoint for registering GitHub Apps.
Browser sign-in (and any GitHub security verification) is an operator step.

## Desktop credential approach

Use a dedicated registered desktop client with PKCE and the existing loopback
callback. GitHub's official desktop MCP implementation documents an embedded
client credential as **public and extractable**, with PKCE binding each login.
It is not an account token, confidential server secret, publisher identity or
signing key. No new OAuth proxy, downstream server or background service is added.

References:

- [GitHub host integration](https://github.com/github/github-mcp-server/blob/main/docs/host-integration.md)
- [Official desktop OAuth implementation](https://github.com/github/github-mcp-server/blob/main/docs/oauth-login.md#how-it-works)
- [Registering a GitHub App](https://docs.github.com/en/apps/creating-github-apps/registering-a-github-app/registering-a-github-app)

The backend accepts these **compile-time** build variables:

| Variable | Purpose |
| --- | --- |
| `GRAIN_GITHUB_OAUTH_CLIENT_ID` | Dedicated alpha app's OAuth client ID |
| `GRAIN_GITHUB_OAUTH_CLIENT_SECRET` | GitHub-required desktop client credential; extractable from the built app |

Supply both from local credential storage or the controlled alpha build step;
do not paste them into tracked files or command lines that will be logged.
Absent/incomplete/invalid values do not enable the built-in registration. Rust's
`option_env!` tracks these compile-time inputs for rebuilds. Changing runtime
environment variables after compilation does not change an installed build.
The existing **Build Test** workflow now has a `github-extension-alpha` option.
When enabled, only its Windows alpha job runs, reusing `build.yml`. Save these
two names as Actions repository secrets on `Punit-Dethe/Grain`. The job validates
their presence/grammar without logging values and passes them only to the alpha
build; ordinary build/release paths do not receive this registration.

Run the workflow on `extensions/tool-only-retirement` with the alpha option
enabled. The resulting `grain-extensions-alpha-x86_64-pc-windows-msvc` artifact
contains an NSIS installer. It is not Authenticode signed; Windows may warn on
installation. This job creates no GitHub Release, publishes no updater manifest
and does not access updater signing keys. The alpha has its own product name and
app identifier/profile, with no updater endpoints or updater artifacts. For now
testers install replacements manually; a future alpha updater feed is a separate
delivery decision. Beta configuration and release/tag workflows are unchanged.
This application installer does not activate or sign the extension catalogue.

The user's registered app is
[Grain GitHub Development](https://github.com/apps/grain-github-development).
Both Actions secret names were verified without reading values. The public app
page currently says **private**: testing on its owner's account can proceed,
but other tester accounts require installation availability **Any account**.
Live account consent, repository permission settings and credential correctness
are verified by actual provider acceptance, not by secret-name checks.

Automatic resolution is restricted to a current, host-verified store connection
for `com.grain.github`, OAuth authentication, and exactly
`https://api.githubcopilot.com/mcp/`. It does not apply to custom MCPs, developer
catalogue entries, differently named packages, revoked owners or changed URLs.
Discovered issuer, authorization endpoint and token endpoint must exactly match
GitHub's current metadata before the build credential is supplied. Discovery
continues through the maintained SDK; the binding is not substitute discovery.

Explicit per-connection credentials take precedence and keep their existing
issuer-bound vault checks. User account access/refresh tokens remain in the
existing connection-owned OS vault. Restart and refresh reuse the same built-in
registration; changing its client ID requires reconnecting, without migration.
The existing installed-extension Connect/Disconnect controls are reused.

## Acceptance and remaining work

### Published GitHub checkpoint, 9 October

The public signed catalogue now contains GitHub 0.1.0, index version 3 at registry
commit `ea7498aebc028e35579878f5af5f9b1c329c48a2`. Its hosting bundle receipt is
`17c4e1a56e289256e07ea97a2f0f9c52232be54e434d57ac2de81dd4daffe964`.
The initial/bootstrap and update workflows both passed; public delivery checks
17 exact files with 40 requests across commit-pinned/current routes and the final
metadata recheck. The normal Grain store backend's genuine live test confirms
freshness, installability and visible GitHub membership, using the MCP validator.
Its first attempt caught the old native-only test assumption; that was corrected,
not a runtime permission or validation bypass. Maintainer suite: 83 Pass;
affected final approval/review checks and strict maintainer Clippy/build Pass.

Use the accepted replacement alpha installer from run **37932435112**:
`C:\Projects\Grain\grain-github-alpha-build-37932435112\nsis\Grain Extensions Alpha_0.0.6_x64-setup.exe`.
SHA256: `0ffaf0d3121cc635a8de1d70e5762421cec930519b47ce4e7b55fce3294f4658`.
The older run **37926347284** has the previous trust pins and is superseded.
Subsequent approval/tooling and test-only changes do not require another app build.

The operator-held approval policy records the user's explicit 9 October decision;
SHA256 `7ce6a84b6acf8dc778069c8acdf5466339de85e3a250a812aa283a2c0d8641c9`.
No independent review is claimed. Source/provenance checks used registry merge
`db6f4f31bd28668574239c655d4d66f256571194` and reviewed head
`b3fa7e3ab9899e0a03f190a63731db796e5f17db`, with the original v0.1.0 source/tag
unchanged. Publication verifier is Grain
`7baaaab004dea55983e89e7742394f3c76be5e7f`. Signing credentials stay local;
neither author CI nor publication workflows receive them. The bootstrap retired
obsolete catalogue entries/assets directly, without carrying old extensions
forward. Permanent production protections and offline/backup custody are not
certified by this tester-alpha publication. Renew signed metadata before its
earliest 8 November expiry using the existing renewal/update route.

### Current user test batch

The user reports that Store installation and GitHub browser sign-in/connection
work in the installed alpha. Agent read/write and restart/disconnect acceptance
remain Pending. On 9 October, follow-up debugging confirmed that enabling Agent
in the UI still used the retired built-in extension command, which rejected the
request. The frontend ignored its structured error and could display a switch
that was on without saving the setting or registering its shortcut. This was an
application defect, not a missed user setup step. Agent and Snippets now have
dedicated core settings commands; their switches check backend errors and roll
back rejected changes. Agent shortcut registration failures are returned before
the enabled setting is changed. A replacement alpha build is required; run
37932435112 predates this fix. Live Agent acceptance remains Pending.

The fix is pushed at `a2215c73ad08f8394b76006045a74645cc25aacc`. Replacement
[alpha build 37955644860](https://github.com/Punit-Dethe/Grain/actions/runs/37955644860)
was dispatched with the existing registered GitHub client. It is not yet an
accepted installer: wait for successful build/artifact delivery, then test the
Agent switch and shortcut in that replacement. Do not repeat this procedure
on the old installer and expect the code fix to be present. The four frontend
regressions exercise the real generated command bindings, mocking only Tauri
IPC; the production frontend bundle and TypeScript check pass. The existing
two capture shortcut conflict tests also pass. Native installed-window behavior
still requires verification with the corrected build.

1. Quit the running Grain, install the replacement alpha above and configure the
   Agent model in its separate profile if needed. Open **Agent** in the sidebar,
   turn on the switch beside **Grain Agent**, and use the shortcut displayed below
   it. Development settings do not enable Agent in the alpha profile. Open
   Extensions, click Developer
   once to enable the alpha gate, then close the drawer without turning it off.
2. In the usual GitHub browser, install/configure **Grain GitHub Development** on
   only `Punit-Dethe/grain-github-alpha-test` (private disposable repo with README).
   In Grain's Store, GitHub should show its DESCRIPTION/artwork. Install it, then
   connect from Installed and approve GitHub sign-in. No OAuth developer fields
   or new app registration should be required.
3. Ask the Agent: `Use GitHub to read the README in Punit-Dethe/grain-github-alpha-test.`
   Expect the actual README, using GitHub rather than a local smoke tool.
4. Ask: `Create exactly one issue in Punit-Dethe/grain-github-alpha-test titled
   Grain alpha acceptance, with body Created through Grain for testing. Return
   its link.` Approve the action and open the returned link to verify one issue.
5. Restart the alpha, ask the Agent to read that same issue, then disconnect and
   reconnect GitHub from Installed. Expect preserved login on restart, accurate
   disconnected state and ordinary browser sign-in again without developer IDs.

Owner account testing can use the current private GitHub App. Other testers need
its installation visibility changed to Any account. GitHub's actual account
permission settings, OAuth credentials and tool outcomes are certified by these
provider checks, not by store publication or the generic test suite.

### Catalogue signing identity reset, 9 October

The first Windows alpha build completed successfully at
`2aa54d0aabd067adb343aeac360a87de3930aab5`. Its downloaded installer SHA256 is
`d1fe19a00623be4435ffae0b87a9332555035a376d2a23a6b1651445665788a1`.
It contains the previous catalogue trust pins and is superseded for the next
public-catalogue acceptance. Do not tell testers that this installer alone makes
the GitHub listing available.

The user confirmed the only available GitHub signing secret was
`TAURI_SIGNING_PRIVATE_KEY`. This is the application updater identity, not the
catalogue publisher. No secret value was retrieved or reused. The registry has
the old public catalogue key, but no catalogue signing secrets were listed in
the repository or its `publish` environment.

New encrypted tester-alpha root A, spare root B and publisher keys were generated
using the existing maintainer CLI. They live outside all source/build trees at
`C:\Users\watrm\AppData\Local\GrainSigning\extensions-alpha-20261009`, restricted
to the current Windows account. Each role has its own key and random unlock file;
neither contents nor passwords were displayed. This local arrangement is for
the tester alpha, not a production KMS or independently backed-up offline root.
The maintainer must retain a secure backup of this directory before distributing
the alpha. Do not upload it, attach it to evidence, or put it in a repository.

Public publisher key:
`RWREj+hdLljbWOv3LCoTKEPv670pVG+P9knBUMWoR+V445yvdBwBuqj8`.
The app pins both new roots, and its empty seed signatures have been regenerated.
Existing genuine equivocation fixtures were re-signed; the independent public
test fixture retains its own signer and only updates its seed-byte binding.
No old settings, catalogue identities or extensions are migrated.

The existing CLI created and verified an empty bound bootstrap (index/revocation
version 2, thirty-day lifetime), local snapshot store and complete hosting bundle
at `C:\Projects\Grain\grain-alpha-publication-20261009`. Independent recorded
bundle receipt SHA256:
`dadb8d3d0dee04e40fdffe0c42fa6c0c84f80daac739fe6d80bcd933d7e362cb`.
Actual verification against each of the two pinned roots passed. These files are
unpublished preparation; the GitHub extension has not been signed into them.

The user then authorized explicit maintainer approval for the first-party tester
alpha. The existing signer now accepts an operator-held `alpha_maintainer_approval`
reference only for the exact GitHub 0.1.0 submission digest, named registry/owner
and dedicated alpha publisher. It uses `review_id: 0` and makes no claim of a
GitHub approving review. Merged PR/head/source/DESCRIPTION, full byte/provenance
pins, expiry, prior catalogue and delayed key checks remain required. Third-party
submissions and changed versions retain independent review. This narrow exception
is temporary maintainer tooling, to be removed when the permanent approval policy
is established; it adds no application, SDK or harness feature.

The replacement alpha build
[37932435112](https://github.com/Punit-Dethe/Grain/actions/runs/37932435112) and
Linux identity checkpoint
[37932600938](https://github.com/Punit-Dethe/Grain-Extention/actions/runs/37932600938)
passed. The app identity remains Grain `8d0acf414e31f4ee8d67e3be03116ab944a40772`;
the approval change affects maintainer tooling only. Publication must use a trusted
verifier commit including the new approval profile and alpha trust pins.

Checks: 12 core trust, 4 bootstrap, 4 initial-publication and 11 normal backend
store tests pass; store cleanup Pass (`logic-q1xTsS`). Fresh producer/candidate
attestation verification and prepared GitHub byte verification pass. No new
harness, signing service, runtime feature or dependency was added. Replacement
alpha packaging, signed GitHub publication, public HTTP/store and live provider
acceptance remain separate pending steps.

Keep automated production tests and actual-provider acceptance separate:

1. Normal backend MCP tests: exact resource/source/issuer binding, malformed build
   inputs, current/revoked ownership, SDK PKCE and registered-client refresh.
2. After registration, build with the real alpha identity; install the signed
   GitHub extension through the existing store route and authorize one test repo.
3. Read a disposable repository, approve one issue creation, independently verify
   it, restart and read again; disconnect/reconnect and remove the extension.
4. A second tester confirms Connect works without entering app credentials.

Do not mark registration, live GitHub tools, signed catalogue delivery or alpha
packaging accepted merely because the generic/backend tests pass. Existing
signing and publication verification stay intact. Actual Linear-issued token
expiration remains the previously deferred separate check.
