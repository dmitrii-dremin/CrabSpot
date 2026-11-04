"""Reproduce original Code-validation encodings and stage-aware JDK 21 outcomes."""
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
# This classification describes the Rust contract, independently of JDK acceptance.
ACCEPT = {"DeadValid", "DeadInterface52", "HandlerValid", "UninitValid"}
DEFERRED = {"DeadTypeMismatch", "DeadUninitializedLocal"}
REJECT = {"DeadBadLdc", "DeadBadField", "DeadBadInvoke", "DeadBadLocal", "DeadBadWide",
          "DeadBadLong", "DeadInterface51", "HandlerStartMiddle", "HandlerEndMiddle",
          "HandlerTargetMiddle", "FrameMiddle", "UninitWrongSite"}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def record_or_check(path, value, check):
    if check:
        if json.loads(path.read_text(encoding="utf-8")) != value:
            raise ValueError(f"provenance differs: {path}")
    else:
        path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8", newline="\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--java")
    args = parser.parse_args()
    home = os.environ.get("JAVA_HOME")
    java = args.java or (str(Path(home) / "bin" / ("java.exe" if os.name == "nt" else "java"))
                         if home else shutil.which("java"))
    if not java:
        parser.error("reference JDK 21 required; set JAVA_HOME or --java")
    try:
        source = ROOT / "tests/fixtures/java/CodeValidationCases.java"
        source_hash = digest(read_java_source(source))
        classes = ROOT / "tests/fixtures/classes"
        compiled = json.loads((classes / "manifest.json").read_text(encoding="utf-8"))
        if compiled.get("sources", {}).get(source.name) != source_hash:
            raise ValueError("compile CodeValidationCases.java before emitting original fixtures")
        driver_hashes = {}
        for name in ("CodeValidationCases", "CodeValidationCases$Pool", "CodeValidationCases$RawLoader"):
            relative = f"fixtures/{name}.class"
            driver_hashes[relative] = digest((classes / relative).read_bytes())
            if compiled.get("files", {}).get(relative) != driver_hashes[relative]:
                raise ValueError(f"compiled driver provenance differs: {relative}")
        result = subprocess.run([java, "-version"], capture_output=True, text=True, check=True, timeout=30)
        version = (result.stdout + result.stderr).strip()
        if not re.match(r'(?:openjdk|java) version "21(?:[.\"+\-])', version):
            raise ValueError(f"reference runtime must be Java 21, got {version!r}")
        command = [java, "-Xverify:all", "-cp", str(classes), "fixtures.CodeValidationCases"]
        scratch = ROOT / "target/fixture-tool"
        scratch.mkdir(parents=True, exist_ok=True)
        destination = ROOT / "tests/fixtures/code-validation"
        with tempfile.TemporaryDirectory(prefix="code-validation-", dir=scratch) as temporary:
            subprocess.run([*command, "--emit-raw", temporary], check=True, timeout=30)
            files = {path.name: path.read_bytes() for path in sorted(Path(temporary).glob("*.class"))}
            if set(files) != {name + ".class" for name in ACCEPT | DEFERRED | REJECT}:
                raise ValueError("original fixture set differs from the 18 declared cases")
            for name, data in files.items():
                major = 51 if name == "DeadInterface51.class" else 52 if name == "DeadInterface52.class" else 65
                if data[:8] != bytes.fromhex("cafebabe0000") + major.to_bytes(2, "big"):
                    raise ValueError(f"unexpected class-file version: {name}")
            manifest = {"source": source.name, "source_sha256": source_hash,
                        "kind": "Original synthetic encodings, not javac-generated classfiles",
                        "files": {name: digest(data) for name, data in files.items()}}
            if args.check:
                if {path.name for path in destination.glob("*.class")} != set(files):
                    raise ValueError("Code-validation fixture set differs")
                for name, data in files.items():
                    if (destination / name).read_bytes() != data:
                        raise ValueError(f"Code-validation fixture differs: {name}")
            else:
                destination.mkdir(parents=True, exist_ok=True)
                for name, data in files.items():
                    (destination / name).write_bytes(data)
            record_or_check(destination / "manifest.json", manifest, args.check)
            result = subprocess.run([*command, "--probe", temporary], capture_output=True,
                                    text=True, encoding="utf-8", check=True, timeout=30)
            rows = {}
            for line in result.stdout.splitlines():
                name, stage, outcome = line.split("\t")
                if name in rows or name not in ACCEPT | DEFERRED | REJECT:
                    raise ValueError(f"unexpected/duplicate oracle case: {name}")
                if stage not in {"define", "lookup", "invoke"} or not re.fullmatch(r"I:-?\d+|[A-Za-z]+Error", outcome):
                    raise ValueError(f"unexpected oracle outcome: {line}")
                classification = "accept" if name in ACCEPT else "deferred-type-error" if name in DEFERRED else "reject"
                rows[name] = f"{name}\t{classification}\t{stage}\t{outcome}\n"
            if set(rows) != ACCEPT | DEFERRED | REJECT:
                raise ValueError("oracle case set incomplete")
            data = "".join(rows[name] for name in sorted(rows)).encode("utf-8")
            reference = ROOT / "tests/fixtures/reference/code-validation.tsv"
            if args.check:
                if reference.read_bytes() != data:
                    raise ValueError("JDK verification-stage outcomes differ")
            else:
                reference.write_bytes(data)
            provenance = {"source": source.name, "source_sha256": source_hash,
                          "driver_classes": driver_hashes, "original_classes": manifest["files"],
                          "options": ["-Xverify:all"], "cases": len(rows),
                          "results_sha256": digest(data)}
            metadata = reference.with_suffix(".json")
            if args.check:
                recorded = json.loads(metadata.read_text(encoding="utf-8"))
                if {key: recorded.get(key) for key in provenance} != provenance:
                    raise ValueError("JDK verification-stage provenance differs")
            else:
                provenance["runtime"] = version
                record_or_check(metadata, provenance, False)
        print(f"{'Verified' if args.check else 'Recorded'} 18 original Code-validation fixtures and stage-aware JDK outcomes")
        print(version)
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        parser.exit(1, f"Code-validation reference failed: {error}\n")


if __name__ == "__main__":
    main()
