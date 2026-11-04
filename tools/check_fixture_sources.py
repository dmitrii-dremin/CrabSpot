"""Check committed Java source provenance before any JDK regeneration."""
import hashlib
import json
from pathlib import Path
import sys
from fixture_source import read_java_source


def read_object(path):
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"{path}: expected a JSON object")
    return value


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
    return len(hashes), len(records)


if __name__ == "__main__":
    try:
        sources, records = verify(Path(__file__).resolve().parents[1])
        print(f"Verified canonical source provenance: {sources} Java sources, {records} reference records")
    except (OSError, ValueError) as error:
        sys.exit(f"source provenance check failed: {error}")
