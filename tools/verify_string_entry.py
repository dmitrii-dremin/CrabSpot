"""Run the Java 21 String applications through CrabSpot's public API and OS launcher."""
import argparse
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

from loading_constraint_reference import reference_tool
from string_applications import applications, stage_selected
from string_constant_reference import CASES, DIRECTORY, ENTRY_ARGS, ROOT, compile_inputs, literal_class, stage
from type_state_encodings import ClassEncoding, op
from verify_application_entry import extract_classes
from verify_java_base import pack_archive


def process(command, code=0, diagnostic=""):
    result = subprocess.run(command, capture_output=True, timeout=180)
    error = result.stderr.decode()
    if result.returncode != code or (diagnostic not in error if diagnostic else bool(error)):
        raise ValueError(f"{command!r}: exit {result.returncode}, stderr={error!r}")
    return result.stdout.decode()


def physical_entry(destination, original, main, owner, expected):
    """Install a separate checked main which divides by zero if an observed field differs."""
    shutil.copytree(original, destination)
    c = ClassEncoding("physical/Entry")
    descriptor = "([Ljava/lang/String;)V"
    code = b"\x2a" + op(0xb8, c.ref(10, main.replace(".", "/"), "main", descriptor))
    for field, value in expected:
        code += b"\x04" + op(0xb2, c.ref(9, owner, field, "I")) + bytes((0x10, value, 0x82))
        # With x = actual ^ expected, 1 / (1 ^ ((x | -x) >>> 31)) succeeds only for x == 0.
        code += bytes.fromhex("59 74 80 10 1f 7c 04 82 6c 57")
    stage(destination, {"physical/Entry.class": c.finish([], [(9, "main", descriptor, code+b"\xb1", 3, 1)])})


def later_inputs(work, javac):
    raw = {label+"/sample/Value.class": literal_class(units, opcode) for label, (units, opcode) in CASES.items()}
    stage(work/"raw", raw)
    compile_inputs(javac, DIRECTORY/"src", work, work/"raw")
    apps = {p.relative_to(work/"apps").as_posix(): p.read_bytes() for p in (work/"apps").rglob("*.class")}
    return stage_selected(work/"cases", apps, raw)


def run_suite(bin_dir, platform, cases, work, original):
    suffix = ".exe" if os.name == "nt" else ""
    public = bin_dir/"examples"/("replay_string_entry"+suffix)
    cli = bin_dir/("crabspot"+suffix)
    count = 0
    closure = None

    def api(case, argset, outcome, fields, usage, *caps):
        nonlocal closure, count
        folder, main = cases[case]
        rows = process([public, platform, folder, main, argset, *caps]).splitlines()
        data = rows.pop().split()
        keys = ("arrays", "args", "evaluations", "objects", "units", "interned", "sites", "work", "proved")
        if [s.partition("=")[0] for s in data] != list(keys):
            raise ValueError(f"{case} malformed usage {data}")
        values = [int(s.partition("=")[2]) for s in data]
        expected = [outcome] if fields is None else [outcome, "fields="+fields]
        if rows != expected or tuple(values[:-1]) != usage:
            raise ValueError(f"{case}/{argset} {caps}: expected {expected} {usage}, got {rows} {values}")
        if outcome.startswith("image Input"):
            if values[-1]:
                raise ValueError("argument validation consumed a class proof")
        elif not outcome.startswith("image Verification"):
            extra = values[-1] - len(list(folder.rglob("*.class")))
            if extra < 7 or closure is not None and closure != extra:
                raise ValueError(f"{case}: inconsistent platform proof count {extra}")
            closure = extra
        count += 1

    def launch(case, args=(), *, failure=None):
        folder, main = cases[case]
        cmd = [cli, "run", "--platform", platform, "--classes", folder, main, *args]
        if process(cmd, 1 if failure else 0, failure or ""):
            raise ValueError(f"{case} produced unexpected stdout")

    def physical(label, source, owner, fields, args):
        folder, main = cases[source]
        physical_entry(work/label, folder, main, owner, fields)
        cases[label] = work/label, "physical.Entry"
        launch(label, ["--", *args])

    zero = (0,)*8
    normal = (1,4,1,5,22,4,5,122)
    api("texts", "normal", "returned", "127/1/1/4", normal, "args=4", "total=18", "each=10")
    api("texts", "empty", "returned", "0/0/0/0", (1,0,1,0,0,0,0,0))
    api("bare", "empty", "returned", "1", (1,0,1,1,4,1,1,14), "work=14")
    api("duplicates", "empty", "returned", "2", (1,0,1,1,4,1,2,25), "work=25", "objects=1", "sites=2")
    api("constants", "empty", "returned", "41/1/41/3", (1,0,1,2,9,2,4,56))
    api("transport", "transport9", "returned", "9/127/0", (1,9,1,9,39,7,7,216))
    for label, (units, _) in CASES.items():
        case = "raw/"+label
        if label == "wrong-category":
            api(case, "normal", "image Verification Verification", None, zero)
            launch(case, ["--", "a"], failure="Verification")
        else:
            n = len(units)
            api(case, label, "returned", "3", (1,1,1,2,2*n,1,1,5*n+10))
            launch(case)
    for case in ("bare", "constants", "duplicates", "texts"):
        launch(case)
    launch("words", failure="has opaque VM state")
    physical("argv", "texts", "texts/State", [("flags",127),("initialized",1),("seen",1),("argc",4)], ENTRY_ARGS)
    transport = ["repeat","repeat","","","--","--platform","left right","€🚀"]
    physical("os", "transport", "transport/Entry", [("count",8),("flags",63),("aliases",0)], transport)
    launch("os", ["--", *transport[:-1], "different"], failure="ArithmeticException")
    if os.name == "nt":
        # One isolated low surrogate: the OS launcher can preserve it on Windows.
        folder, _ = cases["raw/isolated-both"]
        # This case's middle unit is NUL and therefore cannot be transported physically.
        # Reuse the raw literal generator to isolate a single OS-representable surrogate.
        path = work/"single-unit"
        shutil.copytree(folder, path)
        stage(path, {"sample/Value.class": literal_class([0xdfff], 0x13)})
        cases["single-unit"] = path, "sample.Entry"
        physical("os-unit", "single-unit", "sample/Entry", [("phase",3)], ["\udfff"])
    else:
        launch("os", ["--", "valid", b"\xff"], failure="Encoding { index: 1 }")
    if original:
        api("words", "empty", "MainEvaluation main@68 OpaqueStringObject", "3/0/0", (1,0,1,2,12,2,3,53))
        for cap, index, kind, limit, required in [
                ("args=3","None","Arguments",3,4), ("total=17","Some(3)","ArgumentUnits",17,18),
                ("each=9","Some(3)","UnitsPerArgument",9,10)]:
            api("texts", "normal", f"image Input Limit {{ index: {index}, resource: {kind}, limit: {limit}, required: {required} }}",
                None, zero, cap)
        api("texts", "normal", "image Setup String Objects 1/2", None, (1,1,0,1,4,0,0,5), "objects=1")
        api("bare", "empty", "MainEvaluation main@0 WorkUnits 12/13", "0", (1,0,1,0,0,0,0,12), "work=12")
        api("bare", "empty", "MainEvaluation main@3 WorkUnits 13/14", "0", (1,0,1,1,4,1,1,13), "work=13")
        api("duplicates", "empty", "MainEvaluation main@10 WorkUnits 24/25", "1", (1,0,1,1,4,1,1,21), "work=24")
        api("duplicates", "empty", "MainEvaluation main@10 LiteralSites 1/2", "1", (1,0,1,1,4,1,1,14), "sites=1")
        api("constants", "empty", "NamedInitialization initialization LiteralSites 1/2", "0/0/0/0",
            (1,0,1,1,5,1,1,16), "sites=1")
        api("locks", "two", "MainEvaluation main@40 ActiveMonitors 1/2", "2", (1,2,1,2,8,0,0,10), "monitors=1")
        api("locks", "three", "returned", "4", (1,3,1,3,8,1,0,29), "monitors=1", "depth=2")
        api("locks", "three", "MainEvaluation main@40 Recursion 1/2", "2", (1,3,1,3,8,1,0,29), "depth=1")
    print(f"{'Original' if original else 'Rebuilt'}: {count} public API cases, platform closure {closure}; physical CLI passed", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--platform", type=Path)
    parser.add_argument("--jdk", type=Path, default=os.environ.get("JAVA_HOME"))
    args = parser.parse_args()
    subprocess.run(["cargo","build","--release","--locked","-p","crabspot-launcher","--bin","crabspot",
                    "--example","replay_string_entry"], cwd=ROOT, check=True)
    binaries = (ROOT/os.environ.get("CARGO_TARGET_DIR", "target")/"release").resolve()
    scratch = ROOT/"target/fixture-tool"
    scratch.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=scratch) as temp:
        work = Path(temp)
        platform = args.platform
        if platform is None:
            if not args.jdk:
                parser.error("set JAVA_HOME, pass --jdk, or supply --platform")
            archive, platform = work/"packed", work/"platform"
            sha, count, _ = pack_archive(args.jdk/"jmods/java.base.jmod", archive)
            if extract_classes(archive, platform) != count+1:
                raise ValueError("incorrect extracted platform class count")
            print(f"java.base.jmod {sha}, {count} ordinary classes", flush=True)
        javac = args.jdk/"bin"/("javac.exe" if os.name == "nt" else "javac") if args.jdk else reference_tool("javac")
        run_suite(binaries, platform, applications(work/"original/apps"), work/"original", True)
        run_suite(binaries, platform, later_inputs(work/"rebuilt", javac), work/"rebuilt", False)


if __name__ == "__main__":
    main()
