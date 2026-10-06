# Provisional hosting and portable public identity

Date: 6 October 2026. Application branch: `extensions/tool-only-retirement`.

## Decision and boundaries

The user authorized the existing experimental Vercel free-tier website for
provisional use. This supersedes the earlier prohibition on using an experimental
site for **preparing** Grain's public client document; it does not decide the
permanent identity or certify production/provider acceptance.

Use GitHub for source, submissions, independent reviews, locked builds,
attestations and release evidence. Recommend Vercel for initial static registry
staging and the public client document. Registry staging should be a separate
project/deployment from the landing page, with a separate controlled release
path. No new database, serverless function, authentication gateway or hosted Agent
is needed. Native extensions still execute in Grain; remote MCP servers belong
to their providers. Hosting registry files does not host those MCP servers.

Two distinct hosting contracts must not be conflated:

| Resource                     | Purpose                                                                                    | Candidate / current location                                                                                                    | Migration cost                                                                                                               |
| ---------------------------- | ------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| Public OAuth client document | Tell supporting authorization servers which native application and callback request access | Candidate `https://usegrain.vercel.app/oauth/client.json`                                                                       | Static file is easy to move; changing its exact URL changes OAuth client identity and can require fresh registration/consent |
| Signed extension registry    | Serve trusted catalogue, revocations and addressed native/MCP/description/image files      | Existing signed base is GitHub raw `Punit-Dethe/Grain-Extention/main/v1/`; separate Vercel staging recommended, not provisioned | Static bundle is portable; switching trusted bases needs a reviewed root/bootstrap migration and retained historical assets  |

## Actual site discovery and delivered changes

The ignored nested `website/` is the separate
[Grain-Website repository](https://github.com/Punit-Dethe/Grain-Website).
Its GitHub homepage identifies `usegrain.vercel.app`. Ordinary unauthenticated
HTTPS checks return **200 HTML** for `/` and **404** for `/oauth/client.json`
before deployment. The actual successful production deployment points to website
commit `6aba44baea5630a628904fa5b51b44b5d61eaa73`. The older local website checkout
was clean and was not the newest remote main; preparation branched from that
verified remote main without overwriting landing-page files.

Website branch `oauth/public-client-metadata` contains only the public JSON,
one-path Vercel header configuration and maintainer README guidance. Final source:
`ca0ace6e74fc9bd5d76b362cbe59fca99930238c`.
[Website PR 1](https://github.com/Punit-Dethe/Grain-Website/pull/1) is open and
unmerged. There are no client secrets, account records, provider scopes, tokens
or signing keys in the document. The exact existing IPv4 loopback callback,
native application type, code/refresh grants and public-client authentication
method are preserved. Website HTML/CSS/JavaScript are unchanged.

The actual Vercel preview deployment `6877034975` succeeds at this exact commit.
Its unauthenticated document request returns **302 to Vercel SSO**, so deployment
success is not endpoint verification. No protection bypass, project setting,
production promotion or main merge was performed. Do not use a preview/commit URL
as the document identity. Public canonical response acceptance remains pending.

Local existing metadata validation and all **three** existing client-metadata
tests pass. Additional bounded checks confirm canonical identity, preview-URL
refusal, JSON formatting and configuration scoped to exactly one route. The first
configuration formatting check failed and was corrected in a follow-up commit;
only the final check is accepted. No Agent harness source/scenario, dependency,
runtime listener, desktop build or new manual batch was added.

The two used configuration fields also validate against the actual public Vercel
schema's corresponding definitions. An initial attempt to meta-validate that
entire upstream schema failed on unrelated experimental function/service fields
(numeric `exclusiveMinimum` despite its draft-04 declaration); it earns no Pass.
No upstream schema, app dependency or validation rule was changed. Successful
Vercel deployment plus scoped field validation does not certify public response
headers behind the preview login wall.

## Public OAuth deployment and later transition

After promoting the reviewed website changes to its canonical project URL:

```powershell
node scripts/check-mcp-client-metadata.mjs https://usegrain.vercel.app/oauth/client.json --file website/oauth/client.json
node scripts/check-mcp-client-metadata.mjs https://usegrain.vercel.app/oauth/client.json --live
```

Require direct unauthenticated HTTPS 200 JSON without redirects, login walls or
challenges. Check the exact deployed bytes/callback and expected response headers;
the existing live checker validates status, JSON type, size, deadline and identity,
but does not certify all Vercel cache/security settings. A five-minute revalidation
policy is configured for this document; it is not an immutable asset.

Publishing alone does not turn the SDK mechanism on. Grain's
`CLIENT_METADATA_URL` remains `None`, and criterion **56 remains Pending**. Enable
an explicitly chosen host identity in a separate reviewed application unit, repeat
existing affected registration/refusal/cancel/restart checks and validate one
eligible real provider. Use it only where the authorization server advertises
support; configured registrations and SDK DCR compatibility remain available.
No new auth engine or provider-universal support claim is introduced.

Before public release, prefer a stable Grain-owned custom domain/path. Moving the
hosting behind that unchanged URL avoids changing identity. If a Vercel URL is
used now and a new domain later replaces it, update both the document and Grain,
test actual provider behavior, and plan new consent/registration where necessary.
Keep the old endpoint/registration available during migration; do not redirect
or silently relabel old grants. Refresh continuity is provider-dependent and must
be checked. There is no large code rewrite, but the identity transition is not
guaranteed invisible to users. Vercel project renaming/deletion must not remove
an identity that deployed clients still use.

## Signed registry deployment sequence

1. Keep the reviewed append-only serving store and independent pointer pin as the
   release source. Import/protect earlier publication history before treating
   bootstrap as complete. Do not deploy the repository's incomplete legacy
   publisher or merge its draft as a shortcut.
2. Export `export-hosting-bundle` into a fresh directory and independently verify
   the receipt pin with `verify-hosting-bundle`. Keep operational receipts/proofs
   outside the served output where practical; the serving layout must retain all
   required `v1/blob/` and `v1/media/` hashes. A staging address is not yet a
   production trusted base. Do not rewrite signed JSON while copying it.
3. Deploy static files to a separately controlled staging project; no author build
   code or signing key executes there. Check its public exact bytes, addressed
   downloads, MIME policy and old withdrawn assets with the existing real store
   at the changed integration boundary. Verify the candidate before activation.
4. Implement the actual release serialization/current-deployment check and a
   documented authorized-writer boundary. A local file lock or GitHub workflow
   concurrency alone does not prevent another Vercel dashboard/API writer.
   Vercel promotion is not assumed to provide an expected-current compare-and-swap.
   Whole-deployment availability also does not bind a client's several sequential
   metadata requests to the same deployment: the current store fetches separate
   roots/index/revocation documents and signatures. Coherence during publication,
   key rotation, cache skew and revocation failure must be addressed and tested
   before accepting hosted activation. Do not claim offline bundle verification
   closes this boundary.
5. Once the route/protection/retention design is accepted, perform a signed root
   update and align fresh-client bootstrap plus existing-client migration. Keep
   the prior origin and historical addressed files through the supported client
   transition. Retain protected source/history independently of provider preview
   retention; old deployment URLs are not a durable asset archive.
6. Finish actual independent positive release approval, key custody, metadata
   renewal/revocation and a controlled outage/recovery procedure. Store UI work
   then continues in E5; the hosting choice does not certify shipping readiness.

Vercel Hobby is appropriate only within its eligibility and size/usage limits.
Its current CLI source upload limit is **100 MB** and source-file count **15,000**;
the registry's larger local bundle limits do not guarantee Hobby deployment.
Preflight the chosen delivery method's actual limits rather than discard historical
assets to fit. Its non-commercial/personal-use rule also needs reassessment before
a commercial production rollout. These are hosting constraints, not new Grain
limits. Another static host/object store can serve the same layout if it better
fits production needs; no provider-specific runtime API belongs in Grain.

## Roadmap and sources

This completes provisional identity preparation and records the hosting decision,
not a new numbered acceptance criterion. Baseline **52 Pass / 1 Deferred (12)**,
forward **71**, full **56 Pending** and Agent inventory **108 / 93 / 81** are
unchanged. **Five E4–E8 stages retain work**. Physical obsolete-code removal remains
on hold. Root generated bindings and all pre-existing nested registry edits are
preserved. No registry signing, trust anchor/base change or production activation.

Primary sources checked 6 October:

- [MCP draft client registration](https://modelcontextprotocol.io/specification/draft/basic/authorization/client-registration): exact HTTPS URL identity and advertised-support selection; this is the draft, not a claim of a final OAuth RFC.
- [Vercel generated URLs](https://vercel.com/docs/deployments/generated-urls): separate commit/branch/production/custom-domain identities and deployment retention.
- [Vercel configuration](https://vercel.com/docs/project-configuration/vercel-json) and [public schema](https://openapi.vercel.sh/vercel.json): path-specific static headers.
- [Vercel promotion](https://vercel.com/docs/deployments/promoting-a-deployment): promotion is an explicit deployment operation; no distributed CAS guarantee is inferred.
- [Hobby plan](https://vercel.com/docs/plans/hobby) and [limits](https://vercel.com/docs/limits): eligibility, usage and delivery-method limits.

Graph-first minimal context and file summaries preceded source/deployment reads;
the separate website and new planning file were not indexed. Actual GitHub
deployment records and HTTPS responses supplement the graph. Review is scoped to
public metadata, host ownership, migration and unchanged release gates. This is
not a whole-platform security audit or actual-provider acceptance.
