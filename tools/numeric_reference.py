"""Generate/check OpenJDK 21 results for the pure numeric method-body fixtures."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]
CLASSES = ROOT / "tests" / "fixtures" / "classes"


def digest(data):
    return hashlib.sha256(data).hexdigest()


def main(fixture_name="NumericCases", result_name="numeric", result_pattern=r"(?:[IL]:-?[0-9]+|V|E:java/lang/ArithmeticException)"):
    source = ROOT / "tests" / "fixtures" / "java" / f"{fixture_name}.java"
    results = ROOT / "tests" / "fixtures" / "reference" / f"{result_name}.tsv"
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="Compare fresh reference output without writing")
    parser.add_argument("--java", help="Explicit OpenJDK 21 java executable")
    args = parser.parse_args()
    java_home = os.environ.get("JAVA_HOME")
    java = args.java or (
        str(Path(java_home) / "bin" / ("java.exe" if os.name == "nt" else "java"))
        if java_home else shutil.which("java")
    )
    if not java:
        parser.error("reference JDK 21 required; set JAVA_HOME or --java")
    try:
        version_run = subprocess.run([java, "-version"], capture_output=True, text=True, check=True, timeout=30)
        version = (version_run.stdout + version_run.stderr).strip()
        if not re.match(r'(?:openjdk|java) version "21(?:[.\"+\-])', version):
            parser.error(f"reference runtime must be Java 21; got {version!r}")
        compiled = (CLASSES / "fixtures" / f"{fixture_name}.class").read_bytes()
        if compiled[:8] != bytes.fromhex("cafebabe00000041"):
            parser.error(f"compile {fixture_name} with tools/compile_fixtures.py first")
        reference = subprocess.run(
            [java, "-Dfile.encoding=UTF-8", "-cp", str(CLASSES), f"fixtures.{fixture_name}"],
            capture_output=True, text=True, encoding="utf-8", check=True, timeout=30,
        )
        if reference.stderr:
            parser.error(f"unexpected reference stderr: {reference.stderr}")
        lines = reference.stdout.splitlines()
        identities = set()
        for line in lines:
            columns = line.split("\t")
            if len(columns) != 4 or not re.fullmatch(result_pattern, columns[-1]):
                parser.error(f"invalid reference output row: {line!r}")
            identity = tuple(columns[:3])
            if identity in identities:
                parser.error(f"duplicate reference input: {identity!r}")
            identities.add(identity)
        if not lines:
            parser.error("reference emitted no cases")
        data = ("\n".join(lines) + "\n").encode("utf-8")
        metadata = {
            "reference_runtime": version,
            "source_sha256": digest(source.read_bytes()),
            "class_sha256": digest(compiled),
            "results_sha256": digest(data),
            "cases": len(lines),
        }
        metadata_path = results.with_suffix(".json")
        if args.check:
            if results.read_bytes() != data:
                parser.error("checked-in results differ from the JDK reference")
            recorded = json.loads(metadata_path.read_text(encoding="utf-8"))
            for key in ("source_sha256", "class_sha256", "results_sha256", "cases"):
                if recorded.get(key) != metadata[key]:
                    parser.error(f"reference provenance differs for {key}")
        else:
            results.parent.mkdir(parents=True, exist_ok=True)
            results.write_bytes(data)
            metadata_path.write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
        print(f"{'Verified' if args.check else 'Recorded'} {len(lines)} Java 21 {fixture_name} reference cases")
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        parser.exit(1, f"reference generation failed: {error}\n")


if __name__ == "__main__":
    main()
