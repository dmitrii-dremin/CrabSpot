"""Launch original application directories on CrabSpot using this JDK's real java.base bytes."""
import argparse
import json
import os
from pathlib import Path
import re
import struct
import subprocess
import tempfile

from application_entry_reference import CASES, DIRECTORY, inventory, stage_cases
from type_state_reference import ROOT
from verify_java_base import MAX_CLASSES, MAX_CLASS_BYTES, MAX_TOTAL_BYTES, pack_archive

DEVICES = {"CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"}
DEVICES.update(prefix + n for prefix in ("COM", "LPT") for n in "123456789¹²³")


def read_exact(stream, size):
    data = stream.read(size)
    if len(data) != size:
        raise ValueError("truncated packed platform")
    return data


def extract_classes(packed, directory):
    directory.mkdir()
    names = set()
    total = 0
    with packed.open("rb") as stream:
        count, = struct.unpack(">I", read_exact(stream, 4))
        if not 1 < count <= MAX_CLASSES:
            raise ValueError("unsupported platform class count")
        for _ in range(count):
            name_length, size = struct.unpack(">HI", read_exact(stream, 6))
            total += size
            if not 0 < name_length <= 4096 or not 0 < size <= MAX_CLASS_BYTES or total > MAX_TOTAL_BYTES:
                raise ValueError("platform entry length exceeds policy")
            name = read_exact(stream, name_length).decode("utf-8")
            parts = name.split("/")
            for part in parts:
                if (not part or part.endswith((" ", ".")) or any(c in '.:;[\\*?"<>|' or ord(c) < 32 for c in part)
                        or part.upper() in DEVICES):
                    raise ValueError(f"nonportable platform class path {name!r}")
            key = name.casefold()
            if key in names:
                raise ValueError(f"duplicate platform file {name}")
            names.add(key)
            path = directory.joinpath(*parts[:-1], parts[-1] + ".class")
            path.parent.mkdir(parents=True, exist_ok=True)
            data = read_exact(stream, size)
            with path.open("xb") as output:
                output.write(data)
        if stream.read(1) or "module-info" not in names:
            raise ValueError("incomplete platform archive or trailing input")
    return count


EXPECTED = {
    "ordinary": "returned 1/1/37/1/0",
    "abrupt": "MainEvaluation NullPointerException 1/0/0/0/0",
    "invalid": "image Verification",
    "no-main": "image Entry",
    "missing": "image Dependencies",
    "unused-invalid": "image Verification",
}
NAMED = {
    "Child": "returned 12/12", "Hidden": "returned 123/123", "Face": "returned 0/7",
    "Parent": "MainEvaluation NullPointerException 1/1",
    "Broken": "NamedInitialization NullPointerException 14/0",
    "Private": "image Entry", "Wide": "image Entry", "Nonstatic": "image Entry", "Impl": "image Entry",
}

# Bytecode offsets in the archived javac inputs, outermost call site first.
FAILURE_OFFSETS = {"app.Abrupt": [9], "named.Parent": [20], "named.Broken": [4, 1]}


def check_execution(bin_dir, platform, classes, name, expected):
    extension = ".exe" if os.name == "nt" else ""
    cli = bin_dir / ("crabspot" + extension)
    probe = bin_dir / "examples" / ("replay_application_entry" + extension)
    result = subprocess.run([cli, "run", "--platform", platform, "--classes", classes, name],
                            capture_output=True, timeout=180)
    success = expected.startswith("returned ")
    if result.returncode != (0 if success else 1) or result.stdout or (success and result.stderr) or (not success and not result.stderr):
        raise ValueError(f"{name}: unexpected CLI result {result.returncode}, {result.stdout!r}, {result.stderr!r}")
    if not success:
        check_failure_diagnostic(name, expected, result.stderr.decode("utf-8"))
    result = subprocess.run([probe, platform, classes, name], capture_output=True, timeout=180)
    observation = result.stdout.decode("utf-8").strip()
    if result.returncode != 0 or observation != expected:
        raise ValueError(f"{name}: expected {expected!r}, got {observation!r}; {result.stderr!r}")
    print(f"{name}  {observation}", flush=True)


def check_failure_diagnostic(name, expected, diagnostic):
    kind, detail, *_ = expected.split()
    stage = detail if kind == "image" else kind
    # Compare the CLI's own stage and, for runtime failures, its cause and bytecode location.
    first = diagnostic.removeprefix("crabspot: ").split(maxsplit=1)
    if not first or first[0].rstrip(":") != stage:
        raise ValueError(f"expected CLI failure at {stage}, got {diagnostic!r}")
    if kind != "image":
        offsets = [int(pc) for pc in re.findall(r"\bbytecode offset (\d+):", diagnostic)]
        if detail not in diagnostic or offsets != FAILURE_OFFSETS[name]:
            raise ValueError(f"{name}: expected {detail} at {FAILURE_OFFSETS[name]}, got {diagnostic!r}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--jdk", type=Path, default=os.environ.get("JAVA_HOME"))
    parser.add_argument("--platform", type=Path, help="use an already extracted genuine java.base classes directory")
    args = parser.parse_args()
    if not args.platform and not args.jdk:
        parser.error("set JAVA_HOME, pass --jdk, or supply --platform")
    subprocess.run(["cargo", "build", "--release", "--locked", "-p", "crabspot-launcher", "--bin", "crabspot",
                    "--example", "replay_application_entry"], cwd=ROOT, check=True)
    bin_dir = (ROOT / os.environ.get("CARGO_TARGET_DIR", "target") / "release").resolve()
    scratch = ROOT / "target/fixture-tool"
    scratch.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=scratch) as temp:
        work = Path(temp)
        platform = args.platform
        if platform is None:
            packed, platform = work / "platform.bin", work / "platform"
            sha, count, _ = pack_archive(args.jdk / "jmods/java.base.jmod", packed)
            if extract_classes(packed, platform) != count + 1:
                raise ValueError("extracted definition count differs from the source archive")
            print(f"java.base.jmod {sha}, {count} ordinary classes", flush=True)
        classes = DIRECTORY / "classes"
        if inventory(classes) != json.loads((DIRECTORY / "original-classes.json").read_bytes()):
            raise ValueError("archived application input hash mismatch")
        paths = stage_cases(work, classes)
        for case, expected in EXPECTED.items():
            check_execution(bin_dir, platform, paths[case], "app." + CASES[case][0], expected)
        for name, expected in NAMED.items():
            check_execution(bin_dir, platform, paths["named-effects"], "named." + name, expected)


if __name__ == "__main__":
    main()
