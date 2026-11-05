"""Run synchronized and raw monitor applications through the physical CLI and public launcher."""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile

from application_entry_reference import inventory
from monitor_reference import DIRECTORY, ENTRY_CASES
from type_state_reference import ROOT
from verify_application_entry import extract_classes
from verify_java_base import pack_archive

# The physical PCs come from javap on the archived classes. Each result records retained Java fields.
NPE = "MainEvaluation NullPointerException"
IMSE = "MainEvaluation IllegalMonitorStateException"
EXPECTED = {
    "Entry": "returned 1/2/1/9",
    "Null": f"{NPE} main@11 1/0/0/0",
    "balanced": "returned 3",
    "crossed": "returned 3",
    "recursive": "returned 3",
    "unfinished": "returned 3",
    "null-enter": f"{NPE} main@14 1",
    "null-exit": f"{NPE} main@14 1",
    "unowned": f"{IMSE} main@14 1",
    "wrong-object": f"{IMSE} main@21 2",
    # Both optional structured-locking rules apply to each invocation in CrabSpot.
    "leak": f"{IMSE} main@20 3",
    "callee-exit": f"{IMSE} steal@1 via main@16 1",
    "synchronized-instance-exit": f"{IMSE} drop@1 via main@20 1",
}

# Entry acquires/releases its own implicit lock and two explicit outer locks (6 steps).
# Each of two calls to Base.call uses the remaining 20 steps for synchronized calls and blocks.
STEP_BOUNDS = (
    ("Entry", 46, "returned 1/2/1/9"),
    ("Entry", 45, "MainEvaluation Steps 45/46 main@112 1/2/1/9"),
    ("Entry", 0, "MainEvaluation Steps 0/1 main@root 0/0/0/0"),
    ("Null", 0, f"{NPE} main@11 1/0/0/0"),
)


def check_cli(expected, result):
    if expected.startswith("returned "):
        if (result.returncode, result.stdout, result.stderr) != (0, b"", b""):
            raise ValueError(f"unexpected normal launch result: {result}")
        return
    phase, cause, *rest = expected.split()
    diag = result.stderr.decode("utf-8")
    if result.returncode != 1 or result.stdout or not diag.startswith(f"crabspot: {phase}:"):
        raise ValueError(f"unexpected abrupt launch result: {result}")
    # The diagnostic writes the caller's invoke site first and the failing method's PC last.
    offsets = re.findall(r"\bbytecode offset (\d+):", diag)
    locations = [part.split("@")[1] for part in rest if "@" in part]
    if offsets != locations[::-1] or cause not in diag:
        raise ValueError(f"expected {cause} at {locations}, got {diag!r}")


def run_cases(bin_dir, platform):
    extension = ".exe" if os.name == "nt" else ""
    cli = bin_dir / ("crabspot"+extension)
    replay = bin_dir / "examples" / ("replay_monitor_entry"+extension)
    check_inputs(DIRECTORY)
    for case in ("Entry", "Null", *ENTRY_CASES):
        if case in ("Entry", "Null"):
            folder, name = DIRECTORY/"classes", "locks."+case
        else:
            folder, name = DIRECTORY/"encoded"/("entry-"+case), "locks.RawEntry"
        expected = EXPECTED[case]
        command = [cli, "run", "--platform", platform, "--classes", folder, name]
        check_cli(expected, subprocess.run(command, capture_output=True, timeout=180))
        check_effects([replay, platform, folder, name], expected)
        print(f"{name} ({case})  {expected}", flush=True)
    for case, steps, expected in STEP_BOUNDS:
        check_effects([replay, platform, DIRECTORY/"classes", "locks."+case, str(steps)], expected)
        print(f"locks.{case} (steps={steps})  {expected}", flush=True)


def check_inputs(directory):
    manifest = json.loads((directory/"manifest.json").read_bytes())
    if inventory(directory/"classes") != manifest["original"]:
        raise ValueError("original monitor application input changed")
    for case in ENTRY_CASES:
        folder = "entry-"+case
        name = "locks/RawEntry.class"
        if inventory(directory/"encoded"/folder) != {name:manifest["raw"][folder+"/"+name]}:
            raise ValueError(f"raw monitor application input changed: {case}")


def check_effects(command, expected):
    output = subprocess.run(command, capture_output=True, timeout=180)
    if output.returncode or output.stderr or output.stdout.decode("utf-8").strip() != expected:
        raise ValueError(f"{command}: expected {expected!r}, got {output}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--platform", type=Path, help="already extracted Java 21 java.base classes")
    parser.add_argument("--jdk", type=Path, default=os.environ.get("JAVA_HOME"))
    args = parser.parse_args()
    if not args.platform and not args.jdk:
        parser.error("set JAVA_HOME, pass --jdk, or supply --platform")
    subprocess.run(["cargo", "build", "--locked", "--release", "-p", "crabspot-launcher", "--bin", "crabspot",
                    "--example", "replay_monitor_entry"], cwd=ROOT, check=True)
    bin_dir = (ROOT/os.environ.get("CARGO_TARGET_DIR", "target")/"release").resolve()
    if args.platform:
        run_cases(bin_dir, args.platform)
        return
    scratch = ROOT/"target/fixture-tool"
    scratch.mkdir(exist_ok=True, parents=True)
    with tempfile.TemporaryDirectory(dir=scratch) as tmp:
        stage = Path(tmp)
        packed, platform = stage/"platform.bin", stage/"platform"
        sha, count, _ = pack_archive(args.jdk/"jmods/java.base.jmod", packed)
        if extract_classes(packed, platform) != count+1:
            raise ValueError("incorrect extracted class count")
        print(f"java.base.jmod {sha}, {count} ordinary definitions", flush=True)
        run_cases(bin_dir, platform)


if __name__ == "__main__":
    main()
