"""Replay the Java 21 HotSpot loader probes, optionally comparing the recorded OpenJ9 build."""

import argparse
from collections import Counter
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

from loading_constraint_encodings import build, selection_edges
from fixture_source import read_java_source
from type_state_reference import FIXTURES, ROOT, check_or_write, digest, serialize


DIRECTORY = FIXTURES / "loading-constraints"
MATRICES = {"timing": 45, "selection": 18, "transitive": 8, "packages": 7, "inherited": 1, "method-order": 6}


def reference_tool(name):
    home = os.environ.get("JAVA_HOME")
    path = shutil.which(name, path=str(Path(home) / "bin") if home else None)
    if path is None:
        raise ValueError(f"missing JDK 21 reference tool {name}")
    return path


def version(tool):
    result = subprocess.run([tool, "-version"], capture_output=True, text=True, check=True, timeout=30)
    output = (result.stdout + result.stderr).strip()
    if not re.match(r'(?:javac |(?:openjdk|java) version ")21(?:[."+\-]|$)', output):
        raise ValueError(f"JDK 21 required: {output}")
    return output


def compare_originals(files, rows, origins):
    """The refactored emitter and driver must reproduce every original byte and staged row."""
    for matrix, record in origins["matrices"].items():
        for name, checksum in record["inputs"].items():
            if digest(files[name]) != checksum:
                raise ValueError(f"original class bytes differ: {matrix}/{name}")
        original = []
        for row in rows:
            if row[0] != matrix:
                continue
            columns = row[1:]
            if matrix == "transitive":
                # The original driver reported outcomes but omitted these explicit trigger stages.
                columns = columns[:-2] if columns[0] == "transitive" else columns[:-1]
            original.append("\t".join(columns) + "\n")
        if len(original) != record["rows"] or digest("".join(original).encode()) != record["normalized_results_sha256"]:
            raise ValueError(f"original JDK stages differ: {matrix}")


def normalize(rows):
    counts = Counter(row[0] for row in rows)
    if counts != MATRICES:
        raise ValueError(f"unexpected JDK matrix counts {dict(counts)}")
    ordered = []
    seen = set()
    for row in rows:
        if row[0] == "method-order":
            if (len(row) != 6 or row[2:5] != ["initialize", "LinkageError", "A"]
                    or row[5] not in {"B", "LinkageError"} or row[1] in seen):
                raise ValueError(f"unexpected partial result after competing method obligations: {row}")
            seen.add(row[1])
            # HotSpot's method-table traversal can differ between launches of the same build.
            # The full native observations are preserved separately in the development evidence.
            row = [*row[:5], "{B,LinkageError}"]
        ordered.append("\t".join(row))
    if seen != {f"Order{i}{j}" for i in range(3) for j in range(2)}:
        raise ValueError(f"method-order matrix changed: {seen}")
    return ("\n".join(ordered) + "\n").encode()


def check_selection(row, old, contract, invoke):
    expected_width = 8 if invoke else 6
    if len(row) != expected_width or len(old) != expected_width or row[:2] != old[:2]:
        raise ValueError(f"invalid selection observation: {row}")
    kind, mode, first, outcome = row[:4]
    left, right, relation = contract
    if relation not in {"equal", "independent"}:
        raise ValueError(f"unknown preparation relation for {kind}: {relation}")
    conflict = mode == "split" and relation == "equal"
    after = right
    if mode == "shared":
        after = left
    elif mode == "late" and relation == "equal":
        after = "LinkageError"
    stages = {"define", "initialize"} if conflict else {"initialize"}
    agrees = (outcome == ("LinkageError" if conflict else "ok")
              and row[-2:] == [left, after] and first in stages)
    # A reference JVM may agree with SE 21 or reproduce the recorded divergence.
    if not agrees and row != old:
        raise ValueError(f"unclassified selection stage difference: {row}")
    if invoke:
        expected_last = old[4:6] if outcome == "ok" else [first, outcome]
        if row[4:6] != expected_last:
            raise ValueError(f"unexpected invocation outcome: {row}")
    return agrees


def run_selection(java, driver, stage, origins, runtime, check):
    contracts = {}
    for line in (DIRECTORY / "selection-contract.tsv").read_text(encoding="utf-8").splitlines():
        kind, left, right, relation = line.split("\t")
        if kind in contracts:
            raise ValueError(f"duplicate selection contract {kind}")
        contracts[kind] = (left, right, relation)
    for variant in ("prepare", "invoke"):
        invoke = variant == "invoke"
        files = selection_edges(invoke)
        original = origins["selection_edges"][variant]
        if {name: digest(data) for name, data in files.items()} != original["input_classes"]:
            raise ValueError(f"{variant}: original edge encodings differ")
        classes = stage / variant
        for name, data in files.items():
            check_or_write(classes / (name + ".class"), data, False)
        if not invoke:
            retained = DIRECTORY / "selection-classes"
            expected = {name + ".class" for name in files}
            existing = {p.relative_to(retained).as_posix() for p in retained.rglob("*") if p.is_file()}
            if existing - expected or (check and existing != expected):
                raise ValueError("selection fixture set differs")
            for name, data in files.items():
                check_or_write(retained / (name + ".class"), data, check)
        raw = subprocess.run([java, "-Xverify:all", "-cp", driver, "LoadingSelectionEdges", classes, variant],
                             check=True, capture_output=True, text=True, encoding="utf-8", timeout=60).stdout
        baseline = (DIRECTORY / f"selection-{variant}.tsv").read_text(encoding="utf-8")
        if digest(baseline.encode()) != original["normalized_results_sha256"]:
            raise ValueError(f"{variant}: recorded native observations differ from original")
        if runtime == origins["java"] and raw != baseline:
            raise ValueError(f"{variant}: same-build JVM stage observations changed")
        actual = [line.split("\t") for line in raw.splitlines()]
        recorded = [line.split("\t") for line in baseline.splitlines()]
        modes = ["split", "shared"] if invoke else ["split", "late", "shared"]
        if [(r[0], r[1]) for r in actual] != [(kind, mode) for kind in contracts for mode in modes]:
            raise ValueError(f"{variant}: incomplete reference observations")
        differences = 0
        for row, old in zip(actual, recorded, strict=True):
            kind, mode = row[:2]
            if not check_selection(row, old, contracts[kind], invoke):
                differences += 1
            if row != old:
                print(f"{variant} {kind} {mode}: {' '.join(row[2:])} (recorded {' '.join(old[2:])})")
        print(f"JDK {variant}: {len(actual)} observations, {differences} upstream loading outcomes differ from SE 21")


def check_secondary(stage, driver, java, java_sources, driver_digests, compiler):
    folder = DIRECTORY / "openj9"
    metadata = json.loads((folder / "manifest.json").read_text(encoding="utf-8"))
    if metadata["driver_sources"] != {name: digest(data) for name, data in java_sources.items()}:
        raise ValueError("OpenJ9 driver source provenance differs")
    if compiler == metadata["compiler"] and driver_digests != metadata["driver_classes"]:
        raise ValueError("OpenJ9 same-compiler driver class provenance differs")
    if java and version(java) != metadata["runtime"]:
        raise ValueError("the pinned OpenJ9 runtime from the evidence manifest is required for replay")
    for matrix, count in (("main", 85), ("prepare", 27), ("invoke", 18)):
        path = folder / (matrix + ".tsv")
        data = path.read_bytes()
        lines = data.decode().splitlines()
        if len(lines) != count or metadata["observations"][path.name] != {"rows": count, "sha256": digest(data)}:
            raise ValueError(f"recorded OpenJ9 {matrix} stages differ")
        if java:
            args = (["LoadingConstraintProbe", stage / "classes"] if matrix == "main"
                    else ["LoadingSelectionEdges", stage / matrix, matrix])
            actual = subprocess.run([java, "-Xshareclasses:none", "-Xverify:all", "-cp", driver, *args],
                                    check=True, capture_output=True, text=True, encoding="utf-8", timeout=60)
            for old, current in zip(lines, actual.stdout.splitlines(), strict=True):
                # Competing methods may have either legal surviving prefix on a subsequent run.
                if old.startswith("method-order\t"):
                    prefix, leaf = current.rsplit("\t", 1)
                    if prefix == old.rsplit("\t", 1)[0] and leaf in {"B", "LinkageError"}:
                        continue
                if current != old:
                    raise ValueError(f"pinned OpenJ9 {matrix} stage changed: {current} (recorded {old})")
    if java:
        print("Replayed all 130 stages on the pinned Eclipse OpenJ9 build")


def record(check, secondary_java=None):
    java_sources = {name: read_java_source(DIRECTORY / name)
                    for name in ("LoadingConstraintProbe.java", "LoadingSelectionEdges.java")}
    java, javac = reference_tool("java"), reference_tool("javac")
    runtime, compiler = version(java), version(javac)
    files = build()
    stage_root = ROOT / "target/fixture-tool"
    stage_root.mkdir(parents=True, exist_ok=True)
    origins_path = DIRECTORY / "development.json"
    origins = json.loads(origins_path.read_text(encoding="utf-8"))
    with tempfile.TemporaryDirectory(prefix="loading-constraints-", dir=stage_root) as tmp:
        stage = Path(tmp)
        classes = stage / "classes"
        for name, data in files.items():
            check_or_write(classes / (name + ".class"), data, False)
        driver = stage / "driver"
        driver.mkdir()
        for name, data in java_sources.items():
            (stage / name).write_bytes(data)
        subprocess.run([javac, "--release", "21", "-encoding", "UTF-8", "-d", driver,
                        *(stage / name for name in java_sources)],
                       check=True, timeout=60)
        result = subprocess.run([java, "-Xverify:all", "-cp", driver, "LoadingConstraintProbe", classes],
                                check=True, capture_output=True, text=True, encoding="utf-8", timeout=60)
        driver_digests = {p.name: digest(p.read_bytes()) for p in driver.glob("*.class")}
        run_selection(java, driver, stage, origins, runtime, check)
        check_secondary(stage, driver, secondary_java, java_sources, driver_digests, compiler)
    rows = [line.split("\t") for line in result.stdout.splitlines()]
    compare_originals(files, rows, origins)
    results = normalize(rows)
    inputs = DIRECTORY / "classes"
    existing = {p.relative_to(inputs).as_posix() for p in inputs.rglob("*") if p.is_file()}
    expected = {name + ".class" for name in files}
    if existing - expected or (check and existing != expected):
        raise ValueError("loading-constraint class input set differs")
    for name, data in files.items():
        check_or_write(inputs / (name + ".class"), data, check)
    check_or_write(DIRECTORY / "results.tsv", results, check)
    sources = [DIRECTORY / "selection-contract.tsv", Path(__file__),
               ROOT / "tools/loading_constraint_encodings.py", ROOT / "tools/type_state_encodings.py"]
    manifest = {
        "sources": {**{name: digest(data) for name, data in java_sources.items()},
                    **{p.name: digest(p.read_bytes()) for p in sources}},
        "development_sha256": digest(origins_path.read_bytes()),
        "openj9_manifest_sha256": digest((DIRECTORY / "openj9/manifest.json").read_bytes()),
        "classes": {name: digest(data) for name, data in files.items()},
        "matrix_counts": MATRICES, "results_sha256": digest(results),
        "javac_options": ["--release", "21", "-encoding", "UTF-8"], "java_options": ["-Xverify:all"],
    }
    metadata = DIRECTORY / "manifest.json"
    if check:
        stored = json.loads(metadata.read_text(encoding="utf-8"))
        if stored["inputs"] != manifest:
            raise ValueError("loading-constraint source or result provenance differs")
        if stored["compiler"] == compiler and stored["runtime"] == runtime and stored["driver_classes"] != driver_digests:
            raise ValueError("same-build loading-constraint driver differs")
    else:
        metadata.write_bytes(serialize({"compiler": compiler, "runtime": runtime, "driver_classes": driver_digests,
                                        "inputs": manifest}))
    print(f"Matched {len(files)} original class encodings and {len(rows)} loading-constraint stage observations")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="verify committed inputs and HotSpot stage observations")
    parser.add_argument("--compare-openj9", type=Path, metavar="JAVA",
                        help="also replay the separately recorded IBM Semeru 21.0.13.0 Windows outcomes")
    args = parser.parse_args()
    record(args.check, args.compare_openj9)
