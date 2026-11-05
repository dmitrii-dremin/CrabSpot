"""Rebuild and run the original Java 21 directory-launch probes."""
import argparse
import json
from pathlib import Path
import subprocess
import tempfile

from fixture_source import read_java_source
from loading_constraint_reference import reference_tool, version
from type_state_reference import FIXTURES, ROOT, digest, serialize


DIRECTORY = FIXTURES / "application-entry"
NORMAL = ("Entry", "Marks", "Helper", "Calc", "First", "Second")
CASES = {
    "ordinary": ("Entry", NORMAL),
    "abrupt": ("Abrupt", ("Abrupt", "Marks")),
    "invalid": ("Invalid", ("Invalid", "Marks")),
    "no-main": ("NoMain", ("NoMain", "Marks")),
    "missing": ("Entry", tuple(name for name in NORMAL if name != "First")),
    "unused-invalid": ("Entry", (*NORMAL, "Invalid")),
}
PRINTING = ("BrokenInit", "Child", "Face", "Hidden", "Impl", "Nonstatic", "Parent", "Private", "Wide")
EFFECTS = ("Broken", "Child", "Face", "Hidden", "Impl", "Nonstatic", "Parent", "Private", "Wide")


def inventory(folder):
    return {p.relative_to(folder).as_posix(): digest(p.read_bytes()) for p in sorted(folder.rglob("*.class"))}


def tool_digest(path):
    # Git stores these Python sources as LF. An existing Windows worktree can still have CRLF.
    source = path.read_bytes().replace(b"\r\n", b"\n")
    if b"\r" in source:
        raise ValueError(f"unexpected bare carriage return in tool source: {path}")
    return digest(source)


def invalidate_return(data):
    # Invalid.defect is the only method that returns the literal 69.
    good = bytes.fromhex("10 45 ac")
    if data.count(good) != 1:
        raise ValueError("expected one bipush 69 / ireturn in the original Invalid class")
    return data.replace(good, bytes.fromhex("10 45 b0"))


def stage_cases(root, classes):
    paths = {}
    for case, (_, names) in CASES.items():
        directory = root / case
        (directory / "app").mkdir(parents=True)
        for name in names:
            relative = Path("app") / (name + ".class")
            (directory / relative).write_bytes((classes / relative).read_bytes())
        paths[case] = directory
    named = root / "named-effects"
    (named / "named").mkdir(parents=True)
    for name in ("Trace", *EFFECTS):
        relative = Path("named") / (name + ".class")
        (named / relative).write_bytes((classes / relative).read_bytes())
    paths["named-effects"] = named
    return paths


def launch(java, options, classpath, entry, *arguments):
    from os import pathsep
    result = subprocess.run([java, *options, "-Xverify:all", "-cp", pathsep.join(map(str, classpath)),
                             entry, *arguments], capture_output=True, timeout=45)
    return result.returncode, result.stdout.decode("utf-8").strip()


def replay(java, options, cases, external):
    rows = {}
    for case, (name, _) in CASES.items():
        folder = cases[case]
        rows[case] = {
            "direct": launch(java, options, [folder], "app." + name),
            "once": launch(java, options, [folder, external], "observer.Observe", name, "1"),
            "twice": launch(java, options, [folder, external], "observer.Observe", name, "2"),
        }
    rows["named-direct"] = {name: launch(java, options, [cases["named-effects"]], "named." + name)
                            for name in EFFECTS}
    rows["printing-control"] = {name: launch(java, options, [external], "entry." + name) for name in PRINTING}
    return rows


def check_observations(root, archived, compiled, external, java, secondary):
    for corpus, classes in [("original", archived), ("fresh", compiled)]:
        cases = stage_cases(root / corpus, classes)
        for vendor, vm, options in [("zulu", java, []), ("openj9", secondary, ["-Xshareclasses:none"])]:
            if vm is None:
                continue
            actual = serialize(replay(vm, options, cases, external))
            expected = (DIRECTORY / (vendor + ".json")).read_bytes()
            if actual != expected:
                actual_rows, original_rows = json.loads(actual), json.loads(expected)
                wrong = [case for case in actual_rows if original_rows.get(case) != actual_rows[case]]
                raise ValueError(f"{vendor} {corpus}: entry observations differ for {wrong}")


def record(check, secondary):
    source = {p.relative_to(DIRECTORY / "src"): read_java_source(p)
              for p in sorted((DIRECTORY / "src").rglob("*.java"))}
    original = json.loads((DIRECTORY / "original-classes.json").read_bytes())
    archived = DIRECTORY / "classes"
    if original != inventory(archived):
        raise ValueError("archived input classes differ from the original inventory")
    java, javac, javap = map(reference_tool, ("java", "javac", "javap"))
    compiler, primary = version(javac), version(java)
    if "OpenJ9" in primary or "Server VM" not in primary:
        raise ValueError("select a Java 21 HotSpot JDK for the primary reference")
    pin = json.loads((FIXTURES / "loading-constraints/openj9/manifest.json").read_bytes())
    if secondary and version(secondary) != pin["runtime"]:
        raise ValueError("expected the pinned Semeru OpenJ9 runtime")
    temp = ROOT / "target/fixture-tool"
    temp.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=temp) as work:
        root = Path(work)
        for name, raw in source.items():
            dest = root / "src" / name
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_bytes(raw)
        compiled = root / "compiled"
        external = root / "external"
        flags = ["--release", "21", "-g:none", "-encoding", "UTF-8"]
        programs = [root / "src" / name for name in source if name.parts[0] in {"app", "named"}]
        observers = [root / "src" / name for name in source if name.parts[0] in {"observer", "entry"}]
        subprocess.run([javac, *flags, "-d", compiled, *programs], check=True, timeout=60)
        subprocess.run([javac, *flags, "-cp", compiled, "-d", external, *observers], check=True, timeout=60)
        invalid = compiled / "app/Invalid.class"
        invalid.write_bytes(invalidate_return(invalid.read_bytes()))
        fresh = inventory(compiled)
        if fresh.keys() != original.keys():
            raise ValueError("recompiled program inventory differs")
        for folder in [archived, compiled]:
            listing = subprocess.run([javap, "-v", "-c", "-classpath", folder, "app.Entry"],
                                     capture_output=True, check=True, timeout=30).stdout
            if (b"([Ljava/lang/String;)V" not in listing
                    or any(b"= Class " in line and b"// java/lang/String" in line for line in listing.splitlines())):
                raise ValueError("main must accept String[] without a class constant for String")
        check_observations(root, archived, compiled, external, java, secondary)
        manifest = {"compiler": compiler, "primary": primary, "flags": flags,
                    "secondary": pin["runtime"], "secondary_archive_sha256": pin["archive_sha256"],
                    "sources": {name.as_posix(): digest(raw) for name, raw in source.items()},
                    "tools": {name: tool_digest(ROOT / "tools" / name) for name in
                              ("application_entry_reference.py", "fixture_source.py", "loading_constraint_reference.py",
                               "type_state_reference.py")},
                    "original": original, "compiled": fresh, "observer": inventory(external),
                    "observations": {vm: digest((DIRECTORY / (vm + ".json")).read_bytes()) for vm in ("zulu", "openj9")}}
        destination = DIRECTORY / "manifest.json"
        if check:
            stored = json.loads(destination.read_bytes())
            for key in ("flags", "secondary", "secondary_archive_sha256", "sources", "tools", "original", "observations"):
                if manifest[key] != stored[key]:
                    raise ValueError(f"application entry provenance differs: {key}")
            if compiler == stored["compiler"] and primary == stored["primary"]:
                if fresh != stored["compiled"] or manifest["observer"] != stored["observer"]:
                    raise ValueError("same-build application or observer class output differs")
        else:
            destination.write_bytes(serialize(manifest))
    print("Replayed original and freshly compiled application entry cases on HotSpot"
          + (" and pinned OpenJ9" if secondary else ""))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--compare-openj9", type=Path, metavar="JAVA")
    args = parser.parse_args()
    record(args.check, args.compare_openj9)
