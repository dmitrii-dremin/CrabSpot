"""Check committed Java source provenance before any JDK regeneration."""
import hashlib
import json
from pathlib import Path
import sys
from fixture_source import read_java_source
from type_verification_facts import parse_facts


def read_object(path):
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"{path}: expected a JSON object")
    return value


def file_hashes(directory, pattern):
    return {p.relative_to(directory).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest()
            for p in directory.rglob(pattern) if p.is_file()}


def check_type_state(root, sources, compiled):
    fixtures = root / "tests/fixtures"
    generated = fixtures / "type-state"
    projections = fixtures / "type-state-facts"
    if not (generated.exists() or projections.exists() or "TypeStateCases.java" in sources
            or "TypeVerificationFacts.java" in sources):
        return
    raw = read_object(generated / "manifest.json")
    facts = read_object(projections / "manifest.json")
    reference = fixtures / "reference/type-state"
    oracle = read_object(reference.with_suffix(".json"))
    for record, driver in [(raw, "TypeStateCases.java"), (facts, "TypeVerificationFacts.java"),
                           (oracle, "TypeStateCases.java")]:
        if (driver not in sources or record.get("source") != driver
                or record.get("source_sha256") != sources[driver]):
            raise ValueError(f"type-state source provenance differs: {driver}")
    raw_classes = file_hashes(generated, "*.class")
    if not raw_classes or raw_classes != raw.get("files"):
        raise ValueError("type-state original class set/digests differ")
    emitter = root / "tools/type_state_encodings.py"
    if (raw.get("emitter") != emitter.name or raw.get("emitter_sha256") !=
            hashlib.sha256(emitter.read_bytes().replace(b"\r\n", b"\n")).hexdigest()):
        raise ValueError("type-state emitter hash differs")
    reference_bytes = reference.with_suffix(".tsv").read_bytes()
    if (oracle.get("results_sha256") != hashlib.sha256(reference_bytes).hexdigest()
            or oracle.get("cases") != len(reference_bytes.splitlines())
            or oracle.get("original_manifest_sha256") != hashlib.sha256((generated / "manifest.json").read_bytes()).hexdigest()):
        raise ValueError("type-state oracle provenance differs")
    genuine_classes = file_hashes(fixtures / "classes", "*.class")
    if genuine_classes != compiled.get("files"):
        raise ValueError("type-state genuine input class set/digests differ")
    expected_inputs = {"classes/" + name: value for name, value in genuine_classes.items()}
    expected_inputs.update({"type-state/" + name: value for name, value in raw_classes.items()})
    if facts.get("input_classes") != expected_inputs:
        raise ValueError("type-state JDK facts target class set/digests differ")
    for record, names in [(facts, ["TypeVerificationFacts" + s for s in ("", "$Member", "$Metadata", "$Pool")]),
                          (oracle, ["TypeStateCases", "TypeStateCases$GraphLoader"])]:
        paths = ["fixtures/" + name + ".class" for name in names]
        if not all(p in genuine_classes for p in paths) or record.get("driver_classes") != {p: genuine_classes[p] for p in paths}:
            raise ValueError("type-state driver class hashes differ")
    hashes = file_hashes(projections, "*")
    hashes.pop("manifest.json")
    if facts.get("files") != hashes:
        raise ValueError("type-state projected fixture set/digests differ")
    rows = parse_facts((projections / "facts.tsv").read_bytes())
    classes = {"classes/" + name + ".class" for name in rows}
    if (set(hashes) != classes | {"facts.tsv", "missing.tsv", "requested.txt"}
            or facts.get("classes") != len(rows)
            or facts.get("methods") != sum(len(r["methods"]) for r in rows.values())
            or facts.get("fields") != sum(len(r["fields"]) for r in rows.values())
            or set(facts.get("independent_metadata_sha256", {})) != set(rows)):
        raise ValueError("type-state projected definitions and JDK fact rows differ")


def verify(root):
    sources = root / "tests" / "fixtures" / "java"
    hashes = {p.relative_to(sources).as_posix(): hashlib.sha256(read_java_source(p)).hexdigest()
              for p in sources.rglob("*.java")}
    manifest = read_object(root / "tests" / "fixtures" / "classes" / "manifest.json")
    if not hashes or manifest.get("sources") != hashes:
        raise ValueError("fixture manifest source hashes/set differ from canonical Java sources")
    records = sorted((root / "tests" / "fixtures" / "reference").glob("*.json"))
    results = sorted((root / "tests" / "fixtures" / "reference").glob("*.tsv"))
    if {p.stem for p in records} != {p.stem for p in results}:
        raise ValueError("reference JSON/TSV provenance sets differ")
    if not records:
        raise ValueError("no reference provenance records found")
    for path in records:
        record = read_object(path)
        source = record.get("source")
        if not isinstance(source, str) or source not in hashes:
            raise ValueError(f"{path}: unknown or missing reference source identity")
        if record.get("source_sha256") != hashes[source]:
            raise ValueError(f"{path}: reference source hash differs from {source}")
    for name, driver in (("construction", "ConstructionCases.java"),
                         ("code-validation", "CodeValidationCases.java")):
        directory = root / "tests" / "fixtures" / name
        if directory.exists() or driver in hashes:
            synthetic = read_object(directory / "manifest.json")
            source = synthetic.get("source")
            if not isinstance(source, str) or source not in hashes or synthetic.get("source_sha256") != hashes[source]:
                raise ValueError(f"{name} fixture source provenance differs")
            actual = {path.name: hashlib.sha256(path.read_bytes()).hexdigest() for path in directory.glob("*.class")}
            if not actual or synthetic.get("files") != actual:
                raise ValueError(f"{name} fixture hashes/set differ")
    check_type_state(root, hashes, manifest)
    return len(hashes), len(records)


if __name__ == "__main__":
    try:
        sources, records = verify(Path(__file__).resolve().parents[1])
        print(f"Verified canonical source provenance: {sources} Java sources, {records} reference records")
    except (OSError, ValueError) as error:
        sys.exit(f"source provenance check failed: {error}")
