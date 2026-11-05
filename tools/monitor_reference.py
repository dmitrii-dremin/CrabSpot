"""Replay the archived and regenerated Java 21 monitor programs in isolated reference JVMs."""
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
from monitor_encodings import ENTRY_CASES, RAW_CASES, RETURNS, entry_class, flag_case, raw_case, return_case
from type_state_reference import FIXTURES, ROOT, digest, serialize

DIRECTORY = FIXTURES / "monitors"
RETURN_CASES = [(shape, mode) for shape in RETURNS for mode in ("balanced", "leak")] + [
    ("int", "catch-return"), ("int", "catch-rethrow")]
FLAG_CASES = [("ordinary", 8, 1), ("sync-clinit", 0x28, 1), ("sync-init", 8, 0x21)]


def encodings():
    files = {f"{case}/locks/Raw.class": raw_case(case) for case in RAW_CASES}
    for shape, mode in RETURN_CASES:
        files[f"returns-{shape}-{mode}/locks/Returns.class"] = return_case(shape, mode)
    for case, flags, ctor in FLAG_CASES:
        files[f"flags-{case}/locks/Flags.class"] = flag_case(flags, ctor)
    for case in ENTRY_CASES:
        files[f"entry-{case}/locks/RawEntry.class"] = entry_class(case)
    return files


def archived_encodings():
    # The parser fuzzer takes valid .class seeds. Store the deliberately invalid constructor
    # alongside the corpus in its original bytes and restore its .class name for JVM loading.
    folder = DIRECTORY/"encoded"
    result = {p.relative_to(folder).as_posix():p.read_bytes() for p in folder.rglob("*.class")}
    invalid = "flags-sync-init/locks/Flags.class"
    if invalid in result:
        raise ValueError("parser-negative fixture has a valid-seed suffix")
    result[invalid] = (folder/(invalid+".bin")).read_bytes()
    return result


def stage_encodings(root, files):
    for name, data in files.items():
        dest = root/name
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_bytes(data)


def launch(java, options, paths, *args):
    result = subprocess.run([java, *options, "-Xverify:all", "-cp", os.pathsep.join(map(str, paths)), *args],
                            capture_output=True, timeout=45)
    return [result.returncode, result.stdout.decode("utf-8").strip(),
            result.stderr.decode("utf-8").replace("\r\n", "\n").strip()]


def replay(java, options, apps, controls, owners, encoded):
    rows = {}
    for case in RAW_CASES:
        rows["raw/"+case] = launch(java, options, [controls, encoded/case], "Observer", case)
    for shape, mode in RETURN_CASES:
        key = f"{shape}-{mode}"
        rows["returns/"+key] = launch(java, options, [controls, encoded/("returns-"+key)], "ReturnProbe")
    for case, _, _ in FLAG_CASES:
        rows["flags/"+case] = launch(java, options, [controls, encoded/("flags-"+case)], "Flags")
    rows["loader"] = launch(java, options, [controls], "LoaderMonitors", owners)
    for entry in ("Entry", "Null"):
        rows["launch/"+entry] = launch(java, options, [apps], "locks."+entry)
        rows["observe/"+entry] = launch(java, options, [controls, apps], "Observe", entry)
    rows["identities"] = launch(java, options, [controls], "Control")
    for case in ENTRY_CASES:
        folder = encoded / ("entry-"+case)
        rows["direct/"+case] = launch(java, options, [folder], "locks.RawEntry")
        rows["effects/"+case] = launch(java, options, [controls, folder], "EntryObserver")
    return rows


def compare(rows, reference):
    if rows.keys() != reference.keys():
        raise ValueError("monitor case inventory changed")
    for key, (code, out, err) in rows.items():
        expected_code, expected_out, expected_err = reference[key]
        if code != expected_code or out != expected_out:
            raise ValueError(f"{key}: expected {reference[key]!r}, got {rows[key]!r}")
        if err == expected_err:
            continue
        # Vendor wording and line numbers of a directly launched, uncaught Java exception may differ.
        direct_failure = code == 1 and key.startswith(("launch/", "direct/"))
        pattern = r'^Exception in thread "main" (java\.lang\.(?:NullPointerException|IllegalMonitorStateException))\b'
        actual = re.match(pattern, err)
        expected = re.match(pattern, expected_err)
        if not (direct_failure and actual and expected and actual[1] == expected[1]):
            raise ValueError(f"{key}: unexpected diagnostic {err!r}")


def compile_inputs(javac, source, stage):
    flags = ["--release", "21", "-g:none", "-encoding", "UTF-8"]
    for folder, sources, cp in [("apps", source.glob("locks/*.java"), []),
                                ("owners", source.glob("own/*.java"), []),
                                ("control", source.glob("*.java"), ["-cp", stage/"apps"])]:
        subprocess.run([javac, *flags, *cp, "-d", stage/folder, *sorted(sources)], check=True, timeout=60)
    return flags


def check_manifest(current, stored):
    keys = {"secondary", "secondary_archive_sha256", "flags", "sources", "tools", "raw", "original", "observations"}
    for group in ("compiled", "control", "owners"):
        if current[group].keys() != stored[group].keys():
            raise ValueError(f"regenerated monitor class inventory differs: {group}")
    if current["compiler"] == stored["compiler"] and current["primary"] == stored["primary"]:
        keys.update(("compiled", "control", "owners"))
    for key in sorted(keys):
        if current[key] != stored[key]:
            raise ValueError(f"monitor provenance differs: {key}")


def record(check, secondary):
    source = DIRECTORY/"src"
    sources = {p.relative_to(source): read_java_source(p) for p in sorted(source.rglob("*.java"))}
    java, javac = reference_tool("java"), reference_tool("javac")
    primary = version(java)
    if "OpenJ9" in primary or "Server VM" not in primary:
        raise ValueError("select a Java 21 HotSpot primary reference")
    pin = json.loads((FIXTURES/"loading-constraints/openj9/manifest.json").read_bytes())
    if secondary and version(secondary) != pin["runtime"]:
        raise ValueError("OpenJ9 runtime differs from the pinned build")
    vendors = [("hotspot", java, [])]
    if secondary:
        vendors.append(("openj9", secondary, ["-Xshareclasses:none"]))
    artifacts = encodings()
    raw_hashes = {name: digest(data) for name, data in artifacts.items()}
    original_encodings = archived_encodings()
    if original_encodings != artifacts:
        raise ValueError("archived synthetic class differs from the original encoding")
    originals = json.loads((DIRECTORY/"original-classes.json").read_bytes())
    if inventory(DIRECTORY/"classes") != originals:
        raise ValueError("archived javac input differs from the original class")
    scratch = ROOT/"target/fixture-tool"
    scratch.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=scratch) as tmp:
        stage = Path(tmp)
        staged_source = stage/"src"
        for relative, data in sources.items():
            dest = staged_source/relative
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_bytes(data)
        flags = compile_inputs(javac, staged_source, stage)
        stage_encodings(stage/"encoded", artifacts)
        stage_encodings(stage/"archived", original_encodings)
        for vendor, vm, options in vendors:
            reference = json.loads((DIRECTORY/(vendor+".json")).read_bytes())
            for label, apps, encoded in [("original", DIRECTORY/"classes", stage/"archived"),
                                         ("regenerated", stage/"apps", stage/"encoded")]:
                compare(replay(vm, options, apps, stage/"control", stage/"owners", encoded), reference)
                print(f"{vendor} {label}: {len(reference)} monitor observations reproduced", flush=True)
        tools = ("monitor_reference.py", "monitor_encodings.py", "type_state_encodings.py", "type_state_reference.py",
                 "application_entry_reference.py", "loading_constraint_reference.py", "fixture_source.py")
        manifest = {"primary":primary, "compiler":version(javac), "secondary":pin["runtime"],
                    "secondary_archive_sha256":pin["archive_sha256"], "flags":flags,
                    "sources":{path.as_posix():digest(data) for path, data in sources.items()},
                    "tools":{name:tool_digest(ROOT/"tools"/name) for name in tools}, "raw":raw_hashes,
                    "original":originals, "compiled":inventory(stage/"apps"), "control":inventory(stage/"control"),
                    "owners":inventory(stage/"owners"), "observations":{v:digest((DIRECTORY/(v+".json")).read_bytes())
                                                                    for v in ("hotspot","openj9")}}
        output = DIRECTORY/"manifest.json"
        if check:
            check_manifest(manifest, json.loads(output.read_bytes()))
        else:
            if not secondary or manifest["compiled"] != originals:
                raise ValueError("recording requires both reference VMs and the original javac outputs")
            output.write_bytes(serialize(manifest))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--compare-openj9", type=Path, metavar="JAVA")
    args = parser.parse_args()
    record(args.check, args.compare_openj9)
