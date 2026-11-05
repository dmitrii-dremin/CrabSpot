"""Reproduce original class-checking inputs and their JDK verification stages."""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

from fixture_source import read_java_source
from type_state_encodings import ClassEncoding, RETURN_ONE
from type_state_reference import FIXTURES, ROOT, check_or_write, digest, serialize


def build():
    bad = (9, "bad", "()I", bytes.fromhex("01 ac"), 1, 0)
    native = (0x109, "nativeCode", "()I")
    abstract = (0x401, "required", "()I")
    files = {}

    def emit(name, methods=(), parent="java/lang/Object", **header):
        encoding = ClassEncoding("classprobe/" + name, parent)
        files[name] = encoding.finish([], methods, **header)

    emit("GoodThenBad", [RETURN_ONE, bad])
    emit("BadThenGood", [bad, RETURN_ONE])
    emit("BadParent", [bad])
    emit("ChildValid", [RETURN_ONE], parent="classprobe/BadParent")
    emit("EmptyModern")
    emit("EmptyHistory", major=50)
    emit("EmptyPreview", minor=65535)
    emit("EmptyInterface", flags=0x601)
    emit("FinalParent", flags=0x31)
    emit("ChildOfFinal", parent="classprobe/FinalParent")
    emit("AbstractOnly", [abstract], flags=0x421)
    emit("NativeOnly", [native])
    emit("AbstractAndNative", [abstract, native], flags=0x421)
    emit("NativeWithBody", [(*native, bytes.fromhex("04 ac"), 1, 0)])
    emit("InvalidSecondAfterNative", [native, bad])
    return files


def filename(name):
    # The fuzz harness requires every .class seed to parse with default options.
    suffix = ".class.bin" if name in {"EmptyPreview", "NativeWithBody"} else ".class"
    return name + suffix


def probe(java, directory, names, preview):
    args = [java, "-Xverify:all"]
    if preview:
        args.append("--enable-preview")
    args += ["-cp", str(FIXTURES / "classes"), "fixtures.ClassProofCases", str(directory), *names]
    result = subprocess.run(args, capture_output=True, text=True, encoding="utf-8", check=True, timeout=45)
    rows = []
    for line in result.stdout.splitlines():
        name, stage, outcome = line.split("\t")
        if stage not in {"define", "methods", "invoke"} or not outcome:
            raise ValueError(f"invalid JDK stage result: {line}")
        rows.append((name, "preview" if preview else "standard", stage, outcome))
    if [row[0] for row in rows] != names:
        raise ValueError("JDK output differs from requested cases/order")
    return rows


def record(java, check):
    version = subprocess.run([java, "-version"], capture_output=True, text=True, check=True, timeout=30)
    version = (version.stdout + version.stderr).strip()
    if not re.match(r'(?:openjdk|java) version "21(?:[."+\-])', version):
        raise ValueError("a reference JDK 21 is required")
    source = FIXTURES / "java/ClassProofCases.java"
    source_hash = digest(read_java_source(source))
    driver = "fixtures/ClassProofCases.class"
    driver_hash = digest((FIXTURES / "classes" / driver).read_bytes())
    compiled = json.loads((FIXTURES / "classes/manifest.json").read_text(encoding="utf-8"))
    if compiled["sources"].get(source.name) != source_hash or compiled["files"].get(driver) != driver_hash:
        raise ValueError("compile the current ClassProofCases.java before recording")
    directory = FIXTURES / "class-proof"
    files = build()
    scratch = ROOT / "target/fixture-tool"
    scratch.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="class-proof-", dir=scratch) as temporary:
        target = Path(temporary) / "classprobe"
        target.mkdir()
        for name, data in files.items():
            (target / (name + ".class")).write_bytes(data)
        rows = probe(java, target.parent, sorted(files), False)
        rows += probe(java, target.parent, ["EmptyPreview"], True)
    artifacts = {filename(name): data for name, data in files.items()}
    existing = {path.name for path in directory.glob("*") if path.name != "manifest.json"}
    if existing - artifacts.keys() or (check and existing != artifacts.keys()):
        raise ValueError("class-proof artifact set differs")
    emitters = [Path(__file__), ROOT / "tools/type_state_encodings.py"]
    manifest = {"source": source.name, "source_sha256": source_hash,
                "kind": "Original class encodings; two default-parser rejections use .class.bin",
                "emitters": {p.name: digest(p.read_bytes().replace(b"\r\n", b"\n")) for p in emitters},
                "files": {name: digest(data) for name, data in artifacts.items()}}
    for name, data in artifacts.items():
        check_or_write(directory / name, data, check)
    check_or_write(directory / "manifest.json", serialize(manifest), check)
    data = "".join("\t".join(row) + "\n" for row in rows).encode("utf-8")
    reference = FIXTURES / "reference/class-proof.tsv"
    check_or_write(reference, data, check)
    provenance = {"source": source.name, "source_sha256": source_hash, "cases": len(rows),
                  "driver_classes": {driver: driver_hash}, "results_sha256": digest(data),
                  "original_manifest_sha256": digest(serialize(manifest)),
                  "options": ["-Xverify:all"], "preview_enabled_case": "EmptyPreview"}
    metadata = reference.with_suffix(".json")
    if check:
        stored = json.loads(metadata.read_text(encoding="utf-8"))
        if {key: stored.get(key) for key in provenance} != provenance:
            raise ValueError("class-proof JDK provenance differs")
    else:
        metadata.write_bytes(serialize({**provenance, "runtime": version}))
    print(f"{'Verified' if check else 'Recorded'} {len(files)} class-proof inputs and {len(rows)} JDK stage outcomes")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--java")
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    home = os.environ.get("JAVA_HOME")
    java = args.java or (str(Path(home) / "bin" / ("java.exe" if os.name == "nt" else "java"))
                         if home else shutil.which("java"))
    if not java:
        parser.error("set JAVA_HOME to a reference JDK 21")
    try:
        record(java, args.check)
    except (ValueError, OSError, subprocess.SubprocessError) as exc:
        parser.exit(1, f"class-proof reference failed: {exc}\n")
