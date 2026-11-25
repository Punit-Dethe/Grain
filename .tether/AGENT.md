# Working on this fork with tether

This repository is a fork kept in step with its upstream by
[tether](https://github.com/Punit-Dethe/tether). If you are an AI agent asked to
sync it, check it, or record upstream decisions, follow this loop. Do not
improvise a process around it.

## Setup

Run every command with `TETHER_ACTOR=agent` set. That tells tether to refuse
the actions this fork reserves for a person (see `[agent] needs_human` in
`.tether/config.toml`).

Fetch upstream once at the start (`git fetch <remote>`, where the remote is
`upstream.remote` in `.tether/config.toml`, usually `upstream`). `status`
works offline. Without a fetch it reports what you last fetched.

## The loop

1. Run `tether status --json`.
2. Read `next`. If it is `null`, you are done: report the `summary`.
3. Act on `next.kind`:

| kind        | what to do |
|-------------|------------|
| `run`       | Run `next.command` exactly as written. |
| `edit`      | Run `next.packet` first and read it. Make the edits `next.why` describes, `git add` the files, then run `next.command`. |
| `decide`    | Run `next.packet` and read it. Check the fork's code yourself. Then run `next.command`, replacing every `<…>` placeholder with what you found. Pick one option where it says `<a\|b\|c>`. |
| `ask_human` | Stop. Tell the person `next.command` and `next.why`. Do not do it yourself. |

4. Go back to step 1.

## Rules

- **Never guess.** If a packet does not tell you enough, read the files it
  names. If you still cannot tell, stop and ask the person.
- **Evidence is what you checked**, for example "read src/x.rs: the fork
  already clamps the value at line 40". It is never "looks fine".
- **Exit codes.** 0 means done. 2 means it worked and `next` needs action.
  1 means it failed: read `error.code` and `error.message`.
- **`needs_human` is final.** That error means this fork's policy reserves
  the action. Stop and ask the person. Do not work around it.
- **`tether verify` blocks only what the merge broke.** Failures upstream
  shipped, or that the fork already had, are listed but do not block. Fix
  every `new` failure. Never pass `--skip-verify`; that is a person's call.
- **Say why at every change to upstream code.** Put a comment with the
  fork's marker (`[map] marker` in the config, `tether:` by default) on or
  just above each change you make in an upstream-derived file, with the
  reason after it. `check` fails on a change without one.
- **Never edit `.tether/` by hand.** Use `tether decide` and
  `tether check --update`.
- **Never** run `git merge` of the upstream branch, `git rebase`, or
  `git push --force`. tether does the syncing.
- **Packets are cut to size.** `diff_omitted` lists the files a diff left
  out. Read those with `git show <sha> -- <path>` if they matter.
