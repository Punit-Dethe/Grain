#!/usr/bin/env python3
"""Record a human verdict on an upstream commit, then rebuild the dashboard.

    python Upstream/verdict.py <sha|#PR> <Merged|Ignored|Pending> ["note"]
    python Upstream/verdict.py <sha|#PR> --note "note"     # note only
    python Upstream/verdict.py <sha|#PR> --clear           # drop the override
    python Upstream/verdict.py --pending                   # list what needs one

Most commits need no verdict at all: once a sync is merged, the commit is in
our ancestry and the ledger files it as `Merged` by itself. Reach for this only
to say something ancestry cannot — `Ignored` (Grain replaced that surface),
work applied by cherry-pick, or a note explaining a resolution.

It exists because verdicts used to be hand-typed into data.json, a 27 KB
generated file — so an edit and the next regeneration fought over the same
file. verdicts.json is the only tracker file a human owns; everything else is
derived from it.
"""

import json
import sys

import sync_upstream as sync

STATUSES = ("Merged", "Ignored", "Pending")


def load_ledger():
    """The current ledger, for resolving short SHAs and PR numbers.

    Derived and gitignored, so it may simply not exist yet on a fresh clone.
    """
    try:
        with open(sync.DATA_FILE, "r", encoding="utf-8") as f:
            return json.load(f)
    except FileNotFoundError:
        print("No data.json yet — building it first.")
        return sync.refresh()


def resolve(ledger, ref):
    """Full SHA for a short SHA or a `#1234` / `1234` PR reference."""
    ref = ref.strip()
    if ref.startswith("#") or (ref.isdigit() and len(ref) <= 6):
        pr = ref.lstrip("#")
        hits = [r for r in ledger if r["pr"] == pr]
    else:
        hits = [r for r in ledger if r["sha"].startswith(ref.lower())]

    if not hits:
        raise SystemExit(f"No tracked commit matches '{ref}'.")
    if len(hits) > 1:
        print(f"'{ref}' is ambiguous:", file=sys.stderr)
        for r in hits:
            print(f"  {r['sha'][:8]}  {r['commit']}", file=sys.stderr)
        raise SystemExit(1)
    return hits[0]


def read_verdicts():
    try:
        with open(sync.VERDICTS_FILE, "r", encoding="utf-8") as f:
            return json.load(f)
    except FileNotFoundError:
        return {}


def write_verdicts(verdicts):
    # Sorted so two people recording verdicts never produce a reordered diff.
    ordered = {sha: verdicts[sha] for sha in sorted(verdicts)}
    with open(sync.VERDICTS_FILE, "w", encoding="utf-8") as f:
        json.dump(ordered, f, indent=2, ensure_ascii=False)
        f.write("\n")


def list_pending(ledger):
    pending = [r for r in ledger if r["status"] == "Pending"]
    if not pending:
        print("Nothing pending — every tracked commit has a verdict.")
        return
    print(f"{len(pending)} commit(s) awaiting a verdict:\n")
    for r in pending:
        pr = f" #{r['pr']}" if r["pr"] else ""
        print(f"  {r['sha'][:8]}{pr:>7}  {r['date']}  {r['commit']}")
    print("\nMerge the sync and most of these file themselves as Merged.")


def main(argv):
    if not argv or argv[0] in ("-h", "--help"):
        print(__doc__)
        return 0
    if argv[0] == "--pending":
        list_pending(load_ledger())
        return 0

    ledger = load_ledger()
    row = resolve(ledger, argv[0])
    rest = argv[1:]
    verdicts = read_verdicts()
    entry = dict(verdicts.get(row["sha"], {}))

    if not rest:
        raise SystemExit("Say what the verdict is — a status, --note, or --clear.")

    if rest[0] == "--clear":
        verdicts.pop(row["sha"], None)
        entry = None
    elif rest[0] == "--note":
        if len(rest) < 2:
            raise SystemExit("--note needs the note text.")
        entry["notes"] = rest[1]
    else:
        status = rest[0].capitalize()
        if status not in STATUSES:
            raise SystemExit(f"Status must be one of {', '.join(STATUSES)}.")
        # Ancestry already says Merged; storing it again would be a claim the
        # file cannot keep true if the merge is ever undone.
        derived = "Merged" if row["sha"] in sync.ancestry_shas() else "Pending"
        if status == derived:
            entry.pop("status", None)
            # ASCII only: this prints to the Windows console (cp1252), where a
            # stray em dash raises UnicodeEncodeError and kills the command.
            print(f"'{status}' is what ancestry already implies - storing only the note.")
        else:
            entry["status"] = status
        if len(rest) > 1:
            entry["notes"] = rest[1]

    if entry is not None:
        if entry:
            verdicts[row["sha"]] = entry
        else:
            verdicts.pop(row["sha"], None)

    write_verdicts(verdicts)
    print(f"{row['sha'][:8]}  {row['commit']}")
    print(f"  -> {entry if entry else 'no override (derived from ancestry)'}")

    # Rebuild so the ledger (data.json) can never lag the verdict.
    print()
    sync.refresh()
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
