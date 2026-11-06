"""Observe a passive String read followed by an active use and a public static field read."""
import argparse
import json
from pathlib import Path
import tempfile

from application_entry_reference import inventory, tool_digest
from loading_constraint_reference import reference_tool, version
from string_body_reference import FLAGS, capture_inputs, check_provenance, compile_captured, run
from type_state_reference import FIXTURES, ROOT, serialize

DIRECTORY = FIXTURES/"string-body-activation"
CLASS = "budget/Entry.class"
VALUES = {"empty": "4/0/1/1", "latin": "4/3/0/1", "pair": "4/2/0/1"}


def prepare(work, javac):
    captured = work/"captured"
    inputs = capture_inputs(DIRECTORY, captured)
    if (set(inputs["classes"]) != {CLASS} or inputs["encoded"] or
            set(inputs["src"]) != {"budget/Entry.java"} or set(inputs["controls"]) != {"Observe.java"}):
        raise ValueError("unexpected activation fixture inventory")
    apps, controls = work/"apps", work/"controls"
    compile_captured(javac, captured, inputs, apps, controls)
    compiled = inventory(apps)
    if compiled.keys() != inputs["classes"].keys() or inventory(controls).keys() != {"Observe.class"}:
        raise ValueError("unexpected activation fixture compiler output")
    return captured/"classes", apps, controls, inputs, compiled


def record(check, secondary):
    java, javac = reference_tool("java"), reference_tool("javac")
    primary, compiler = version(java), version(javac)
    parent = json.loads((FIXTURES/"string-bodies/manifest.json").read_bytes())
    if "Server VM" not in primary or "OpenJ9" in primary:
        raise ValueError("a Java 21 HotSpot JDK is required")
    if not check and (not secondary or (primary, compiler) != (parent["primary"], parent["compiler"])):
        raise ValueError("recording requires the pinned reference tools")
    vendors = [("hotspot", java, [])]
    if secondary:
        if version(secondary) != parent["secondary"]:
            raise ValueError("unexpected secondary VM")
        vendors.append(("openj9", secondary, ["-Xshareclasses:none"]))
    names = ["string_activation_reference.py", "string_body_reference.py", "application_entry_reference.py",
             "fixture_source.py", "type_state_reference.py", "loading_constraint_reference.py"]
    def tool_hashes():
        return {name: tool_digest(ROOT/"tools"/name) for name in names}
    hashes = tool_hashes()
    scratch = ROOT/"target/fixture-tool"
    scratch.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=scratch) as temp:
        original, rebuilt, controls, inputs, compiled = prepare(Path(temp), javac)
        manifests = {"original": inputs["classes"], "sources": inputs["src"] | inputs["controls"],
                     "compiled": compiled, "controls": inventory(controls), "primary": primary, "compiler": compiler,
                     "secondary": parent["secondary"], "secondary_archive_sha256": parent["secondary_archive_sha256"],
                     "flags": FLAGS, "tools": hashes}
        path = DIRECTORY/"manifest.json"
        if check:
            check_provenance(manifests, json.loads(path.read_bytes()))
        elif compiled != inputs["classes"]:
            raise ValueError("pinned javac differs from the original application")
        outputs = {}
        for name, vm, options in vendors:
            expected = json.loads((DIRECTORY/(name+".json")).read_bytes()) if check else {}
            rows = {}
            for stage, folder in (("original", original), ("rebuilt", rebuilt)):
                for arg, fields in VALUES.items():
                    row = run(vm, [folder, controls], "Observe", [arg], options)
                    if row != fields+"/java.lang.String$CaseInsensitiveComparator":
                        raise ValueError(f"{name}/{stage}/{arg}: unexpected activation result {row}")
                    rows[stage+"/"+arg] = row
            if check and rows != expected:
                raise ValueError(f"{name} activation records differ")
            outputs[name] = rows
            print(f"{name}: {len(rows)} activation observations", flush=True)
        if tool_hashes() != hashes:
            raise ValueError("activation reference tooling changed during capture")
        if not check:
            path.write_bytes(serialize(manifests))
            for name, rows in outputs.items():
                (DIRECTORY/(name+".json")).write_bytes(serialize(rows))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--compare-openj9", type=Path)
    args = parser.parse_args()
    record(args.check, args.compare_openj9)
