"""Exercise the archived class-object applications via the real CLI and public Rust API."""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile

from class_mirror_applications import stage_applications
from class_mirror_reference import DIRECTORY, ROOT
from type_state_encodings import ClassEncoding, op
from type_state_reference import digest
from verify_application_entry import extract_classes
from verify_java_base import pack_archive


# Image building eagerly verifies all application classes. Missing imports and
# verification errors have no invocation, retained Java effects or evaluation PC.
IMAGE = {
    "literal/missing": ("Dependencies", "MissingApplicationDependency", "2"),
    "literal/missing-array": ("Dependencies", "MissingApplicationDependency", "262"),
    "literal/missing-I": ("Dependencies", "MissingApplicationDependency", "14"),
    "literal/closure-only": ("Dependencies", "UnsupportedPlatformReference", "14"),
    "literal/wrong-category": ("Verification", "Verification", "-"),
    "call/unfinished": ("Verification", "Verification", "-"),
    "call/visible-array": ("Verification", "Verification", "-"),
}
VERIFIER_REASON = {
    "literal/wrong-category": ("Instruction(0)", "category-two loadable constant"),
    "call/unfinished": ("Instruction(3)", "receiver is unfinished"),
    "call/visible-array": ("Instruction(3)", "verification types are not assignable"),
}
EXECUTION = {
    "literal/inaccessible-array": "IllegalAccessError run@0 via main@4",
    "literal/private-package": "IllegalAccessError run@0 via main@4",
    "call/null": "NullPointerException run@1 via run@1",
    "call/static-mismatch": "IncompatibleClassChangeError run@0 via run@0",
    "call/nonimplementor": "IncompatibleClassChangeError run@7 via run@7",
    "call/through-object": "IncompatibleClassChangeError helper@1 via helper@1",
    "call/native-overload": "UnboundNative getClass@entry via run@8",
    "new/abandon": "ReservedClassObject run@5 via main@4",
    "new/constructor": "ReservedClassObject run@5 via main@4",
    "new/unfinished-lock": "ReservedClassObject run@5 via main@4",
    "platform/passive-name": "OpaqueClassObject main@7 via main@7",
    "platform/active-forname": "initializing UnboundNative registerNatives@entry via <clinit>@0",
}
FIELDS = {"mirrors": "127/3/1", "store": "63/0", "simple": "1"}
MAX = (1 << 64) - 1
LIMITS = (
    ("simple", (1, 1, MAX, MAX), "returned fields=1"),
    ("simple", (0, 1, MAX, MAX), "MainEvaluation Mirrors 0/1 main@0 fields=0"),
    ("simple", (1, 0, MAX, MAX), "MainEvaluation ClassSites 0/1 main@0 fields=0"),
    ("simple", (MAX, MAX, MAX, MAX), "returned fields=1"),
    ("mirrors", (8, 9, 1, 3), "returned fields=127/3/1"),
    ("mirrors", (7, 9, 1, 3), "MainEvaluation Mirrors 7/8 main@115 via main@115 fields=15/0/0"),
    ("mirrors", (8, 8, 1, 3), "MainEvaluation ClassSites 8/9 run@0 via main@170 fields=63/3/0"),
    ("mirrors", (8, 9, 1, 2), "MainEvaluation Recursion 2/3 run@4 via main@170 fields=63/3/0"),
)


def expected_effects(case, primary, secondary):
    if case in IMAGE:
        stage, kind, site = IMAGE[case]
        return f"image {stage} {kind} cp={site}"
    if case in EXECUTION:
        return f"MainEvaluation {EXECUTION[case]} fields=1"
    if case in FIELDS:
        return "returned fields="+FIELDS[case]
    if primary["effects/"+case] != "returned 2" or secondary["effects/"+case] != "returned 2":
        raise ValueError(f"missing CrabSpot disposition for {case}")
    return "returned fields=2"


def platform_cases(work):
    cases = {}
    for case, sha in (("passive-name", "3629e756cef210c67777cffc92c9e602a1da1e7da49c5c5c3f66b1ebc19f76a3"),
                      ("active-forname", "7276757bcefc66789b506dcfeee6082e895231adb4fafeae1891d1f6c6681ba4")):
        caller = ClassEncoding("probe/Entry")
        phase = op(0xb3, caller.ref(9, caller.name, "phase", "I"))
        if case == "passive-name":
            body = op(0x13, caller.cls(caller.name)) + op(0xb6, caller.ref(10, "java/lang/Class", "getName", "()Ljava/lang/String;"))
        else:
            body = b"\x01" + op(0xb8, caller.ref(10, "java/lang/Class", "forName", "(Ljava/lang/String;)Ljava/lang/Class;"))
        code = b"\x04" + phase + body + b"\x57\x05" + phase + b"\xb1"
        blob = caller.finish([(9, "phase", "I")], [(9, "main", "([Ljava/lang/String;)V", code, 1, 1)])
        if digest(blob) != sha:
            raise ValueError(f"changed platform control: {case}")
        folder = work/"platform-cases"/case
        (folder/"probe").mkdir(parents=True)
        (folder/"probe/Entry.class").write_bytes(blob)
        cases["platform/"+case] = (folder, "probe.Entry")
    return cases


def check_cli(case, result):
    if result.stdout:
        raise ValueError(f"{case}: unexpected CLI stdout {result.stdout!r}")
    if case not in IMAGE and case not in EXECUTION:
        if result.returncode or result.stderr:
            raise ValueError(f"{case}: expected a normal launch, got {result}")
        return
    diagnostic = result.stderr.decode()
    if result.returncode != 1:
        raise ValueError(f"{case}: expected exit 1, got {result}")
    if case in IMAGE:
        stage, kind, site = IMAGE[case]
        tokens = VERIFIER_REASON.get(case, (f"Some({site}): {kind}",))
        if not diagnostic.startswith(f"crabspot: {stage} ") or any(s not in diagnostic for s in tokens):
            raise ValueError(f"{case}: unexpected image error {diagnostic}")
        return
    expected = EXECUTION[case]
    category = expected.removeprefix("initializing ").split()[0]
    text = {"ReservedClassObject": "ordinary allocation of VM Class", "UnboundNative": "native methods require runtime services",
            "OpaqueClassObject": "has opaque VM state"}
    offsets = re.findall(r"bytecode offset (\d+):", diagnostic)
    if (not diagnostic.startswith("crabspot: MainEvaluation:") or text.get(category, category) not in diagnostic
            or offsets != re.findall(r"@(\d+)", expected)[::-1]):
        raise ValueError(f"{case}: unexpected CLI failure {diagnostic}")


def check_api(command, expected):
    result = subprocess.run(command, capture_output=True, timeout=180)
    row = result.stdout.decode().strip()
    if result.returncode or result.stderr:
        raise ValueError(f"{command}: unexpected observer failure {result}")
    if expected.startswith("image "):
        if row != expected:
            raise ValueError(f"expected {expected!r}, got {row!r}")
        return None
    fields, separator, count = row.rpartition(" proved=")
    if not separator or fields != expected or not count.isascii() or not count.isdecimal():
        raise ValueError(f"expected {expected!r} and proof count, got {row!r}")
    return int(count)


def run_cases(bin_dir, platform, work):
    extension = ".exe" if os.name == "nt" else ""
    cli = bin_dir/("crabspot"+extension)
    replay = bin_dir/"examples"/("replay_class_mirror_entry"+extension)
    primary, secondary = [json.loads((DIRECTORY/(v+".json")).read_bytes()) for v in ("hotspot", "openj9")]
    cases = stage_applications(work/"applications")
    cases.update(platform_cases(work))
    closure = None
    counts = {}
    for case, (folder, main) in cases.items():
        expected = expected_effects(case, primary, secondary)
        result = subprocess.run([cli, "run", "--platform", platform, "--classes", folder, main],
                                capture_output=True, timeout=180)
        check_cli(case, result)
        proved = check_api([replay, platform, folder, main], expected)
        if proved is not None:
            platform_count = proved - len(list(folder.rglob("*.class")))
            if platform_count < 7 or closure is not None and platform_count != closure:
                raise ValueError(f"{case}: inconsistent number of proved platform classes: {platform_count}")
            closure = platform_count
            counts[case] = proved
        print(f"{case}  {expected}  proved={proved}", flush=True)
    for case, limits, expected in LIMITS:
        folder, main = cases[case]
        proved = check_api([replay, platform, folder, main, *map(str, limits)], expected)
        if proved != counts[case]:
            raise ValueError(f"{case}: heap policy changed image proof count")
        print(f"{case} limits={limits}  {expected}", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--platform", type=Path)
    parser.add_argument("--jdk", type=Path, default=os.environ.get("JAVA_HOME"))
    args = parser.parse_args()
    if not args.platform and not args.jdk:
        parser.error("set JAVA_HOME, pass --jdk, or supply --platform")
    subprocess.run(["cargo", "build", "--locked", "--release", "-p", "crabspot-launcher", "--bin", "crabspot",
                    "--example", "replay_class_mirror_entry"], cwd=ROOT, check=True)
    bin_dir = (ROOT/os.environ.get("CARGO_TARGET_DIR", "target")/"release").resolve()
    scratch = ROOT/"target/fixture-tool"
    scratch.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=scratch) as tmp:
        work = Path(tmp)
        platform = args.platform
        if platform is None:
            packed, platform = work/"platform.bin", work/"platform"
            sha, count, _ = pack_archive(args.jdk/"jmods/java.base.jmod", packed)
            if extract_classes(packed, platform) != count+1:
                raise ValueError("incorrect extracted platform class count")
            print(f"java.base.jmod {sha}, {count} ordinary classes", flush=True)
        run_cases(bin_dir, platform, work)


if __name__ == "__main__":
    main()
