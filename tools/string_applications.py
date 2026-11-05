"""Isolate each String application from reference observers and unrelated definitions."""
import json

from string_constant_reference import CASES, DIRECTORY, archive, stage
from type_state_reference import digest


def applications(work, directory=DIRECTORY):
    apps, raw = archive(directory)
    manifest = json.loads((directory/"manifest.json").read_bytes())
    for files, group in ((apps, "original"), (raw, "encoded")):
        if {name: digest(data) for name, data in files.items()} != manifest[group]:
            raise ValueError(f"changed String application inputs: {group}")
    return stage_selected(work, apps, raw)


def stage_selected(work, apps, raw):
    cases = {}

    def add(key, main, files):
        folder = work/key
        folder.mkdir(parents=True)
        if main.replace(".", "/")+".class" not in files:
            raise ValueError(f"missing String entry: {key}")
        stage(folder, files)
        cases[key] = folder, main

    for package, entry in (("texts", "Entry"), ("bare", "Literal"), ("words", "Entry"),
                           ("transport", "Entry"), ("constants", "Entry"), ("locks", "Strings"),
                           ("duplicates", "Entry")):
        files = {key: data for key, data in apps.items() if key.startswith(package+"/")}
        add(package, package+"."+entry, files)
    for label in CASES:
        name = "sample/Value.class"
        add("raw/"+label, "sample.Entry", {name: raw[label+"/"+name], "sample/Entry.class": apps["sample/Entry.class"]})
    return cases
