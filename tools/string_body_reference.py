"""Reference Java library methods and reciprocal nest access on archived and recompiled classes."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile

from application_entry_reference import inventory, tool_digest
from fixture_source import read_java_source
from loading_constraint_reference import reference_tool, version
from string_body_nests import CASES, make
from type_state_reference import FIXTURES, ROOT, digest, serialize


DIRECTORY = Path(__file__).resolve().parents[1]/"tests/fixtures/string-bodies"
FLAGS = ["--release", "21", "-g:none", "-encoding", "UTF-8"]
SHARED = ("application_entry_reference.py", "fixture_source.py", "loading_constraint_reference.py",
          "type_state_encodings.py", "type_state_reference.py")


def capture_inputs(directory, work):
    inventories = {}
    for group, pattern in (("src", "*.java"), ("controls", "*.java"), ("classes", "*.class"), ("encoded", "*.class")):
        source = directory/group
        hashes = {}
        for path in sorted(source.rglob(pattern)):
            name = path.relative_to(source).as_posix()
            data = read_java_source(path) if pattern == "*.java" else path.read_bytes()
            dst = work/group/name
            dst.parent.mkdir(exist_ok=True, parents=True)
            dst.write_bytes(data)
            hashes[name] = digest(data)
        inventories[group] = hashes
    return inventories


def compile_captured(javac, captured, inventories, apps, controls):
    for group, output, extra in (("src", apps, []), ("controls", controls, ["-cp", apps])):
        paths = [captured/group/name for name in inventories[group]]
        subprocess.run([javac, *FLAGS, *extra, "-d", output, *paths], check=True, timeout=90)


def check_provenance(current, archived):
    if current.keys() != archived.keys():
        raise ValueError("String-body manifest schema differs")
    variable = {"compiler", "primary", "compiled", "controls"}
    for key in current.keys() - variable:
        if current[key] != archived[key]:
            raise ValueError(f"fixture provenance changed: {key}")
    pinned = all(current[key] == archived[key] for key in ("compiler", "primary"))
    for key in ("compiled", "controls"):
        if current[key].keys() != archived[key].keys() or pinned and current[key] != archived[key]:
            raise ValueError(f"unexpected javac output: {key}")


def run(java, cp, main, args, options):
    result = subprocess.run([java, *options, "-Xverify:all", "-cp", os.pathsep.join(map(str, cp)), main, *args],
                            capture_output=True, timeout=60)
    if result.returncode or result.stderr:
        raise ValueError(f"{main}: {result.returncode} {result.stderr!r}")
    return result.stdout.decode("ascii").replace("\r\n", "\n").removesuffix("\n")


def observations(java, options, apps, encoded, controls):
    cp = [controls, apps]
    rows = {}

    def expect(label, main, args, expected, paths=cp):
        actual = run(java, paths, main, args, options)
        if actual != expected:
            raise ValueError(f"{label}: expected {expected!r}, observed {actual!r}")
        rows[label] = actual

    expect("strings/empty", "Observe", [], "63/0/3/4/1/2/0")
    expect("strings/arguments", "Observe", ["two"], "127/0/3/4/1/2/3")
    expect("strings/empty-only", "ObserveEmpty", [], "3")
    expect("strings/empty-argument", "ObserveEmpty", [""], "7")
    expect("nest/javac", "NestOutcome", [], "returned/7")
    expect("nest/split-loader", "Split", [apps], "true/true/41\nfalse/false/IllegalAccessError", [controls])
    for label, *_ in CASES:
        member = label in {"genuine-names", "extra-missing"}
        host = "peer.Host" if member else "peer.Other" if label == "other-host" else "peer.Host$Inner"
        access = "returned/7" if member else "IllegalAccessError/1"
        paths = [controls, encoded/label]
        expect(label+"/access", "NestOutcome", [], access, paths)
        expect(label+"/prior-query", "NestOutcome", ["query"], f"{host}/{str(member).lower()}/{access}", paths)
    return rows


def record(check, secondary):
    java, javac = reference_tool("java"), reference_tool("javac")
    primary = version(java)
    compiler = version(javac)
    if "Server VM" not in primary or "OpenJ9" in primary:
        raise ValueError("select Java 21 HotSpot as the primary")
    pin = json.loads((FIXTURES/"loading-constraints/openj9/manifest.json").read_bytes())
    vendors = [("hotspot", java, [])]
    if secondary:
        if version(secondary) != pin["runtime"]:
            raise ValueError("unexpected secondary JVM")
        vendors.append(("openj9", secondary, ["-Xshareclasses:none"]))
    elif not check:
        raise ValueError("recording requires both pinned JVMs")
    pinned_pair = json.loads((FIXTURES/"string-constants/manifest.json").read_bytes())
    if not check and (primary, compiler) != (pinned_pair["primary"], pinned_pair["compiler"]):
        raise ValueError("recording requires the pinned HotSpot runtime and javac")
    scripts = [ROOT/"tools"/name for name in SHARED] + [Path(__file__), Path(__file__).with_name("string_body_nests.py")]
    tool_hashes = {p.name: tool_digest(p) for p in scripts}
    scratch = ROOT/"target/fixture-tool"
    scratch.mkdir(exist_ok=True, parents=True)
    with tempfile.TemporaryDirectory(dir=scratch) as tmp:
        work = Path(tmp)
        apps, controls = work/"apps", work/"controls"
        captured = work/"archived"
        inputs = capture_inputs(DIRECTORY, captured)
        sources = {group+"/"+name: sha for group in ("src", "controls") for name, sha in inputs[group].items()}
        compile_captured(javac, captured, inputs, apps, controls)
        for case in CASES:
            make(work, *case)
        compiled = inventory(apps)
        original = inputs["classes"]
        encoded = inventory(work/"nest-encoded")
        if original.keys() != compiled.keys() or inputs["encoded"] != encoded:
            raise ValueError("String-body application inventory changed")
        results, platforms = {}, {}
        for vendor, vm, flags in vendors:
            for source, classes, synthetic in [("original", captured/"classes", captured/"encoded"),
                                               ("rebuilt", apps, work/"nest-encoded")]:
                rows = observations(vm, flags, classes, synthetic, controls)
                if source == "rebuilt" and rows != results[vendor]:
                    raise ValueError("rebuilt String observations differ")
                results[vendor] = rows
                print(f"{vendor} {source}: {len(rows)} Java String and nest observations", flush=True)
            # Compare the public Java length controls on both HotSpot backing policies.
            if vendor == "hotspot":
                for argv, key in [([], "empty"), (["two"], "arguments")]:
                    if run(vm, [controls, apps], "Observe", argv, flags+["-XX:-CompactStrings"]) != rows["strings/"+key]:
                        raise ValueError("String lengths changed with the compact backing policy")
            runtime_classes = work/("platform-"+vendor)
            hashes = run(vm, [controls], "ClassBytes", [runtime_classes], flags)
            for line in hashes.splitlines():
                cls, length, sha = line.split()
                data = (runtime_classes/(cls+".class")).read_bytes()
                if len(data) != int(length) or digest(data) != sha:
                    raise ValueError(f"incorrect {vendor} jrt class identity for {cls}")
            platform = {"classes": inventory(runtime_classes)}
            # Private layout reports belong to the identified runtime image.
            # Other implementations still run all public behavior checks above.
            known = {"d4f87fcde6a7a0f4fea37f7c56e29279e94a9da55ccb41a6155d68cb44fe25d0",
                     "25bc3e38a28f35a537715c22ea57abe9c78db5c630535cc603345b92f39278ce"}
            if platform["classes"]["java/lang/String.class"] in known:
                modes = [("default", [])]
                if vendor == "hotspot":
                    modes.append(("wide", ["-XX:-CompactStrings"]))
                for name, mode in modes:
                    platform[name] = run(vm, [controls], "Layout", [],
                                         flags+mode+["--add-opens", "java.base/java.lang=ALL-UNNAMED"])
            platforms[vendor] = platform
        if tool_hashes != {p.name: tool_digest(p) for p in scripts}:
            raise ValueError("reference tooling changed during capture")
        provenance = {"primary": primary, "compiler": compiler, "secondary": pin["runtime"],
                      "secondary_archive_sha256": pin["archive_sha256"], "flags": FLAGS,
                      "sources": sources, "original": original, "compiled": compiled, "controls": inventory(controls),
                      "encoded": encoded, "tools": tool_hashes}
        path = DIRECTORY/"manifest.json"
        if check:
            base = json.loads(path.read_bytes())
            check_provenance(provenance, base)
            for vendor, rows in results.items():
                if json.loads((DIRECTORY/(vendor+".json")).read_bytes()) != rows:
                    raise ValueError(f"observations changed for {vendor}")
                pinned = vendor == "openj9" or primary == base["primary"]
                if pinned and platforms[vendor] != json.loads((DIRECTORY/(vendor+"-layout.json")).read_bytes()):
                    raise ValueError(f"{vendor} runtime classes or private layout differ")
        else:
            if original != compiled:
                raise ValueError("recording requires a compiler matching archived definitions")
            path.write_bytes(serialize(provenance))
            for vendor, rows in results.items():
                (DIRECTORY/(vendor+".json")).write_bytes(serialize(rows))
                (DIRECTORY/(vendor+"-layout.json")).write_bytes(serialize(platforms[vendor]))


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--check", action="store_true")
    p.add_argument("--compare-openj9", type=Path)
    args = p.parse_args()
    record(args.check, args.compare_openj9)
