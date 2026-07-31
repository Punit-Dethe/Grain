#!/usr/bin/env python3
"""UI parity gate — every setting must stay reachable through the UI rewrite.

The way a UI rewrite fails is not a crash. It is a setting that quietly stops
being reachable, and nobody notices for months. This turns that into a red CI
run: for every top-level field of ``AppSettings`` (as generated into
``src/bindings.ts``), assert the active UI tree can still reach it.

Reachability is deliberately generous, because a field can legitimately be
driven three different ways:

  * by **name** — ``updateSetting("paste_method", …)``;
  * by a **dedicated command** — the provider pools never touch
    ``stt_providers`` by name, they call ``sttUpsertProvider``;
  * by a **raw invoke** — the extension platform calls
    ``invoke("extension_set_developer_mode")`` rather than the typed binding.

All three count. Anything none of them reach must be listed in
``docs/UI 2.0/PARITY.md`` with a reason, or CI fails. That file is the record of
what was dropped ON PURPOSE — the whole point is that dropping a setting becomes
a decision someone wrote down, not an accident.

Modes::

    python scripts/ui_parity.py              # the gate
    python scripts/ui_parity.py --commands   # report-only: commands no UI calls
    python scripts/ui_parity.py --tree src/next   # gate a specific UI tree

Note: ``docs/`` is gitignored in this repo, so PARITY.md lives on disk only. If
it is missing the gate still runs; it just has no exceptions to honour.
"""

import argparse
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BINDINGS = os.path.join(ROOT, "src", "bindings.ts")
PARITY_DOC = os.path.join(ROOT, "docs", "UI 2.0", "PARITY.md")
CODE_EXT = {".ts", ".tsx"}


def read(path: str) -> str:
    with open(path, encoding="utf-8", errors="replace") as f:
        return f.read()


def app_settings_fields(src: str) -> list:
    """Top-level field names of the generated AppSettings type.

    Brace-matched rather than regexed to the first '}', because specta emits
    nested inline object types whose fields would otherwise be counted as
    settings (measured: 225 phantom fields vs the real 84).
    """
    start = src.index("export type AppSettings")
    open_brace = src.index("{", start)
    depth = 0
    for i in range(open_brace, len(src)):
        if src[i] == "{":
            depth += 1
        elif src[i] == "}":
            depth -= 1
            if depth == 0:
                end = i
                break
    body = re.sub(r"/\*.*?\*/", "", src[open_brace + 1 : end], flags=re.S)

    fields, depth, cur = [], 0, ""
    for ch in body:
        if ch in "{<([":
            depth += 1
        elif ch in "}>)]":
            depth -= 1
        if ch == ";" and depth == 0:
            m = re.match(r"([a-z_][A-Za-z0-9_]*)\s*\??\s*:", cur.strip())
            if m:
                fields.append(m.group(1))
            cur = ""
        else:
            cur += ch
    m = re.match(r"([a-z_][A-Za-z0-9_]*)\s*\??\s*:", cur.strip())
    if m:
        fields.append(m.group(1))
    return sorted(set(fields))


def commands(src: str) -> list:
    """Generated command functions, as (camelCase, snake_case) pairs."""
    out = []
    for camel in sorted(set(re.findall(r"^async (\w+)\(", src, re.M))):
        snake = re.sub(r"([A-Z])", lambda m: "_" + m.group(1).lower(), camel)
        out.append((camel, snake))
    return out


def tokens(name: str) -> set:
    """Word tokens of a snake_case name, singularised.

    `pp` and `stt` are the pool commands' shorthand for the settings prefixes
    `post_process` and `stt`, so they are folded together; without that,
    `pp_set_smart_rotation` would not be seen to cover
    `post_process_smart_rotation`.
    """
    parts = []
    for p in name.split("_"):
        parts += ["post", "process"] if p == "pp" else [p]
    # Singularise AFTER expanding, so both sides of a comparison get the same
    # treatment ("process" -> "proces" on one side only would never match).
    return {p[:-1] if len(p) > 3 and p.endswith("s") else p for p in parts}


def covers(field: str, command: str) -> bool:
    """Does this command plausibly own this settings field?

    Every word of the field must appear in the command name. Deliberately not
    the other way round: commands carry an extra verb (`set`, `upsert`) and
    often an extra noun, and requiring the field to be the subset is what makes
    the match specific rather than merely overlapping.
    """
    return tokens(field) <= tokens(command)


def ui_sources(tree: str) -> str:
    """Every UI source file, concatenated. bindings.ts itself is excluded — it
    mentions every field and command, so including it would make the gate pass
    unconditionally."""
    blobs = []
    for dirpath, _, names in os.walk(os.path.join(ROOT, tree)):
        for name in names:
            if os.path.splitext(name)[1] in CODE_EXT and name != "bindings.ts":
                blobs.append(read(os.path.join(dirpath, name)))
    return "\n".join(blobs)


def exceptions() -> dict:
    """Deliberately-unreachable fields, from PARITY.md.

    Format: a markdown table row per field, `| `field` | reason |`.
    """
    if not os.path.exists(PARITY_DOC):
        return {}
    out = {}
    for line in read(PARITY_DOC).splitlines():
        m = re.match(r"\s*\|\s*`([a-z_][a-z0-9_]*)`\s*\|([^|]*)\|", line)
        if m:
            out[m.group(1)] = m.group(2).strip()
    return out


def gate(tree: str) -> int:
    src = read(BINDINGS)
    fields = app_settings_fields(src)
    blob = ui_sources(tree)
    allowed = exceptions()

    unreachable = []
    called = [
        snake
        for camel, snake in commands(src)
        if f".{camel}(" in blob or f'"{snake}"' in blob
    ]
    for field in fields:
        if field in blob:
            continue
        # A dedicated command that covers the field counts: pp_set_smart_rotation
        # reaches post_process_smart_rotation, and extension_set_developer_mode
        # reaches extension_developer_mode, without either spelling the field.
        # Matched on word tokens (singularised) rather than substrings, so
        # `stt_providers` pairs with `stt_upsert_provider`.
        if any(covers(field, snake) for snake in called):
            continue
        unreachable.append(field)

    missing = [f for f in unreachable if f not in allowed]
    documented = [f for f in unreachable if f in allowed]

    print(f"[parity] {len(fields)} AppSettings fields, tree = {tree}/")
    if documented:
        print(f"[parity] {len(documented)} documented as backend-only or dropped:")
        for f in documented:
            print(f"    {f} - {allowed[f]}")
    if missing:
        print(
            "\n".join(
                f"[parity] FAIL: `{f}` is not reachable from {tree}/. Either surface "
                f"it, or add a row to docs/UI 2.0/PARITY.md saying why it is "
                f"backend-only or deliberately dropped."
                for f in missing
            ),
            file=sys.stderr,
        )
        return 1

    stale = [f for f in allowed if f not in unreachable]
    for f in stale:
        print(f"[parity] note: `{f}` is listed in PARITY.md but IS reachable - drop the row")

    print(f"[parity] OK: every setting reachable or documented")
    return 0


def command_report(tree: str) -> int:
    src = read(BINDINGS)
    blob = ui_sources(tree)
    unused = [
        camel
        for camel, snake in commands(src)
        if f".{camel}(" not in blob and f'"{snake}"' not in blob
    ]
    total = len(commands(src))
    print(f"[parity] {total} generated commands; {len(unused)} not called from {tree}/:")
    for c in unused:
        print(f"    {c}")
    print(
        "  Report only. Some are backend-internal or extension-only — the point is "
        "that someone reads this each phase."
    )
    return 0


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--tree", default="src", help="UI tree to check (default: src)")
    ap.add_argument("--commands", action="store_true", help="command reachability report")
    args = ap.parse_args()
    return command_report(args.tree) if args.commands else gate(args.tree)


if __name__ == "__main__":
    sys.exit(main())
