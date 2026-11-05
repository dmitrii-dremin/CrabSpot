"""Regenerate original type-state class encodings and record explicit JDK 21 verification stages."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

from fixture_source import read_java_source
from type_state_encodings import build


ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "tests/fixtures"


def digest(data):
    return hashlib.sha256(data).hexdigest()


def check_or_write(path, data, check):
    if check:
        if path.read_bytes() != data:
            raise ValueError(f"reference artifact differs: {path}")
    else:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)


def serialize(value):
    return (json.dumps(value, indent=2, sort_keys=True) + "\n").encode("utf-8")


def record(java, check):
    version = subprocess.run([java, "-version"], capture_output=True, text=True, check=True, timeout=30)
    version = (version.stdout + version.stderr).strip()
    if not re.match(r'(?:openjdk|java) version "21(?:[."+\-])', version):
        raise ValueError(f"Java 21 required, got {version!r}")
    source = FIXTURES / "java/TypeStateCases.java"
    classes = FIXTURES / "classes"
    compiled = json.loads((classes / "manifest.json").read_text(encoding="utf-8"))
    source_hash = digest(read_java_source(source))
    if compiled["sources"].get(source.name) != source_hash:
        raise ValueError("compile the current TypeStateCases.java before recording")
    drivers = {}
    for name in ["fixtures/TypeStateCases.class", "fixtures/TypeStateCases$GraphLoader.class"]:
        drivers[name] = digest((classes / name).read_bytes())
        if drivers[name] != compiled["files"].get(name):
            raise ValueError(f"driver class provenance differs: {name}")

    files, selected = build()
    if (len(files), len(selected)) != (69, 57):
        raise ValueError("unexpected number of classes/cases")
    destination = FIXTURES / "type-state"
    hashes = {name + ".class": digest(data) for name, data in files.items()}
    existing = {path.relative_to(destination).as_posix() for path in destination.rglob("*.class")}
    if existing - hashes.keys() or (check and existing != hashes.keys()):
        raise ValueError("unexpected original type-state fixture set")
    scratch = ROOT / "target/fixture-tool"
    scratch.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="type-state-", dir=scratch) as temporary:
        for name, data in files.items():
            path = Path(temporary) / (name + ".class")
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
        command = [java, "-Xverify:all", "-cp", str(classes), "fixtures.TypeStateCases",
                   temporary, *sorted(selected)]
        probe = subprocess.run(command, capture_output=True, text=True, encoding="utf-8",
                               check=True, timeout=45)
    rows = {}
    for line in probe.stdout.splitlines():
        name, stage, outcome = line.split("\t")
        if name in rows or name not in selected or stage not in {"define", "lookup", "invoke"}:
            raise ValueError(f"unexpected oracle row: {line}")
        method, descriptor = selected[name]
        rows[name] = "\t".join((name, method, descriptor, stage, outcome))
    if rows.keys() != selected.keys():
        raise ValueError("JDK oracle omitted a selected class")
    output = ("\n".join(rows[name] for name in sorted(rows)) + "\n").encode("utf-8")
    emitter = ROOT / "tools/type_state_encodings.py"
    manifest = {"kind": "Original synthetic class encodings", "source": source.name,
                "source_sha256": source_hash, "files": hashes, "emitter": emitter.name,
                "emitter_sha256": digest(emitter.read_bytes().replace(b"\r\n", b"\n"))}
    provenance = {"source": source.name, "source_sha256": source_hash, "cases": len(rows),
                  "driver_classes": drivers, "original_manifest_sha256": digest(serialize(manifest)),
                  "options": ["-Xverify:all"], "results_sha256": digest(output)}
    for name, data in files.items():
        check_or_write(destination / (name + ".class"), data, check)
    check_or_write(destination / "manifest.json", serialize(manifest), check)
    reference = FIXTURES / "reference/type-state.tsv"
    check_or_write(reference, output, check)
    metadata = reference.with_suffix(".json")
    if check:
        recorded = json.loads(metadata.read_text(encoding="utf-8"))
        if {key: recorded.get(key) for key in provenance} != provenance:
            raise ValueError("type-state reference provenance differs")
    else:
        metadata.write_bytes(serialize({**provenance, "runtime": version}))
    print(f"{'Verified' if check else 'Recorded'} {len(rows)} JDK stage outcomes across {len(files)} original classes")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--java")
    args = parser.parse_args()
    home = os.environ.get("JAVA_HOME")
    java = args.java or (str(Path(home) / "bin" / ("java.exe" if os.name == "nt" else "java"))
                         if home else shutil.which("java"))
    if not java:
        parser.error("set JAVA_HOME to a reference JDK 21")
    try:
        record(java, args.check)
    except (OSError, ValueError, subprocess.SubprocessError) as exc:
        parser.exit(1, f"type-state reference failed: {exc}\n")
