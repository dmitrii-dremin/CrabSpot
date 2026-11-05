"""Recompile Java 21 String programs and compare original and regenerated inputs in separate JVMs."""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile

from application_entry_reference import inventory, tool_digest
from fixture_source import read_java_source
from loading_constraint_reference import reference_tool, version
from string_constant_encodings import CASES, duplicate_literals, field_readers, literal_class
from type_state_reference import FIXTURES, ROOT, digest, serialize

DIRECTORY = FIXTURES / "string-constants"
FLAGS = ["--release", "21", "-g:none", "-encoding", "UTF-8"]
TOOLS = ("string_constant_reference.py", "string_constant_encodings.py", "type_state_encodings.py",
         "fixture_source.py", "type_state_reference.py", "loading_constraint_reference.py",
         "application_entry_reference.py")
ENTRY_ARGS = ["crab", "crab", "", "left right"]


def stage(root, files):
    for name, data in files.items():
        dest = root/name
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_bytes(data)


def archive(directory):
    base = directory/"classes"
    apps = {p.relative_to(base).as_posix(): p.read_bytes() for p in sorted(base.rglob("*.class"))}
    originals = json.loads((directory/"original-classes.json").read_bytes())
    if {name: digest(data) for name, data in apps.items()} != originals:
        raise ValueError("original String application inventory changed")
    base = directory/"encoded"
    raw = {p.relative_to(base).as_posix(): p.read_bytes() for p in sorted(base.rglob("*")) if p.is_file()}
    # The maximum modified-UTF8 constant alone takes 65,535 bytes. The whole
    # class is valid but exceeds the parser fuzzer's 64 KiB seed size limit.
    largest = "max-mutf/sample/Value.class"
    if largest in raw or largest+".bin" not in raw:
        raise ValueError("expected oversize String class in its original binary archive")
    raw[largest] = raw.pop(largest+".bin")
    return apps, raw


def compile_inputs(javac, source, output, raw):
    apps = output/"apps"
    stage(apps, field_readers())
    stage(apps, {"duplicates/Entry.class": duplicate_literals()})
    sources = sorted(source.glob("*/*.java"))
    subprocess.run([javac, *FLAGS, "-cp", os.pathsep.join(map(str, [apps, raw/"zero"])),
                    "-d", apps, *sources], check=True, timeout=90)
    for group in sorted((source/"controls").iterdir()):
        subprocess.run([javac, *FLAGS, "-cp", apps, "-d", output/"controls"/group.name,
                        *sorted(group.glob("*.java"))], check=True, timeout=60)


def programs(apps, raw, controls):
    for label, arguments in (("empty", []), ("normal", ENTRY_ARGS)):
        yield "direct/"+label, [], [apps], ["texts.Entry", *arguments]
        yield "entry/"+label, [], [controls/"entry", apps], ["See", *arguments]
    for label, entry in (("bare", "bare.Literal"), ("words", "words.Entry")):
        yield "direct/"+label, [], [apps], [entry]
    for label, mode, options in (("units", "words", []), ("bare", "bare", []), ("default", "checks", []),
                                 ("class-only", "checks", ["-ea:checks.Enabled"]), ("system", "checks", ["-esa"]),
                                 ("all", "checks", ["-ea", "-esa"])):
        yield "behavior/"+label, options, [controls/"behavior", apps], ["Observe", mode]
    for label, (units, _) in CASES.items():
        paths = [controls/"raw", raw/label, apps]
        yield "raw/"+label, [], paths, ["Observe"]
        pattern = units[:1] if len(set(units)) == 1 else units
        yield "effects/"+label, [], paths, ["RunValue", str(len(units)), *(format(u, "x") for u in pattern)]
    yield "intern/loaders", [], [controls/"loaders"], ["First", apps]
    yield "intern/unique", [], [controls/"loaders"], ["Identity"]
    yield "transport/units", [], [controls/"transport", apps], ["Transport", "units"]
    yield "fields/duplicates", [], [controls/"entry", apps], ["Duplicates"]
    for mode in ("constants", "fresh", "canonical"):
        yield "fields/"+mode, [], [controls/"entry", apps], ["Fields", mode]


def launch(java, options, paths, args):
    result = subprocess.run([java, *options, "-Xverify:all", "-cp", os.pathsep.join(map(str, paths)), *args],
                            capture_output=True, timeout=60)
    if result.returncode or result.stderr:
        raise ValueError(f"{args[0]} reference process exited {result.returncode}: {result.stderr!r}")
    return result.stdout.decode("utf-8").replace("\r\n", "\n").removesuffix("\n")


def signed_hash(units):
    value = 0
    for unit in units:
        value = (value * 31 + unit) & 0xffffffff
    return value if value < (1 << 31) else value - (1 << 32)


def check_transport(row, suffix):
    stats, *arguments = row.split("|")
    if len(arguments) != 5 or any(not re.fullmatch(r"(?:[0-9a-fA-F]{4})*", a) for a in arguments):
        raise ValueError(f"invalid native argument report: {row}")
    units = [[int(a[i:i+4], 16) for i in range(0, len(a), 4)] for a in arguments]
    if units[:4] != [[99, 114, 97, 98]]*2 + [[], []]:
        raise ValueError(f"native launcher changed ASCII argument boundaries: {row}")
    count, flags, unit_hash, order = map(int, stats.split("/"))
    if count != 5 or flags & 76 != 76 or flags & ~127 or signed_hash(map(len, units)) != order:
        raise ValueError(f"native argument counts or identity predicates differ: {row}")
    if unit_hash != signed_hash(u for value in units for u in value):
        raise ValueError(f"native argument hash differs from delivered units: {row}")
    if suffix == "a b" and units[-1] != [97, 32, 98]:
        raise ValueError(f"space inside argument lost: {row}")
    if suffix != "a b" and not units[-1]:
        raise ValueError(f"unexpected supplementary-character transport: {row}")
    return units


def matches_observation(label, row, expected, pinned):
    if pinned or label not in {"intern/loaders", "intern/unique"}:
        return row == expected
    actual_lines, expected_lines = row.splitlines(), expected.splitlines()
    if len(actual_lines) != len(expected_lines):
        return False
    for actual, baseline in zip(actual_lines, expected_lines):
        fields = actual.split("/")
        if fields[0] not in {"true", "false"}:
            return False
        start = 1
        if label == "intern/loaders":
            if len(fields) < 2 or fields[0] != fields[1]:
                return False
            start = 2
        if fields[start:] != baseline.split("/")[start:]:
            return False
    return True


def replay(vm, options, apps, raw, controls, expected, pinned):
    observed = {}
    for label, extra, paths, args in programs(apps, raw, controls):
        row = launch(vm, [*options, *extra], paths, args)
        if label not in expected or not matches_observation(label, row, expected[label], pinned):
            raise ValueError(f"{label}: expected {expected.get(label)!r}, got {row!r}")
        observed[label] = row
    if observed.keys() != expected.keys():
        raise ValueError("String observation inventory changed")
    native, delivered = {}, {}
    for key, suffix in (("spaces", "a b"), ("unicode", "\u20ac\U0001f680")):
        row = launch(vm, options, [controls/"argv", apps], ["NativeReport", "crab", "crab", "", "", suffix])
        delivered[key] = check_transport(row, suffix)
        native[key] = row
    return native, delivered


def check_manifest(current, original):
    variable = {"compiler", "primary", "compiled", "controls"}
    if current.keys() != original.keys():
        raise ValueError("String fixture manifest schema differs")
    for key in current.keys() - variable:
        if current[key] != original[key]:
            raise ValueError(f"String fixture provenance differs: {key}")
    same = all(current[key] == original[key] for key in ("primary", "compiler"))
    for key in ("compiled", "controls"):
        if current[key].keys() != original[key].keys() or same and current[key] != original[key]:
            raise ValueError(f"String compiler output differs: {key}")


def record(check, secondary):
    folder = DIRECTORY/"src"
    sources = {p.relative_to(folder).as_posix(): read_java_source(p) for p in sorted(folder.rglob("*.java"))}
    tools = {name: tool_digest(ROOT/"tools"/name) for name in TOOLS}
    observations = {v: (DIRECTORY/(v+".json")).read_bytes() for v in ("hotspot", "openj9")}
    windows = (DIRECTORY/"observed-windows-argv.json").read_bytes()
    archived, archived_raw = archive(DIRECTORY)
    raw = {label+"/sample/Value.class": literal_class(units, opcode) for label, (units, opcode) in CASES.items()}
    extra = field_readers() | {"duplicates/Entry.class": duplicate_literals()}
    if raw != archived_raw or any(archived.get(path) != data for path, data in extra.items()):
        raise ValueError("original raw String class differs from its encoding")
    java, javac = reference_tool("java"), reference_tool("javac")
    primary = version(java)
    historical_primary = json.loads((DIRECTORY/"manifest.json").read_bytes())["primary"] if check else primary
    if "Server VM" not in primary or "OpenJ9" in primary:
        raise ValueError("select a Java 21 HotSpot primary")
    pin = json.loads((FIXTURES/"loading-constraints/openj9/manifest.json").read_bytes())
    vendors = [("hotspot", java, [])]
    if secondary:
        if version(secondary) != pin["runtime"]:
            raise ValueError("unexpected OpenJ9 runtime")
        vendors.append(("openj9", secondary, ["-Xshareclasses:none"]))
    elif not check:
        raise ValueError("recording requires both reference JVMs")
    scratch = ROOT/"target/fixture-tool"
    scratch.mkdir(exist_ok=True, parents=True)
    with tempfile.TemporaryDirectory(dir=scratch) as tmp:
        work = Path(tmp)
        for name, files in (("src", sources), ("raw", raw), ("original-apps", archived), ("original-raw", archived_raw)):
            stage(work/name, files)
        compile_inputs(javac, work/"src", work, work/"raw")
        for label, vm, options in vendors:
            expected = json.loads(observations[label])
            native = None
            for kind, apps, encoded in (("original", "original-apps", "original-raw"), ("regenerated", "apps", "raw")):
                result, delivered = replay(vm, options, work/apps, work/encoded, work/"controls",
                                           expected, label == "openj9" or primary == historical_primary)
                if native is not None and delivered != native:
                    raise ValueError("original and regenerated native argument units differ")
                native = delivered
                print(f"{label} {kind}: {len(expected)} String observations, native argv {result}", flush=True)
        original = {name: digest(data) for name, data in archived.items()}
        manifest = {"primary": primary, "compiler": version(javac), "secondary": pin["runtime"],
                    "secondary_archive_sha256": pin["archive_sha256"], "flags": FLAGS, "tools": tools,
                    "sources": {name: digest(data) for name, data in sources.items()}, "original": original,
                    "encoded": {name: digest(data) for name, data in raw.items()}, "compiled": inventory(work/"apps"),
                    "controls": inventory(work/"controls"), "observations": {v: digest(b) for v, b in observations.items()},
                    "original_windows_argv": digest(windows)}
        if any(tool_digest(ROOT/"tools"/name) != sha for name, sha in tools.items()):
            raise ValueError("String reference tooling changed during replay")
        path = DIRECTORY/"manifest.json"
        if check:
            check_manifest(manifest, json.loads(path.read_bytes()))
        elif manifest["compiled"] != original:
            raise ValueError("recording requires the original application's class bytes")
        else:
            path.write_bytes(serialize(manifest))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--compare-openj9", type=Path)
    args = parser.parse_args()
    record(args.check, args.compare_openj9)
