"""Capture truthful reference-JDK facts as synthetic, nonexecutable verifier fixtures."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

from fixture_source import read_java_source

ROOT = Path(__file__).resolve().parents[1]
DESTINATION = ROOT / "tests/fixtures/type-state-facts"


def digest(data):
    return hashlib.sha256(data).hexdigest()


def decode_units(value):
    if len(value) % 4 or not re.fullmatch(r"[0-9a-f]*", value):
        raise ValueError("invalid UTF-16 fact encoding")
    return bytes.fromhex(value).decode("utf-16-be", errors="surrogatepass")


def parse_facts(data):
    """Validate the capture envelope independently; Rust validates actual class metadata."""
    classes = {}
    current = None
    member_ids = set()
    for line in data.decode("utf-8").splitlines():
        row = line.split("\t")
        if row[0] == "C" and len(row) == 9:
            name, parent = decode_units(row[2]), decode_units(row[6])
            if not name or name in classes or row[1] not in {"bootstrap", "platform"}:
                raise ValueError("invalid/duplicate fact class or defining-loader designation")
            if not re.fullmatch(r"[0-9a-f]{64}", row[7]) or not row[8].startswith("jrt:/"):
                raise ValueError("fact class lacks original JDK resource provenance")
            major, minor, flags = map(int, row[3:6])
            if major != 65 or minor != 0 or not 0 <= flags <= 65535:
                raise ValueError("expected non-preview Java21 fact class")
            current = {"loader": row[1], "name": name, "parent": parent, "interfaces": [],
                       "flags": flags, "fields": [], "methods": [], "original_sha256": row[7],
                       "resource": row[8]}
            classes[name] = current
            member_ids.clear()
        elif current is not None and row[0] == "I" and len(row) == 2:
            if current["fields"] or current["methods"]:
                raise ValueError("interface fact after member facts")
            current["interfaces"].append(decode_units(row[1]))
        elif current is not None and ((row[0] == "F" and len(row) == 4) or (row[0] == "M" and len(row) == 5)):
            flags = int(row[1])
            if not 0 <= flags <= 65535 or (row[0] == "F" and current["methods"]):
                raise ValueError("invalid member fact flags/order")
            name, descriptor = decode_units(row[2]), decode_units(row[3])
            if not name or not descriptor:
                raise ValueError("empty member fact identity")
            member = (flags, name, descriptor)
            if row[0] == "M":
                if row[4] not in {"0", "1"}:
                    raise ValueError("invalid original Code-presence fact")
                member += (row[4] == "1",)
            members = current["fields" if row[0] == "F" else "methods"]
            identity = (row[0], name, descriptor)
            if identity in member_ids:
                raise ValueError("duplicate member fact identity")
            member_ids.add(identity)
            members.append(member)
        else:
            raise ValueError("malformed fact record")
    if not classes:
        raise ValueError("empty fact capture")
    for record in classes.values():
        if any(name not in classes for name in record["interfaces"] + ([record["parent"]] if record["parent"] else [])):
            raise ValueError("fact hierarchy is not closed")
    return classes


def metadata_from_inspector(inspector, path):
    run = subprocess.run([str(inspector), "inspect", str(path)], capture_output=True,
                         text=True, encoding="utf-8", check=True, timeout=30)
    if run.stderr or "\nattributes:" not in run.stdout:
        raise ValueError(f"unexpected inspector response: {path}")
    # Preserve all ordered class/header/hierarchy/member facts, excluding CP layout
    # and attributes intentionally absent from a metadata projection.
    lines = run.stdout.split("\nattributes:", 1)[0].splitlines()
    return ("\n".join(line for line in lines if not line.startswith("constant_pool_count:")) + "\n").encode("utf-8")


def compare_pair(inspector, capture, name, expected):
    relative = Path(name + ".class")
    original = (capture / "original" / relative).resolve()
    projected = (capture / "projected" / relative).resolve()
    original.relative_to((capture / "original").resolve())
    projected.relative_to((capture / "projected").resolve())
    if digest(original.read_bytes()) != expected["original_sha256"]:
        raise ValueError(f"captured original digest differs: {name}")
    original_metadata = metadata_from_inspector(inspector, original)
    if original_metadata != metadata_from_inspector(inspector, projected):
        raise ValueError(f"Rust parser found projected metadata mismatch: {name}")
    return name, digest(original_metadata)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--java")
    parser.add_argument("--inspector", type=Path)
    args = parser.parse_args()
    home = os.environ.get("JAVA_HOME")
    java = args.java or (str(Path(home) / "bin" / ("java.exe" if os.name == "nt" else "java"))
                         if home else shutil.which("java"))
    target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
    if not target.is_absolute():
        target = ROOT / target
    inspector = args.inspector or target / "debug" / ("crabspot.exe" if os.name == "nt" else "crabspot")
    if not java or not inspector.is_file():
        parser.error("JDK21 and built CrabSpot inspector required; set JAVA_HOME and run cargo build -p crabspot-launcher")
    try:
        source = ROOT / "tests/fixtures/java/TypeVerificationFacts.java"
        source_hash = digest(read_java_source(source))
        compiled = json.loads((ROOT / "tests/fixtures/classes/manifest.json").read_text(encoding="utf-8"))
        if compiled.get("sources", {}).get(source.name) != source_hash:
            raise ValueError("compile TypeVerificationFacts.java before capturing facts")
        driver_hashes = {}
        for suffix in ("", "$Member", "$Metadata", "$Pool"):
            name = f"fixtures/TypeVerificationFacts{suffix}.class"
            driver_hashes[name] = digest((ROOT / "tests/fixtures/classes" / name).read_bytes())
            if compiled.get("files", {}).get(name) != driver_hashes[name]:
                raise ValueError(f"compiled fact driver provenance differs: {name}")
        inputs = [ROOT / "tests/fixtures/classes"]
        original_cases = ROOT / "tests/fixtures/type-state"
        if original_cases.is_dir():
            inputs.append(original_cases)
        input_hashes = {path.relative_to(ROOT / "tests/fixtures").as_posix(): digest(path.read_bytes())
                        for directory in inputs for path in sorted(directory.rglob("*.class"))}
        version = subprocess.run([java, "-version"], capture_output=True, text=True, check=True, timeout=30)
        version = (version.stdout + version.stderr).strip()
        if not re.match(r'(?:openjdk|java) version "21(?:[.\"+\-])', version):
            raise ValueError(f"reference runtime must be Java21: {version!r}")
        scratch = ROOT / "target/fixture-tool"
        scratch.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="type-facts-", dir=scratch) as temporary:
            capture = Path(temporary)
            command = [java, "-cp", str(ROOT / "tests/fixtures/classes"), "fixtures.TypeVerificationFacts", str(capture), *map(str, inputs)]
            subprocess.run(command, check=True, timeout=60)
            facts_bytes = (capture / "facts.tsv").read_bytes()
            records = parse_facts(facts_bytes)
            expected_paths = {name + ".class" for name in records}
            for directory in ("original", "projected"):
                actual = {path.relative_to(capture / directory).as_posix() for path in (capture / directory).rglob("*.class")}
                if actual != expected_paths:
                    raise ValueError(f"captured {directory} class set differs from facts")
            # Independent Rust reader, not the Java capture/parser itself, checks every pair.
            with ThreadPoolExecutor(max_workers=8) as pool:
                comparisons = list(pool.map(lambda item: compare_pair(inspector, capture, *item), records.items()))
            files = {"facts.tsv": facts_bytes, "requested.txt": (capture / "requested.txt").read_bytes(),
                     "missing.tsv": (capture / "missing.tsv").read_bytes()}
            for name in sorted(records):
                files["classes/" + name + ".class"] = (capture / "projected" / (name + ".class")).read_bytes()
            manifest = {"source": source.name, "source_sha256": source_hash,
                        "kind": "Synthetic metadata projections; inert bodies never executed or credited as genuine proofs",
                        "driver_classes": driver_hashes, "input_classes": input_hashes,
                        "classes": len(records), "fields": sum(len(record["fields"]) for record in records.values()),
                        "methods": sum(len(record["methods"]) for record in records.values()),
                        "independent_metadata_sha256": dict(comparisons),
                        "files": {name: digest(data) for name, data in files.items()}}
            manifest_path = DESTINATION / "manifest.json"
            if args.check:
                for name, data in files.items():
                    if (DESTINATION / name).read_bytes() != data:
                        raise ValueError(f"reference fact fixture differs: {name}")
                existing = {path.relative_to(DESTINATION).as_posix() for path in DESTINATION.rglob("*.class")}
                if existing != {name for name in files if name.endswith(".class")}:
                    raise ValueError("reference fact fixture class set differs")
                recorded = json.loads(manifest_path.read_text(encoding="utf-8"))
                if {key: recorded.get(key) for key in manifest} != manifest:
                    raise ValueError("reference fact fixture provenance differs")
            else:
                previous = json.loads(manifest_path.read_text(encoding="utf-8")) if manifest_path.exists() else {"files": {}}
                for path in DESTINATION.rglob("*.class"):
                    relative = path.relative_to(DESTINATION).as_posix()
                    if relative not in files:
                        if relative not in previous.get("files", {}):
                            raise ValueError(f"unexpected unmanaged projection: {relative}")
                        path.resolve().relative_to(DESTINATION.resolve())
                        path.unlink()  # Only obsolete, previously managed individual fixture files.
                for name, data in files.items():
                    destination = DESTINATION / name
                    destination.resolve().relative_to(DESTINATION.resolve())
                    destination.parent.mkdir(parents=True, exist_ok=True)
                    destination.write_bytes(data)
                manifest["reference_runtime"] = version
                manifest_path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8", newline="\n")
        print(f"{'Verified' if args.check else 'Recorded'} {len(records)} synthetic JDK fact projections; all metadata independently matched by Rust")
        print(version)
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        parser.exit(1, f"type-verification fact capture failed: {error}\n")


if __name__ == "__main__":
    main()
