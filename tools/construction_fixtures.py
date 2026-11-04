"""Reproduce original Java65 construction encodings using the explicit JDK-only driver."""
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

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--java")
    args = parser.parse_args()
    home = os.environ.get("JAVA_HOME")
    java = args.java or (str(Path(home) / "bin" / ("java.exe" if os.name == "nt" else "java")) if home else shutil.which("java"))
    if not java:
        parser.error("reference JDK21 required; set JAVA_HOME or --java")
    try:
        source = ROOT / "tests/fixtures/java/ConstructionCases.java"
        source_bytes = read_java_source(source)
        compiled = json.loads((ROOT / "tests/fixtures/classes/manifest.json").read_text(encoding="utf-8"))
        source_hash = hashlib.sha256(source_bytes).hexdigest()
        if compiled.get("sources", {}).get(source.name) != source_hash:
            parser.error("compile ConstructionCases.java before emitting original fixtures")
        for name in ("ConstructionCases", "ConstructionCases$Pool", "ConstructionCases$RawLoader"):
            relative = f"fixtures/{name}.class"
            data = (ROOT / "tests/fixtures/classes" / relative).read_bytes()
            if compiled.get("files", {}).get(relative) != hashlib.sha256(data).hexdigest():
                parser.error(f"compiled construction driver provenance differs: {relative}")
        version = subprocess.run([java, "-version"], capture_output=True, text=True, check=True, timeout=30)
        version = (version.stdout + version.stderr).strip()
        if not re.match(r'(?:openjdk|java) version "21(?:[.\"+\-])', version):
            parser.error(f"reference runtime must be Java21, got {version!r}")
        scratch = ROOT / "target/fixture-tool"
        scratch.mkdir(parents=True, exist_ok=True)
        destination = ROOT / "tests/fixtures/construction"
        with tempfile.TemporaryDirectory(prefix="construction-", dir=scratch) as temporary:
            subprocess.run([java, "-Xverify:all", "-cp", str(ROOT / "tests/fixtures/classes"),
                            "fixtures.ConstructionCases", "--emit-raw", temporary], check=True, timeout=30)
            files = {path.name: path.read_bytes() for path in sorted(Path(temporary).glob("*.class"))}
            if len(files) != 9 or any(data[:8] != bytes.fromhex("cafebabe00000041") for data in files.values()):
                parser.error("expected nine original Java65 class files")
            manifest = {"source": "ConstructionCases.java", "source_sha256": source_hash,
                        "kind": "Original synthetic encodings, not javac-generated classfiles",
                        "files": {name: hashlib.sha256(data).hexdigest() for name, data in files.items()}}
            if args.check:
                if {path.name for path in destination.glob("*.class")} != set(files):
                    parser.error("construction fixture set differs")
                for name, data in files.items():
                    if (destination / name).read_bytes() != data:
                        parser.error(f"construction fixture differs: {name}")
                if json.loads((destination / "manifest.json").read_text(encoding="utf-8")) != manifest:
                    parser.error("construction fixture provenance differs")
            else:
                destination.mkdir(parents=True, exist_ok=True)
                for name, data in files.items():
                    (destination / name).write_bytes(data)
                (destination / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8", newline="\n")
        print(f"{'Verified' if args.check else 'Recorded'} nine original Java65 construction fixtures")
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        parser.exit(1, f"construction fixture generation failed: {error}\n")


if __name__ == "__main__":
    main()
