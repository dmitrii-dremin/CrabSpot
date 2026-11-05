"""Rebuild the cross-loader member references and replay their per-VM observations."""

import argparse
import json
from pathlib import Path
import re
import subprocess
import tempfile

from fixture_source import read_java_source
from loading_constraint_reference import reference_tool, version
from member_resolution_encodings import field_method_order, signature_order
from type_state_reference import FIXTURES, ROOT, digest, serialize


DIRECTORY = FIXTURES / "member-resolution"
SHARED = FIXTURES / "loading-constraints/LoadingConstraintProbe.java"
FAMILIES = {
    "ordinary": ("Resolution", 24),
    "dispatch": ("DispatchProbe", 32),
    "field-method-order": ("OrderProbe", 39),
    "signature-order": ("LeafProbe", 20),
    "missing-class": ("StickyProbe", 4),
}
ENCODINGS = {"field-method-order": field_method_order, "signature-order": signature_order}


def class_bytes(directory):
    return {p.relative_to(directory).as_posix(): p.read_bytes() for p in directory.rglob("*.class")}


def hashes(data):
    return {name: digest(raw) for name, raw in data.items()}


def sources():
    # Read every source before launching even java -version. Compile these same validated bytes.
    files = {SHARED.name: read_java_source(SHARED)}
    for path in sorted(DIRECTORY.rglob("*.java")):
        files[path.relative_to(DIRECTORY).as_posix()] = read_java_source(path)
    return files


def read_originals():
    originals = json.loads((DIRECTORY / "originals.json").read_bytes())
    if originals.keys() != FAMILIES.keys():
        raise ValueError("original member-resolution family inventory differs")
    for family, record in originals.items():
        folder = DIRECTORY / family
        if hashes(class_bytes(folder / "classes")) != record["original_input_classes"]:
            raise ValueError(f"{family}: original class files differ")
        for vendor in ("zulu", "openj9"):
            raw = (folder / (vendor + ".tsv")).read_bytes()
            if (digest(raw) != record["observations"][vendor] or b"\r" in raw
                    or len(raw.splitlines()) != FAMILIES[family][1]):
                raise ValueError(f"{family}: original {vendor} observation set differs")
    return originals


def compare_rows(family, actual, expected):
    observed = actual.splitlines()
    baseline = expected.splitlines()
    if len(observed) != FAMILIES[family][1] or len(observed) != len(baseline):
        raise ValueError(f"{family}: incomplete observation set")
    for index, (row, original) in enumerate(zip(observed, baseline, strict=True), 1):
        if row != original:
            raise ValueError(f"{family}, row {index}: got {row!r}; expected {original!r}")


def inspect_symbols(javap, classes, family):
    # Match ASCII facts in the byte stream: javap may localize unrelated filesystem metadata.
    if family == "ordinary":
        cls = "caller.Probe"
        expected = [("getstatic", "symbolic/Sub.slot:Lvalue/K;"), ("putstatic", "symbolic/Sub.slot:Lvalue/K;"),
                    ("invokestatic", "symbolic/Sub.echo:(Lvalue/K;)Lvalue/K;")]
    elif family == "dispatch":
        cls = "caller.Dispatch"
        expected = [("invokevirtual", "symbolic/Sub.hit:(Lvalue/K;)I"), ("invokeinterface", "api/Port.hit:(Lvalue/K;)I"),
                    ("getfield", "symbolic/Sub.slot:Lvalue/K;")]
    elif family == "field-method-order":
        cls = "call.Steps"
        expected = [(opcode, f"sym/Sub.{name}:Lvalue/K;") for name, opcodes in (
            ("sf", ("getstatic", "getfield")), ("instanceField", ("getfield", "getstatic")),
            ("finalField", ("putstatic",)), ("privateField", ("getstatic",)), ("absent", ("getstatic",))
        ) for opcode in opcodes]
        expected += [(opcode, f"sym/Sub.{name}:(Lvalue/K;)V") for name, opcodes in (
            ("publicCall", ("invokestatic", "invokevirtual")), ("instanceCall", ("invokevirtual", "invokestatic")),
            ("privateCall", ("invokestatic",)), ("absent", ("invokestatic",))
        ) for opcode in opcodes]
    elif family == "signature-order":
        cls = "call.Steps"
        expected = [("invokestatic", "sym/Sub." + target) for target in (
            "PQ:(Lvalue/P;Lvalue/Q;)V", "QP:(Lvalue/Q;Lvalue/P;)V", "PRet:(Lvalue/Q;)Lvalue/P;",
            "QRet:(Lvalue/P;)Lvalue/Q;", "Array:([[Lvalue/P;[Lvalue/Q;)V",
        )]
    elif family == "missing-class":
        cls = "caller.Sticky"
        expected = [("getstatic", "owner/Late.count:I"), ("invokestatic", "owner/Late.value:()I")]
    else:
        raise ValueError(f"unknown member-resolution family: {family}")
    output = subprocess.run([javap, "-v", "-p", "-classpath", classes, cls],
                            check=True, capture_output=True, timeout=30).stdout
    facts = {}
    for opcode, symbol in expected:
        pattern = rb"\b" + opcode.encode() + rb"\s+#(\d+)\b[^\n]*\s" + re.escape(symbol.encode()) + rb"(?:\s|$)"
        ids = {int(index) for index in re.findall(pattern, output)}
        if len(ids) != 1:
            raise ValueError(f"{family}: expected a single {opcode} CP entry for {symbol}, found {ids}")
        index = ids.pop()
        tag = "InterfaceMethodref" if opcode == "invokeinterface" else "Methodref" if opcode.startswith("invoke") else "Fieldref"
        constant = rb"^\s*#" + str(index).encode() + rb"\s+=\s+" + tag.encode() + rb"\s+#(\d+)\.#\d+\s+// " + re.escape(symbol.encode()) + rb"\s*$"
        match = re.search(constant, output, re.MULTILINE)
        if not match:
            raise ValueError(f"{family}: CP #{index} does not name the expected {tag} {symbol}")
        site = facts.setdefault(symbol, {"index": index, "tag": tag, "owner_index": int(match[1]), "opcodes": []})
        if site["index"] != index:
            raise ValueError(f"{family}: expected all uses of {symbol} to share a CP entry")
        site["opcodes"].append(opcode)
    if family == "missing-class" and len({site["owner_index"] for site in facts.values()}) != 1:
        raise ValueError("expected field and method sites to share the failed Class entry")
    return facts


def replay(java, vendor, drivers, family, classes):
    name, _ = FAMILIES[family]
    options = ["-Xshareclasses:none"] if vendor == "openj9" else []
    result = subprocess.run([java, *options, "-Xverify:all", "-cp", drivers, name, classes],
                            check=True, capture_output=True, encoding="utf-8", timeout=60)
    compare_rows(family, result.stdout, (DIRECTORY / family / (vendor + ".tsv")).read_text(encoding="utf-8"))


def record(check, secondary_java=None):
    source_bytes = sources()
    originals = read_originals()
    java, javac, javap = (reference_tool(tool) for tool in ("java", "javac", "javap"))
    compiler, runtime = version(javac), version(java)
    archive = json.loads((FIXTURES / "loading-constraints/openj9/manifest.json").read_bytes())
    if secondary_java and version(secondary_java) != archive["runtime"]:
        raise ValueError("expected the pinned Semeru OpenJ9 runtime build")
    scratch = ROOT / "target/fixture-tool"
    scratch.mkdir(parents=True, exist_ok=True)
    program_hashes, symbols = {}, {}
    with tempfile.TemporaryDirectory(dir=scratch, prefix="member-resolution-") as temporary:
        stage = Path(temporary)
        for name, raw in source_bytes.items():
            path = stage / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(raw)
        drivers = stage / "compiled-drivers"
        driver_sources = [stage / family / (entry + ".java") for family, (entry, _) in FAMILIES.items()]
        subprocess.run([javac, "--release", "21", "-encoding", "UTF-8", "-d", drivers,
                        stage / SHARED.name, *driver_sources], check=True, timeout=60)
        driver_hashes = hashes(class_bytes(drivers))
        for family in FAMILIES:
            archived = DIRECTORY / family / "classes"
            fresh = stage / family / "classes"
            if family in ENCODINGS:
                generated = ENCODINGS[family]()
                if hashes(generated) != originals[family]["original_input_classes"]:
                    raise ValueError(f"{family}: regenerated raw encoding differs from the original")
                for name, raw in generated.items():
                    path = fresh / name
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_bytes(raw)
            else:
                src = stage / family / "src"
                programs = sorted(src.rglob("*.java"))
                if {p.relative_to(src).as_posix(): digest(p.read_bytes()) for p in programs} != originals[family]["original_sources"]:
                    raise ValueError(f"{family}: original javac sources differ")
                subprocess.run([javac, "--release", "21", "-encoding", "UTF-8", "-d", fresh, *programs],
                               check=True, timeout=60)
            generated = hashes(class_bytes(fresh))
            original = originals[family]["original_input_classes"]
            if generated.keys() != original.keys():
                raise ValueError(f"{family}: regenerated class inventory differs")
            program_hashes[family] = generated
            symbols[family] = inspect_symbols(javap, archived, family)
            corpora = [archived] if original == generated else [archived, fresh]
            for corpus in corpora:
                if corpus != archived:
                    inspect_symbols(javap, corpus, family)
                replay(java, "zulu", drivers, family, corpus)
                if secondary_java:
                    replay(secondary_java, "openj9", drivers, family, corpus)
    checked_inputs = {
        "sources": hashes(source_bytes),
        "tools": {p.name: digest(p.read_bytes()) for p in [
            Path(__file__), ROOT / "tools/member_resolution_encodings.py", ROOT / "tools/type_state_encodings.py"]},
        "originals_sha256": digest((DIRECTORY / "originals.json").read_bytes()),
        "archived_symbols": symbols,
        "primary_runtime_options": ["-Xverify:all"],
        "secondary_release": archive["release"], "secondary_archive_sha256": archive["archive_sha256"],
        "secondary_runtime_options": ["-Xshareclasses:none", "-Xverify:all"],
        "compiler_options": ["--release", "21", "-encoding", "UTF-8"],
    }
    path = DIRECTORY / "manifest.json"
    if check:
        stored = json.loads(path.read_bytes())
        if stored["inputs"] != checked_inputs:
            raise ValueError("member-resolution provenance differs")
        if (stored["compiler"], stored["primary_runtime"]) == (compiler, runtime):
            if stored["compiled_drivers"] != driver_hashes or stored["compiled_programs"] != program_hashes:
                raise ValueError("same-build compiler outputs differ")
    else:
        path.write_bytes(serialize({"inputs": checked_inputs, "compiler": compiler, "primary_runtime": runtime,
                                    "compiled_drivers": driver_hashes, "compiled_programs": program_hashes}))
    total = sum(count for _, count in FAMILIES.values())
    print(f"Replayed {total} member-resolution observations on HotSpot"
          + (f" and {total} on the pinned OpenJ9 runtime" if secondary_java else ""))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="verify the committed provenance")
    parser.add_argument("--compare-openj9", type=Path, metavar="JAVA", help="also replay the pinned OpenJ9 runtime")
    args = parser.parse_args()
    record(args.check, args.compare_openj9)
