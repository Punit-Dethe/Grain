// B1a exercises production startup, approval and real native workers. All disk
// mutations occur between owned host lifetimes except deliberate oracle faults.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile, writeFile, mkdir, realpath } from "node:fs/promises";
import { join } from "node:path";
import { assertWithin } from "./support.mjs";
import { FIXTURE_ID, TYPED_INPUTS } from "./model.mjs";
import { withRegistryLock } from "./registry.mjs";

export function foundationHandlers(ctx) {
  const {
    root,
    status,
    fixture,
    request,
    greeting,
    activate,
    waitFor,
    events,
    restartHost,
  } = ctx;
  const data = assertWithin(root, join(root, "data"));
  const path = (name) => assertWithin(root, join(data, name));
  const read = async (name) => JSON.parse(await readFile(path(name), "utf8"));
  const write = (name, value) => writeFile(path(name), JSON.stringify(value));
  let evidence = [];
  async function restart(stage, operation) {
    const before = (await status()).hostSession;
    await restartHost(operation);
    const after = await status();
    assert.equal(after.hostSession, before + 1, "No actual migration restart");
    assert.equal(after.worker.count, 0, "Migration started a native worker");
    evidence.push({
      stage,
      session: after.hostSession,
      workers: after.worker.count,
    });
  }
  return {
    takeEvidence() {
      const result = evidence;
      evidence = [];
      return result;
    },
    handlers: {
      async "native.author-results"() {
        const sourcePath = assertWithin(
          root,
          join(root, "fixture/dist/main.js"),
        );
        const original = await readFile(sourcePath, "utf8");
        const cases = [
          ["tagged-null", "null", "succeeded"],
          ["tagged-empty-text", '""', "succeeded"],
          [
            "plain-display",
            '{ body: "Harness contract plain" }',
            "succeeded",
            "Harness contract plain",
          ],
          [
            "invalid-boolean",
            "false",
            "resultUnavailable",
            "invalid action result",
          ],
          [
            "ambiguous-null-envelope",
            "{ ok: null, error: null }",
            "resultUnavailable",
            "invalid action result",
          ],
          [
            "unsupported-follow-up",
            "{ needsInteraction: null }",
            "resultUnavailable",
            "extension follow-up is not available",
          ],
          [
            "invocation-context",
            '{ body: "Harness contract context: " + JSON.stringify({ arguments: Object.keys(args), context: Object.keys(context), keyType: context.idempotencyKey === null ? "null" : typeof context.idempotencyKey }) }',
            "succeeded",
            "Harness contract context:",
          ],
        ];
        try {
          for (const [stage, value, classification, uiText] of cases) {
            await fixture("unload");
            const tagged = [
              "tagged-null",
              "tagged-empty-text",
              "invalid-boolean",
            ].includes(stage)
              ? `{ ok: ${value} }`
              : value;
            await writeFile(
              sourcePath,
              `grain.actions({ hello: async function(args, context) { return ${tagged}; } });\n`,
            );
            await fixture("load");
            try {
              const call = await ctx.failureCall("hello", classification, {
                retire: false,
                uiText,
              });
              if (stage === "invocation-context") {
                const text = await call.page.locator("body").innerText();
                const match = text.match(
                  /Harness contract context: (\{[^\n]+\})/,
                );
                assert.ok(match, "Invocation context result was not displayed");
                const actual = JSON.parse(match[1]);
                assert.deepEqual(actual.arguments, []);
                assert.deepEqual(actual.context, ["idempotencyKey"]);
                assert.ok(["string", "null"].includes(actual.keyType));
              }
              evidence.push({
                stage,
                classification,
                dispatchDelta: 1,
                pending: call.after.worker.current.pending,
              });
            } catch (error) {
              evidence.push({ stage, status: "Fail", error: error.message });
              throw new Error(
                `Native author result ${stage}: ${error.message}`,
                { cause: error },
              );
            }
          }
        } finally {
          await fixture("unload");
          await writeFile(sourcePath, original);
        }
      },
      async "native.typed-contract"() {
        async function typed(instruction) {
          const action = instruction.startsWith("typed_number_")
            ? "typed_optional_echo"
            : "typed_echo";
          const before = await status();
          const modelStart = ctx.model().journal.length;
          const page = await request(instruction);
          assert.equal(
            events(await status(), "dispatched").length,
            events(before, "dispatched").length,
          );
          await activate(page.locator(".agc-confirm-actions .agc-action-btn"));
          await waitFor(
            "Typed native result and Agent completion",
            async () => {
              const value = await status();
              return (
                !value.agent.active &&
                events(value, "outcome", action).length ===
                  events(before, "outcome", action).length + 1
              );
            },
          );
          const after = await status();
          assert.equal(
            events(after, "dispatched", action).length,
            events(before, "dispatched", action).length + 1,
          );
          assert.equal(
            events(after, "outcome", action).at(-1).outcome,
            "succeeded",
          );
          assert.equal(
            ctx
              .model()
              .journal.slice(modelStart)
              .filter((entry) => entry.typedVerified).length,
            1,
            "Model did not verify the worker's typed result",
          );
          assert.ok(
            !ctx
              .model()
              .journal.slice(modelStart)
              .some((entry) => entry.state === "error"),
            "Typed result oracle rejected native output",
          );
          await page
            .getByText("Harness verified native typed result", { exact: false })
            .first()
            .waitFor({ state: "visible", timeout: 10000 });
          evidence.push({
            stage: instruction,
            typesVerified: true,
            dispatchDelta: 1,
          });
        }
        for (const instruction of Object.keys(TYPED_INPUTS))
          await typed(instruction);
        const before = await status();
        const pending = await request("typed_omitted");
        const manifestPath = assertWithin(
          root,
          join(root, "fixture/manifest.json"),
        );
        const original = await readFile(manifestPath, "utf8");
        const changed = JSON.parse(original);
        const changedAction = changed.contributes.actions.find(
          (action) => action.id === "typed_echo",
        );
        changedAction.params.find((param) => param.name === "note").required =
          true;
        changedAction.utterances = [
          "harness typed {text} {count} {entity} {note}",
        ];
        try {
          // No reload or generation change: only the current declaration changes.
          await writeFile(manifestPath, JSON.stringify(changed));
          assert.deepEqual((await status()).fixtureOwner, before.fixtureOwner);
          assert.equal(
            (await status()).worker.current.identity,
            before.worker.current.identity,
            "Declaration edit unexpectedly reloaded the worker",
          );
          await activate(
            pending.locator(".agc-confirm-actions .agc-action-btn"),
          );
          await waitFor(
            "Changed native declaration refusal",
            async () =>
              !(await status()).agent.active &&
              !(await status()).agent.pendingApproval,
          );
          assert.equal(
            events(await status(), "dispatched").length,
            events(before, "dispatched").length,
            "Changed contract dispatched the old call",
          );
          assert.match(
            await pending.locator("body").innerText(),
            /no longer approved|changed|expired|unavailable/i,
          );
          evidence.push({
            stage: "contract-change-without-reload",
            dispatchDelta: 0,
          });
          await fixture("load");
          await typed("typed_values");
        } finally {
          await writeFile(manifestPath, original);
        }
      },
      async "native.legacy-migration"() {
        assertWithin(await realpath(root), await realpath(data));
        await ctx.imported("migration-baseline");
        await ctx.allow();
        await greeting("migration-baseline");
        const settings = await read("grain.settings.json");
        const registry = await read("extensions.json");
        const record = registry.records[FIXTURE_ID];
        const artifactName = `extensions/${FIXTURE_ID}/${record.installed_version}/${record.artifact_sha256}/pack.grainpack.json`;
        const legacy = await read(artifactName);
        legacy.manifest.permissions = ["capture"];
        const bytes = Buffer.from(JSON.stringify(legacy));
        const hash = createHash("sha256").update(bytes).digest("hex");
        const legacyArtifact = `extensions/${FIXTURE_ID}/${record.installed_version}/${hash}/pack.grainpack.json`;
        const user = {
          id: "harness-user-prompt",
          name: "My own prompt",
          prompt: "Preserve my independent instructions",
        };
        const retired = {
          id: `ext:${FIXTURE_ID}:edited`,
          name: "Edited legacy prompt",
          prompt: "User-edited legacy text",
        };
        const key = `ext:${FIXTURE_ID}:hello`;
        const binding = {
          id: key,
          name: "Edited binding",
          description: "Legacy fixture only",
          default_binding: "Ctrl+J",
          current_binding: "Ctrl+Shift+J",
        };
        const sentinel = path("migration-user-data.txt");
        async function seed(prompt, interrupted) {
          const seeded = structuredClone(registry);
          seeded.tool_only_migration_version = 0;
          seeded.records[FIXTURE_ID].artifact_sha256 = hash;
          seeded.records[FIXTURE_ID].enabled = true;
          seeded.records[FIXTURE_ID].granted = ["capture"];
          const old = structuredClone(settings);
          old.post_process_prompts = [
            ...settings.post_process_prompts,
            user,
            prompt,
          ];
          old.post_process_selected_prompt_id = prompt.id;
          old.bindings[key] = binding;
          await mkdir(
            assertWithin(
              root,
              join(
                data,
                `extensions/${FIXTURE_ID}/${record.installed_version}/${hash}`,
              ),
            ),
            { recursive: true },
          );
          await writeFile(path(legacyArtifact), bytes);
          await write("extensions.json", seeded);
          await write("grain.settings.json", old);
          if (interrupted) {
            // Equivalent durable checkpoint: archives published, old active
            // settings/registry still present. Startup must finish idempotently.
            await write("retired-extension-prompts.json", [retired, prompt]);
            await write("retired-extension-bindings.json", [[key, binding]]);
          }
        }
        async function verify(expectedPrompts) {
          const migrated = await read("extensions.json");
          const old = migrated.records[FIXTURE_ID];
          assert.equal(migrated.tool_only_migration_version, 1);
          assert.equal(
            old.enabled,
            false,
            "Legacy capture extension re-enabled",
          );
          assert.deepEqual(old.granted, []);
          assert.equal(old.actions_approved, undefined);
          assert.ok(
            migrated.quarantined[FIXTURE_ID],
            "No persistent retirement reason",
          );
          const active = await read("grain.settings.json");
          assert.deepEqual(active.post_process_prompts, [
            ...settings.post_process_prompts,
            user,
          ]);
          assert.ok(
            active.post_process_prompts.some(
              (prompt) => prompt.id === active.post_process_selected_prompt_id,
            ),
          );
          assert.ok(!active.post_process_selected_prompt_id.startsWith("ext:"));
          assert.ok(
            !Object.keys(active.bindings).some((value) =>
              value.startsWith("ext:"),
            ),
          );
          assert.deepEqual(
            active.bindings,
            settings.bindings,
            "Migration changed first-party shortcut bindings",
          );
          assert.deepEqual(
            await read("retired-extension-prompts.json"),
            expectedPrompts,
          );
          assert.deepEqual(await read("retired-extension-bindings.json"), [
            [key, binding],
          ]);
          assert.deepEqual(
            await readFile(path(legacyArtifact)),
            bytes,
            "Legacy artifact was destroyed",
          );
          assert.equal(
            await readFile(sentinel, "utf8"),
            "Preserved inert user data",
          );
          assert.equal((await status()).worker.count, 0);
          return {
            promptArchive: await readFile(
              path("retired-extension-prompts.json"),
            ),
            bindingArchive: await readFile(
              path("retired-extension-bindings.json"),
            ),
          };
        }
        await restart("upgrade:fresh-archive", async () => {
          await writeFile(sentinel, "Preserved inert user data");
          await seed(retired, false);
        });
        if (ctx.fault === "lost-migration-archive")
          await write("retired-extension-prompts.json", []);
        const first = await verify([retired]);
        await assert.rejects(
          fixture("enable"),
          /quarantined|retired|tools only/i,
        );
        assert.equal((await status()).fixtureEnabled, false);
        await restart("upgrade:repeat");
        assert.deepEqual(
          await verify([retired]),
          first,
          "Restart changed inert archive bytes",
        );
        const edited = {
          ...retired,
          prompt: "A second user edit survives interruption",
        };
        await restart("upgrade:interrupted-checkpoint", () =>
          seed(edited, true),
        );
        const second = await verify([retired, edited]);
        await restart("upgrade:interrupted-repeat");
        assert.deepEqual(await verify([retired, edited]), second);
        const beforeRemoval = await read("extensions.json");
        await withRegistryLock(ctx, async () => {
          await assert.rejects(
            fixture("remove_installed"),
            /persist.*extensions\.json/i,
          );
        });
        assert.equal(
          (await status()).fixtureInstalled,
          true,
          "Failed uninstall lost its live owner",
        );
        assert.deepEqual(await read("extensions.json"), beforeRemoval);
        await restart("upgrade:failed-uninstall-preserved");
        assert.deepEqual(await verify([retired, edited]), second);
        evidence.push({
          stage: "archives-preserved",
          promptVersions: 2,
          bindingEntries: 1,
          retiredDispatches: 0,
        });
        // Remove only through the actual host, then prove normal tools recover.
        await fixture("remove_installed");
        await fixture("load");
        await greeting();
        // Also exercise terminal removal of a quarantined developer-only owner.
        const manifestPath = assertWithin(
          root,
          join(root, "fixture/manifest.json"),
        );
        const original = await readFile(manifestPath, "utf8");
        await restart("upgrade:retired-dev-owner", async () => {
          const bad = JSON.parse(original);
          bad.permissions = ["capture"];
          await writeFile(manifestPath, JSON.stringify(bad));
        });
        assert.equal((await status()).fixtureEnabled, false);
        assert.ok((await read("extensions.json")).quarantined[FIXTURE_ID]);
        await fixture("unload");
        assert.equal(
          (await read("extensions.json")).quarantined[FIXTURE_ID],
          undefined,
        );
        await writeFile(manifestPath, original);
        await fixture("load");
        await greeting();
        evidence.push({
          stage: "retired-owner-recovery",
          installedRemoved: true,
          devRemoved: true,
          failedUninstallPreserved: true,
        });
      },
    },
  };
}
