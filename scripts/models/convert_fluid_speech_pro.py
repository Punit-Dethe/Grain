"""Convert FluidVoice's Fluid Speech Pro (.nemo) into a transcribe.cpp GGUF.

Fluid Speech Pro is a weights-only fine-tune of Parakeet TDT 0.6B v3 plus one
small extra block between the encoder's subsampling and its first Conformer
layer. transcribe.cpp has no slot for that block, so it is dropped, exactly as
FluidVoice does for its own GGUF ("plain Parakeet TDT 0.6B v3 layout").

Everything else is converted by the paired transcribe.cpp fork's own
`scripts/convert-parakeet.py`, imported unchanged: this script only registers a
variant profile and the block's tensor prefix, so neither the fork nor Handy's
catalog diverges. The profile keeps `stt.variant = tdt-0.6b-v3` because the
runtime layout is v3's; that is also what the fork's buffered-TDT (Flow) gate
checks.

Usage (from any directory):
    py -3.11 scripts/models/convert_fluid_speech_pro.py \
        --fork C:/Projects/Grain/transcribe-nemotron \
        --nemo C:/Projects/Grain/Reference/fluid-speech-pro/source/fluid-speech-pro.nemo \
        --out-dir C:/Projects/Grain/Reference/fluid-speech-pro/out

Writes `<out-dir>/fluid-speech-pro/fluid-speech-pro-F32.gguf`; quantize it with
the fork's `transcribe-quantize` (Q8_0 is the shipped quant).
"""

from __future__ import annotations

import argparse
import importlib.util
import sys
from pathlib import Path

SLUG = "fluid-speech-pro"

# State-dict prefixes with no transcribe.cpp slot. Anything else the converter
# does not map still fails loudly.
#   encoder.fv_insert.  the small pre-Conformer block (ln, down, depthwise mix, up)
#   clean_mode.         the optional rank-32 adapters behind "clean mode"
EXTRA_BLOCK_PREFIXES: tuple[str, ...] = ("encoder.fv_insert.", "clean_mode.")


def load_converter(fork: Path):
    scripts = fork / "scripts"
    sys.path.insert(0, str(scripts))
    spec = importlib.util.spec_from_file_location("convert_parakeet", scripts / "convert-parakeet.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def register(cp, extra_prefixes: tuple[str, ...]) -> None:
    v3 = cp.VARIANT_PROFILES["parakeet-tdt-0.6b-v3"]
    cp.VARIANT_PROFILES[SLUG] = {
        **v3,
        "display_name": "Fluid Speech Pro",
        "author": "FluidVoice",
        "organization": "FluidVoice-Labs",
        "prefer_direct_load": True,
    }
    if extra_prefixes:
        cp.EXPECTED_UNUSED_PREFIXES_BASE = tuple(cp.EXPECTED_UNUSED_PREFIXES_BASE) + extra_prefixes


def main(argv: list[str]) -> int:
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument("--fork", type=Path, required=True, help="transcribe.cpp fork checkout")
    p.add_argument("--nemo", type=Path, required=True, help="fluid-speech-pro.nemo")
    p.add_argument("--out-dir", type=Path, required=True)
    p.add_argument(
        "--drop-prefix",
        action="append",
        default=[],
        help="extra state-dict prefix to drop (the pre-Conformer block)",
    )
    args = p.parse_args(argv)

    cp = load_converter(args.fork)
    register(cp, EXTRA_BLOCK_PREFIXES + tuple(args.drop_prefix))
    out = args.out_dir / SLUG / cp.gguf_name(SLUG, cp.REFERENCE_DTYPE_LABEL)
    out.parent.mkdir(parents=True, exist_ok=True)
    cp.convert(str(args.nemo), out, repo_id=f"FluidVoice-Labs/{SLUG}")
    print(f"wrote {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
