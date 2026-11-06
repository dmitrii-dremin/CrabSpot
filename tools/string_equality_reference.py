"""Observe Java String equality and bootstrap assertion policy in independent JVMs."""
import argparse
import json
from pathlib import Path
import tempfile

from application_entry_reference import inventory, tool_digest
from loading_constraint_reference import reference_tool, version
from string_body_reference import FLAGS, capture_inputs, check_provenance, compile_captured, run
from type_state_reference import FIXTURES, ROOT, serialize

MODES = {
    "default": [], "system-on": ["-esa"], "system-off": ["-dsa"],
    "app-on": ["-ea"], "app-class-on": ["-ea:app.Policy"],
    "helper-on": ["-ea:java.lang.StringLatin1"], "all": ["-ea", "-esa"],
    "all-except-app-class": ["-ea", "-esa", "-da:app.Policy"],
    "system-except-helper": ["-esa", "-da:java.lang.StringLatin1"],
    "system-reset": ["-esa", "-dsa"],
}
APPS = {"app/BootstrapStatus", "app/Policy", "app/TextEquality"}
CONTROLS = {"ReadEquality", "ReadPolicy", "PlatformIdentity"}


def expected(vendor, mode):
    # Both JVMs agree on these public ordinary-class observations.
    system = mode in {"system-on", "all", "all-except-app-class", "system-except-helper"}
    app = mode in {"app-on", "app-class-on", "all"}
    helper = mode in {"system-on", "all", "all-except-app-class", "helper-on"}
    # Compare actual private startup state and array assertions separately for each implementation.
    if vendor == "openj9":
        helper_disabled = True
        app_array = mode in {"app-on", "all", "all-except-app-class"}
        arrays = f"{int(system)}/{int(app_array)}/{int(system)}"
        fields = "not-reflectively-present,not-reflectively-present"
    else:
        helper_disabled = not helper
        arrays = "0/0/0"
        fields = "[Ljava.lang.Class;/0,[Ljava.io.ObjectStreamField;/0"
    return [
        "511/10",
        f"policy={3*int(system)}/2/{int(app)}/{int(app)}/{int(helper)}/{int(helper_disabled)}",
        "loaders=1/1/1/1", "arrays=" + arrays, "class-fields=" + fields,
        "child=1/1/1/0/2", "child=1/0/0/1/0",
    ]


def portable(rows):
    equality, policy, loaders, _, _, child_on, child_off = rows
    return [equality, "/".join(policy.split("/")[:4]), loaders, child_on, child_off]


def validate(rows, pinned_rows, exact):
    if rows != (pinned_rows if exact else portable(pinned_rows)):
        raise ValueError(f"unexpected {'pinned' if exact else 'portable'} assertion/equality results: {rows}")


def recording_pins(directory, observed):
    path = directory/"manifest.json"
    if path.exists() and json.loads(path.read_bytes())["platform_classes"] != observed:
        raise ValueError("supplied java.base identities differ from the archived recording pins")
    return observed


def prepare(directory, work, javac):
    captured = work/"captured"
    inputs = capture_inputs(directory, captured)
    for group, stems, suffix in (("src", APPS, ".java"), ("classes", APPS, ".class"),
                                 ("controls", CONTROLS, ".java"), ("encoded", set(), ".class")):
        if set(inputs[group]) != {stem + suffix for stem in stems}:
            raise ValueError(f"unexpected equality fixture {group} inventory")
    rebuilt, controls = work/"rebuilt", work/"control-classes"
    compile_captured(javac, captured, inputs, rebuilt, controls)
    if set(inventory(rebuilt)) != set(inputs["classes"]) or set(inventory(controls)) != {s+".class" for s in CONTROLS}:
        raise ValueError("unexpected equality compiler output inventory")
    return inputs, captured/"classes", rebuilt, controls


def record(directory, check, other, portable_primary=False):
    java, javac = reference_tool("java"), reference_tool("javac")
    pin = json.loads((FIXTURES/"string-bodies/manifest.json").read_bytes())
    primary, compiler = version(java), version(javac)
    if "Server VM" not in primary or "OpenJ9" in primary:
        raise ValueError("select a Java 21 HotSpot JDK")
    if not check and (not other or portable_primary or (primary, compiler) != (pin["primary"], pin["compiler"])):
        raise ValueError("recording requires the pinned Java 21 reference tools")
    vendors = [("hotspot", java, [])]
    if other:
        if version(other) != pin["secondary"]:
            raise ValueError("unexpected OpenJ9 runtime")
        vendors.append(("openj9", other, ["-Xshareclasses:none"]))
    tools = [ROOT/"tools"/(s+".py") for s in ("string_body_reference", "application_entry_reference",
             "fixture_source", "loading_constraint_reference", "type_state_reference")] + [Path(__file__)]
    def hashes():
        return {p.name: tool_digest(p) for p in tools}
    scripts = hashes()
    scratch = ROOT/"target/fixture-tool"
    scratch.mkdir(exist_ok=True, parents=True)
    with tempfile.TemporaryDirectory(dir=scratch) as tmp:
        inputs, original, rebuilt, controls = prepare(directory, Path(tmp), javac)
        identities = {v: run(vm, [controls], "PlatformIdentity", [], opts).splitlines() for v,vm,opts in vendors}
        if not check:
            for vendor, classes in identities.items():
                parent = json.loads((FIXTURES/"string-bodies"/(vendor+"-layout.json")).read_bytes())
                if classes[0] != "String=" + parent["classes"]["java/lang/String.class"]:
                    raise ValueError(f"{vendor} String differs from the pinned archive")
        archived = json.loads((directory/"manifest.json").read_bytes()) if check else None
        manifest = {"sources": {k+"/"+n:h for k in ("src","controls") for n,h in inputs[k].items()},
                    "original": inputs["classes"], "compiled": inventory(rebuilt), "controls": inventory(controls),
                    "flags": FLAGS, "modes": MODES, "tools": scripts, "primary": primary, "compiler": compiler,
                    "secondary": pin["secondary"], "secondary_archive_sha256": pin["secondary_archive_sha256"],
                    "platform_classes": archived["platform_classes"] if check else recording_pins(directory, identities)}
        if check:
            check_provenance(manifest, archived)
        elif inputs["classes"] != manifest["compiled"]:
            raise ValueError("pinned javac differs from archived equality classes")
        outputs = {}
        for vendor, vm, prefix in vendors:
            exact = (identities[vendor] == manifest["platform_classes"][vendor] and
                     (vendor == "openj9" or primary == pin["primary"] and not portable_primary))
            if vendor == "openj9" and not exact:
                raise ValueError("OpenJ9 java.base class hashes differ from the pinned runtime")
            if not check and not exact:
                raise ValueError("recording requires the pinned platform classes")
            rows = {}
            for stage, app in (("original", original), ("rebuilt", rebuilt)):
                for mode, switches in MODES.items():
                    opens = ["--add-opens", "java.base/java.lang=ALL-UNNAMED"] if exact else []
                    args = [app] if exact else [app, "portable"]
                    lines = run(vm, [controls, app], "ReadPolicy", args, [*prefix, *switches, *opens]).splitlines()
                    reference = expected(vendor, mode)
                    validate(lines, reference, exact)
                    rows[stage+"/"+mode] = lines
                if vendor == "hotspot":
                    outcome = run(vm, [controls, app], "ReadEquality", [], ["-XX:-CompactStrings"])
                    if outcome != "511/10":
                        raise ValueError(f"{stage} noncompact equality: {outcome}")
                    rows[stage+"/noncompact"] = [outcome]
            if check:
                saved = json.loads((directory/(vendor+".json")).read_bytes())
                if not exact:
                    saved = {k: v if k.endswith("/noncompact") else portable(v) for k,v in saved.items()}
                if rows != saved:
                    raise ValueError(f"{vendor} recorded observations differ")
            outputs[vendor] = rows
            print(f"{vendor}: {len(rows)} {'pinned' if exact else 'portable'} original and rebuilt observations", flush=True)
        if hashes() != scripts:
            raise ValueError("reference tool changed during measurement")
        if not check:
            for name, obj in {"manifest":manifest, **outputs}.items():
                (directory/(name+".json")).write_bytes(serialize(obj))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--compare-openj9", type=Path)
    parser.add_argument("--portable-primary", action="store_true",
                        help="also exercise the public-only observer on a pinned primary VM")
    args = parser.parse_args()
    record(FIXTURES/"string-equality", args.check, args.compare_openj9, args.portable_primary)
