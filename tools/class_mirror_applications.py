"""Assemble closed application directories containing only the code used by each mirror case."""
import json

from class_mirror_encodings import ALLOCATIONS, CALLS, LITERALS
from class_mirror_reference import DIRECTORY, stage_bytes
from type_state_reference import digest


def stage_applications(root, directory=DIRECTORY):
    manifest = json.loads((directory/"manifest.json").read_bytes())
    groups = {}
    for folder, group in (("classes", "original"), ("encoded", "raw"), ("support", "original_support")):
        base = directory/folder
        files = {p.relative_to(base).as_posix(): p.read_bytes() for p in sorted(base.rglob("*.class"))}
        if {name: digest(data) for name, data in files.items()} != manifest[group]:
            raise ValueError(f"mirror application fixture inventory changed: {folder}")
        groups[folder] = files
    cases = {}

    def select(files, prefix):
        return {name.removeprefix(prefix): data for name, data in files.items() if name.startswith(prefix)}

    def stage(key, entry, data):
        if entry.replace(".", "/")+".class" not in data:
            raise ValueError(f"mirror application {key} lacks its entry class")
        (root/key).mkdir(parents=True, exist_ok=False)
        stage_bytes(root/key, data)
        cases[key] = (root/key, entry)

    for package, entry in (("mirrors", "Entry"), ("store", "Entry"), ("simple", "Plain")):
        apps = {name: data for name, data in groups["classes"].items() if name.startswith(package+"/")}
        stage(package, f"{package}.{entry}", apps)
    encoded = groups["encoded"]
    for case, _, _ in LITERALS:
        key = "literal/"+case
        files = select(encoded, key+"/")
        files.update(select(encoded, "literal-support/"))
        stage(key, "probe.Entry", files)
    stage("literal/missing-I", "probe.Entry", select(encoded, "literal/arr-of-I/"))
    for case in CALLS:
        key = "call/"+case
        files = select(encoded, key+"/")
        if case == "interface-object":
            files.update(groups["support"])
        elif case in ("visible-array", "through-object"):
            files["p/Face.class"] = groups["support"]["p/Face.class"]
        stage(key, "probe.Entry", files)
    for case in ALLOCATIONS:
        key = "new/"+case
        stage(key, "probe.Entry", select(encoded, key+"/"))
    return cases
