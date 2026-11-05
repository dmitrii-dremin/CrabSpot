"""Run the original and regenerated Java 21 class-object probes in isolated JVMs."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile

from application_entry_reference import inventory, tool_digest
from class_mirror_encodings import ALLOCATIONS, CALLS, LITERALS, driver_encodings, encodings
from fixture_source import read_java_source
from loading_constraint_reference import reference_tool, version
from type_state_reference import FIXTURES, ROOT, digest, serialize

DIRECTORY = FIXTURES / "class-mirrors"
FLAGS = ("--release", "21", "-g:none", "-encoding", "UTF-8")
TOOLS = ("class_mirror_reference.py", "class_mirror_encodings.py", "type_state_encodings.py",
         "type_state_reference.py", "fixture_source.py", "application_entry_reference.py",
         "loading_constraint_reference.py")


def stage_bytes(root, files):
    for relative, data in files.items():
        target = root / relative
        target.parent.mkdir(exist_ok=True, parents=True)
        target.write_bytes(data)


def compile_inputs(javac, source, stage):
    apps = [p for folder in ("mirrors", "store", "simple") for p in sorted((source/folder).glob("*.java"))]
    for output, paths, classpath in (("apps", apps, []), ("support", sorted((source/"p").glob("*.java")), []),
                                    ("controls", sorted((source/"control").glob("*.java")), ["-cp", stage/"apps"])):
        subprocess.run([javac, *FLAGS, *classpath, "-d", stage/output, *paths], check=True, timeout=60)


def cases(apps, raw, controls, support):
    for key, entry, observer in (("entry", "mirrors.Entry", "Observe"), ("store", "store.Entry", "Watch"),
                                 ("plain", "simple.Plain", "See")):
        yield key+"/direct", [apps], [entry]
        yield key+"/observe", [controls, apps], [observer]
    yield "entry/loaders", [controls], ["Loaders", apps]
    for case, _, _ in LITERALS:
        key = "literal/"+case
        paths = [controls, raw/key, raw/"literal-support"]
        yield key, paths, ["ConstantObserver"]
        yield "effects/"+key, paths, ["EntryEffects"]
    # Reuse the exact [LI; classfile and omit the ordinary class I from its class path.
    paths = [controls, raw/"literal/arr-of-I"]
    yield "literal/missing-I", paths, ["ConstantObserver"]
    yield "effects/literal/missing-I", paths, ["EntryEffects"]
    for case in CALLS:
        key = "call/"+case
        paths = [controls, raw/key, support]
        yield key, paths, ["ConstantObserver"]
        yield "effects/"+key, paths, ["EntryEffects"]
    for case in ALLOCATIONS:
        key = "new/"+case
        yield key, [controls, raw/key], ["CheckNew"]
        yield "effects/"+key, [controls, raw/key], ["EntryEffects"]
    yield "sticky", [controls], ["Gate", raw/"sticky"]


def replay(java, options, programs, expected):
    observed = {}
    for case, paths, arguments in programs:
        result = subprocess.run([java, *options, "-Xverify:all", "-cp", os.pathsep.join(map(str, paths)), *arguments],
                                capture_output=True, timeout=45)
        if result.returncode or result.stderr:
            raise ValueError(f"{case}: reference JVM exited {result.returncode}, stderr={result.stderr!r}")
        observed[case] = result.stdout.decode("utf-8")
        if observed[case] != expected.get(case):
            raise ValueError(f"{case}: expected {expected.get(case)!r}, saw {observed[case]!r}")
    if observed.keys() != expected.keys():
        raise ValueError("class-mirror observation inventory changed")
    return len(observed)


def compare_manifest(current, stored):
    # A different Java 21 compiler may produce a different encoding of the same source.
    same_compiler = (current["compiler"], current["primary"]) == (stored["compiler"], stored["primary"])
    for key in ("compiled", "support", "controls"):
        if current[key].keys() != stored[key].keys() or same_compiler and current[key] != stored[key]:
            raise ValueError(f"class-mirror compiler output differs: {key}")
    variable = {"compiler", "primary", "compiled", "support", "controls"}
    if current.keys() != stored.keys():
        raise ValueError("class-mirror manifest schema differs")
    for key in current.keys() - variable:
        if current[key] != stored[key]:
            raise ValueError(f"class-mirror provenance differs: {key}")


def record(check, secondary):
    source = DIRECTORY / "src"
    sources = {p.relative_to(source).as_posix(): read_java_source(p) for p in sorted(source.rglob("*.java"))}
    tool_hashes = {name: tool_digest(ROOT/"tools"/name) for name in TOOLS}
    observations = {v: (DIRECTORY/(v+".json")).read_bytes() for v in ("hotspot", "openj9")}
    originals = json.loads((DIRECTORY/"original-classes.json").read_bytes())
    def capture_classes(folder):
        return {p.relative_to(folder).as_posix(): p.read_bytes() for p in sorted(folder.rglob("*.class"))}

    archived_apps = capture_classes(DIRECTORY/"classes")
    archived_support = capture_classes(DIRECTORY/"support")
    if {name: digest(data) for name, data in archived_apps.items()} != originals:
        raise ValueError("archived Java 21 application inventory changed")
    raw = encodings() | driver_encodings()
    hashes = {name: digest(data) for name, data in raw.items()}
    archived_raw = capture_classes(DIRECTORY/"encoded")
    if archived_raw != raw:
        raise ValueError("archived raw classfiles differ from the generated encodings")
    java, javac = reference_tool("java"), reference_tool("javac")
    primary = version(java)
    if "Server VM" not in primary or "OpenJ9" in primary:
        raise ValueError("select a Java 21 HotSpot primary")
    pin = json.loads((FIXTURES/"loading-constraints/openj9/manifest.json").read_bytes())
    vendors = [("hotspot", java, [])]
    if secondary:
        if version(secondary) != pin["runtime"]:
            raise ValueError("OpenJ9 differs from the pinned runtime")
        vendors.append(("openj9", secondary, ["-Xshareclasses:none"]))
    elif not check:
        raise ValueError("recording requires both reference JVMs")
    scratch = ROOT/"target/fixture-tool"
    scratch.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=scratch) as tmp:
        stage = Path(tmp)
        stage_bytes(stage/"src", sources)
        stage_bytes(stage/"encoded", raw)
        stage_bytes(stage/"original-apps", archived_apps)
        stage_bytes(stage/"original-raw", archived_raw)
        stage_bytes(stage/"original-support", archived_support)
        compile_inputs(javac, stage/"src", stage)
        for vendor, vm, options in vendors:
            expected = json.loads(observations[vendor])
            for label, apps, raw_dir, support in (
                ("original", stage/"original-apps", stage/"original-raw", stage/"original-support"),
                ("regenerated", stage/"apps", stage/"encoded", stage/"support"),
            ):
                programs = cases(apps, raw_dir, stage/"controls", support)
                count = replay(vm, options, programs, expected)
                print(f"{vendor} {label}: {count} class-object observations reproduced", flush=True)
        manifest = {
            "primary": primary, "compiler": version(javac), "secondary": pin["runtime"],
            "secondary_archive_sha256": pin["archive_sha256"], "flags": list(FLAGS),
            "sources": {name: digest(data) for name, data in sources.items()},
            "tools": tool_hashes,
            "raw": hashes, "original": originals, "compiled": inventory(stage/"apps"),
            "original_support": {name: digest(data) for name, data in archived_support.items()},
            "support": inventory(stage/"support"), "controls": inventory(stage/"controls"),
            "observations": {v: digest(data) for v, data in observations.items()},
        }
        if any(tool_digest(ROOT/"tools"/name) != sha for name, sha in tool_hashes.items()):
            raise ValueError("reference tool changed during replay")
        output = DIRECTORY/"manifest.json"
        if check:
            compare_manifest(manifest, json.loads(output.read_bytes()))
        elif manifest["compiled"] != originals or manifest["support"] != manifest["original_support"]:
            raise ValueError("recording requires the original application class encodings")
        else:
            output.write_bytes(serialize(manifest))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--compare-openj9", type=Path)
    arguments = parser.parse_args()
    record(arguments.check, arguments.compare_openj9)
