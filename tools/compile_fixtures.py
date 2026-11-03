"""Compile genuine Java 21 parser fixtures; never execute application bytecode."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[1]
SOURCES = ROOT / "tests" / "fixtures" / "java"
DESTINATION = ROOT / "tests" / "fixtures" / "classes"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--javac", help="Explicit reference JDK 21 javac executable")
    parser.add_argument("--check", action="store_true", help="Check reproducibility without writing fixtures")
    args = parser.parse_args()
    java_home = os.environ.get("JAVA_HOME")
    executable = args.javac or (
        str(Path(java_home) / "bin" / ("javac.exe" if os.name == "nt" else "javac"))
        if java_home else shutil.which("javac")
    )
    if not executable:
        parser.error("JDK 21 javac is required; set JAVA_HOME or --javac")
    try:
        result = subprocess.run([executable, "-version"], capture_output=True, text=True, check=True)
        version = (result.stdout + result.stderr).strip()
        if not re.fullmatch(r"javac 21(?:[. +\-].*)?", version):
            parser.error(f"reference compiler must be JDK 21; found {version!r}")
        sources = sorted(SOURCES.rglob("*.java"))
        scratch = ROOT / "target" / "fixture-tool"
        scratch.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="compile-", dir=scratch) as temporary:
            output = Path(temporary)
            subprocess.run(
                [executable, "--release", "21", "-encoding", "UTF-8", "-g", "-parameters",
                 "-d", str(output), *map(str, sources)], check=True
            )
            compiled = sorted(output.rglob("*.class"))
            if not compiled:
                parser.error("javac produced no class files")
            manifest = {
                "compiler": version,
                "release": 21,
                "flags": ["-encoding", "UTF-8", "-g", "-parameters"],
                "sources": {p.relative_to(SOURCES).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest() for p in sources},
                "files": {},
            }
            for path in compiled:
                data = path.read_bytes()
                relative = path.relative_to(output)
                if data[:8] != bytes.fromhex("cafebabe00000041"):
                    parser.error(f"{relative}: not a non-preview Java 21 class file")
                manifest["files"][relative.as_posix()] = hashlib.sha256(data).hexdigest()
                destination = DESTINATION / relative
                if args.check:
                    if not destination.is_file() or destination.read_bytes() != data:
                        parser.error(f"{relative}: checked-in fixture differs from fresh compilation")
                else:
                    destination.parent.mkdir(parents=True, exist_ok=True)
                    destination.write_bytes(data)
            if args.check:
                existing = {p.relative_to(DESTINATION).as_posix() for p in DESTINATION.rglob("*.class")}
                if existing != set(manifest["files"]):
                    parser.error("checked-in fixture set differs from compiler output")
                recorded = json.loads((DESTINATION / "manifest.json").read_text(encoding="utf-8"))
                if recorded.get("sources") != manifest["sources"] or recorded.get("files") != manifest["files"]:
                    parser.error("fixture manifest digests differ from source or class files")
            else:
                (DESTINATION / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
            print(f"{'Verified' if args.check else 'Compiled'} {len(compiled)} Java 21 class files using {version}")
    except (OSError, subprocess.CalledProcessError, ValueError) as error:
        parser.exit(1, f"fixture compilation failed: {error}\n")


if __name__ == "__main__":
    main()
