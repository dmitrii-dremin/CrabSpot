"""Check actual Java String bodies and nest access through the CrabSpot directory entry points."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

from application_entry_reference import inventory
from string_activation_reference import DIRECTORY as ACTIVATION, prepare as prepare_activation
from string_body_nests import CASES, make
from string_body_reference import DIRECTORY, ROOT, capture_inputs, compile_captured
from string_constant_reference import DIRECTORY as STRINGS
from loading_constraint_reference import version
from type_state_encodings import ClassEncoding, op
from verify_application_entry import extract_classes
from verify_java_base import pack_archive
from verify_string_entry import physical_entry, process


def observer(binary, platform, folder, main, arguments, *limits):
    rows = process([binary, platform, folder, main, arguments, *limits]).splitlines()
    if len(rows) not in (2, 3):
        raise ValueError(f"{main} malformed observer output {rows}")
    labels = ("discovery", "work", "backings", "bytes", "proved", "witnesses", "admitted", "argv", "args", "evaluations")
    parts = [item.partition("=") for item in rows[-1].split()]
    if tuple(name for name, _, _ in parts) != labels or any(not n.isascii() or not n.isdecimal() for _, _, n in parts):
        raise ValueError(f"{main} malformed counters {rows[-1]}")
    values = {name: int(n) for name, _, n in parts}
    fields = None
    if len(rows) == 3:
        if not rows[1].startswith("fields="):
            raise ValueError(f"{main} missing fields: {rows}")
        fields = rows[1].removeprefix("fields=")
        if not values["proved"] or not values["proved"] == values["witnesses"] == values["admitted"]:
            raise ValueError(f"{main} inconsistent proof authority {values}")
    return rows[0], fields, values


def check(row, outcome, fields, backing=None, **counters):
    status, retained, usage = row
    if not re.fullmatch(outcome, status) or fields != retained:
        raise ValueError(f"expected {outcome!r} fields {fields!r}, observed {row}")
    if backing is not None:
        counters.update(zip(("work", "backings", "bytes"), backing, strict=True))
    for key, expected in counters.items():
        if usage[key] != expected:
            raise ValueError(f"expected {key}={expected}, observed {row}")
    return usage


def stage_groups(work, javac):
    base = work/"captured"
    inputs = capture_inputs(DIRECTORY, base)
    manifest = json.loads((DIRECTORY/"manifest.json").read_bytes())
    if inputs["classes"] != manifest["original"] or inputs["encoded"] != manifest["encoded"]:
        raise ValueError("changed archived Java-body application")
    sources = {k+"/"+n: sha for k in ("src", "controls") for n, sha in inputs[k].items()}
    if sources != manifest["sources"]:
        raise ValueError("changed Java-body source inventory")
    compile_captured(javac, base, inputs, work/"compiled", work/"controls")
    pinned = version(javac) == manifest["compiler"]
    for key, folder in (("compiled", work/"compiled"), ("controls", work/"controls")):
        actual = inventory(folder)
        if actual.keys() != manifest[key].keys() or pinned and actual != manifest[key]:
            raise ValueError(f"unexpected compiler output for {key}")
    for case in CASES:
        make(work, *case)
    if inventory(work/"nest-encoded") != inputs["encoded"]:
        raise ValueError("re-encoded nest inputs differ")
    old, new, control, activation, compiled = prepare_activation(work/"activation", javac)
    expected = json.loads((ACTIVATION/"manifest.json").read_bytes())
    if activation["classes"] != expected["original"] or activation["src"] | activation["controls"] != expected["sources"]:
        raise ValueError("changed activation fixture inputs")
    if pinned and (compiled != expected["compiled"] or inventory(control) != expected["controls"]):
        raise ValueError("unexpected pinned activation compiler output")
    for kind, apps, nests, budget in (("original", base/"classes", base/"encoded", old),
                                      ("rebuilt", work/"compiled", work/"nest-encoded", new)):
        group = work/kind
        for package in ("compact", "peer"):
            shutil.copytree(apps/package, group/package/package)
        yield group, group/"compact", group/"peer", nests, budget


def product_suite(bin_dir, platform, javac, work, openj9=None):
    ext = ".exe" if os.name == "nt" else ""
    binary = bin_dir/"examples"/("replay_string_body_entry"+ext)
    cli = bin_dir/("crabspot"+ext)
    actual = hashlib.sha256((platform/"java/lang/String.class").read_bytes()).hexdigest()
    print(f"Supplied String.class {actual}", flush=True)
    field_names = ("flags","emptyLength","latinLength","mixedLength","isolatedLength","pairLength","argumentLength")
    for group, compact, peer, nests, budget in stage_groups(work, javac):
        def replay(directory, main, args, *limits):
            return observer(binary, platform, directory, main, args, *limits)

        def physical(label, directory, main, fields, arguments):
            path = group/label
            physical_entry(path, directory, main, main.replace(".", "/"), fields)
            command = [cli, "run", "--platform", platform, "--classes", path, "physical.Entry", "--", *arguments]
            if process(command):
                raise ValueError(f"unexpected stdout from {label}")

        # For each cold read: one unit plus the UTF-16 width scan n and bytes B.
        # Each length then reads coder and each isEmpty reads value. The final alias reads both again.
        for args, values, costs, argv in [
                ("none", [63,0,3,4,1,2,0], (44,5,17), []),
                ("two", [127,0,3,4,1,2,3], (58,7,23), ["","A€B"])]:
            row = replay(compact, "compact.Entry", args, "strict")
            usage = check(row, "returned", "/".join(map(str,values)), costs, argv=1, args=len(argv), evaluations=1)
            if not usage["discovery"]:
                raise ValueError("compatible image consumed no discovery work")
            physical("length-"+args, compact, "compact.Entry", list(zip(field_names, values, strict=True)), argv)
        for args, flag, costs, argv in [("none",3,(5,2,2),[]), ("empty",7,(6,3,2),[""])]:
            check(replay(compact, "compact.EmptyOnly", args), "returned", str(flag), costs, args=len(argv))
            physical("empty-"+args, compact, "compact.EmptyOnly", [("flags",flag)], argv)
        if process([cli,"run","--platform",platform,"--classes",group/"empty-empty","physical.Entry","--","changed"],
                   1, "ArithmeticException"):
            raise ValueError("unexpected stdout for the wrong physical argument")
        for args, fields, costs in [("empty","4/0/1/1",(3,1,0)), ("latin","4/3/0/1",(9,1,3)),
                                    ("wide","4/2/0/1",(9,1,4))]:
            check(replay(budget, "budget.Entry", args), "returned", fields, costs)
        physical("activate", budget, "budget.Entry", [("phase",4),("units",0),("empty",1),("stable",1)], [""])
        peer_usage = check(replay(peer, "peer.Host", "none"), "returned", "7", (0,0,0))
        if not peer_usage["discovery"]:
            raise ValueError("compatible image consumed no discovery work before dependency closure")

        for label, *_ in CASES:
            row = replay(nests/label, "peer.Host", "none")
            if label in {"absent-host","unlisted","extra-missing"}:
                check(row, "image Dependencies MissingApplicationDependency", None, (0,0,0),
                      discovery=peer_usage["discovery"], argv=0, args=0, evaluations=0,
                      proved=0, witnesses=0, admitted=0)
            elif label == "genuine-names":
                check(row, "returned", "7", (0,0,0))
            else:
                check(row, r"MainEvaluation main@4( via main@4)? IllegalAccessError", "1", (0,0,0))
        print(f"{group.name}: genuine bodies, arguments and all nine nest cases passed", flush=True)
        if group.name == "original":
            boundaries(replay, compact, budget)
            words = group/"words"
            shutil.copytree(STRINGS/"classes/words", words/"words")
            check(replay(words, "words.Entry", "none"),
                  "MainEvaluation main@78 via main@78 OpaqueStringObject", "3/0/0", (23,1,14))
            if openj9:
                expected_j9 = "25bc3e38a28f35a537715c22ea57abe9c78db5c630535cc603345b92f39278ce"
                if hashlib.sha256((openj9/"java/lang/String.class").read_bytes()).hexdigest() != expected_j9:
                    raise ValueError("unexpected OpenJ9 String")
                legacy = observer(binary, openj9, compact, "compact.EmptyOnly", "none")
                u = check(legacy, r"MainEvaluation main@2( via main@2)? OpaqueStringObject", "0", (0,0,0))
                strict = observer(binary, openj9, compact, "compact.EmptyOnly", "none", "strict")
                check(strict, "image Verification UnsupportedShape", None, (0,0,0), discovery=u["discovery"], argv=0)
                check(observer(binary, openj9, words, "words.Entry", "none"),
                      "MainEvaluation main@68 via main@68 OpaqueStringObject", "3/0/0", (0,0,0))
            altered_platform(binary, platform, compact, group/"altered-platform")
            private = ClassEncoding("peer/Host")
            constant = private.cls("java/io/ObjectStreamField")
            method = (9,"main","([Ljava/lang/String;)V",op(0x13, constant)+bytes.fromhex("57 b1"),1,1)
            path = group/"private-import/peer/Host.class"
            path.parent.mkdir(parents=True)
            path.write_bytes(private.finish([(9,"phase","I")], [method]))
            check(replay(group/"private-import", "peer.Host","none"),
                  "image Dependencies UnsupportedPlatformReference", None, (0,0,0),
                  discovery=peer_usage["discovery"], argv=0, args=0, evaluations=0,
                  proved=0, witnesses=0, admitted=0)


def boundaries(replay, compact, budget):
    for fuel, site, fields, costs in [
        (7, "isEmpty@1 via main@7", "1/0/0/0", (0,0,0)),
        (8, "isEmpty@4 via main@7", "1/0/0/0", (1,1,0)),
        (24, "length@4 via main@24", "2/0/1/0", (2,1,0)),
        (33, "<clinit>@11 via coder@0", "2/0/1/0", (2,1,0)),
        (63, "main@63", "4/0/1/1", (3,1,0)),
    ]:
        check(replay(budget, "budget.Entry", "empty", f"fuel={fuel}"),
              f"MainEvaluation {site} Fuel {fuel}", fields, costs)
    # Two arrays allow argv and the empty String's backing. isEmpty completes;
    # length reaches coder and the real initializer attempts a third array.
    check(replay(budget, "budget.Entry", "empty", "arrays=2"),
          "MainEvaluation <clinit>@5 via coder@0 Arrays 2/3", "2/0/1/0", (2,1,0))
    check(replay(budget, "budget.Entry", "empty", "arrays=1"),
          r"MainEvaluation isEmpty@1( via \S+@\d+)? Arrays 1/2", "1/0/0/0", (1,0,0))
    # The deepest call prepares Object.<init> from the comparator constructor.
    # Its single return byte brings the total from 131 to 132 prepared bytes.
    for limit, error in (("frames=5", "Frames 5/6"), ("code=131", "CodeBytes 131/132")):
        check(replay(budget, "budget.Entry", "empty", limit),
              f"MainEvaluation <init>@entry via <init>@1 {error}", "2/0/1/0", (2,1,0))
    check(replay(budget, "budget.Entry", "empty", "frames=6", "code=132", "arrays=3", "work=3", "fuel=64"),
          "returned", "4/0/1/1", (3,1,0))
    check(replay(budget, "budget.Entry", "empty", "work=2"),
          "MainEvaluation coder@7 via length@6 BackingWork 2/3", "2/0/1/0", (2,1,0))
    for args, limit, needed, accepted in [("empty",0,1,0),("latin",3,4,1),("wide",6,7,3)]:
        check(replay(budget, "budget.Entry", args, "work="+str(limit)),
              rf"MainEvaluation isEmpty@1( via \S+@\d+)? BackingWork {limit}/{needed}",
              "1/0/0/0", (accepted,0,0))
    exact = check(replay(compact, "compact.EmptyOnly", "none", "work=5"), "returned", "3", (5,2,2))
    discovery = exact["discovery"]
    check(replay(compact, "compact.EmptyOnly", "none", f"discovery={discovery}"), "returned", "3", (5,2,2))
    row = replay(compact, "compact.EmptyOnly", "none", f"discovery={discovery-1}")
    u = check(row, rf"image Verification DiscoveryWork {discovery-1}/{discovery}", None, (0,0,0), argv=0)
    if u["discovery"] >= discovery:
        raise ValueError("failed discovery consumed an unpaid charge")
    check(replay(compact, "compact.EmptyOnly", "none", "discovery=0"),
          "image Verification DiscoveryWork 0/1", None, (0,0,0), discovery=0, argv=0)


def replace_instruction(blob, recipe, position, opcode):
    # Match the complete Code byte span, its declared length and the empty handler table.
    # Only CP slot numbers vary. An ambiguous method body is unsuitable for this control.
    matches = list(re.finditer(recipe, blob, re.DOTALL))
    if len(matches) != 1:
        raise ValueError("expected one original Code recipe for the platform mutation")
    if not 0 <= position < len(matches[0].group("code")) or not 0 <= opcode <= 255:
        raise ValueError("instruction replacement outside the chosen Code or byte range")
    offset = matches[0].start("code") + position
    return blob[:offset] + bytes([opcode]) + blob[offset+1:]


def altered_platform(binary, platform, compact, work):
    shutil.copytree(platform, work)
    path = work/"java/lang/String.class"
    original = path.read_bytes()
    # Keep verification valid but change the actual arithmetic in length from ishr to iadd.
    length = rb"\x00\x00\x00\x0b(?P<code>\x2a\xb4..\xbe\x2a\xb6..\x7a\xac)\x00\x00"
    path.write_bytes(replace_instruction(original, length, 9, 0x60))
    for mode in ([], ["strict"]):
        check(observer(binary, work, compact, "compact.EmptyOnly", "none", *mode),
              r"image Verification InvalidBinding .+", None, (0,0,0), argv=0, evaluations=0)
    # A different initializer is an unsupported recipe. Auto can use the original kernel.
    init = rb"\x00\x00\x00\x16(?P<code>\x04\xb3..\x03\xbd..\xb3..\xbb..\x59\xb7..\xb3..\xb1)\x00\x00"
    path.write_bytes(replace_instruction(original, init, 0, 0x03))
    row = observer(binary, work, compact, "compact.EmptyOnly", "none")
    usage = check(row, "MainEvaluation main@2 via main@2 OpaqueStringObject", "0", (0,0,0), evaluations=1)
    check(observer(binary, work, compact, "compact.EmptyOnly", "none", "strict"),
          "image Verification UnsupportedShape", None, (0,0,0), argv=0, discovery=usage["discovery"])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--jdk", type=Path, default=os.environ.get("JAVA_HOME"))
    parser.add_argument("--platform", type=Path)
    parser.add_argument("--openj9-platform", type=Path)
    args = parser.parse_args()
    if not args.jdk:
        parser.error("set JAVA_HOME or provide --jdk for Java 21 fixture regeneration")
    subprocess.run(["cargo","build","--locked","--release","-p","crabspot-launcher","--bin","crabspot",
                    "--example","replay_string_body_entry"], cwd=ROOT, check=True)
    binaries = (ROOT/os.environ.get("CARGO_TARGET_DIR","target")/"release").resolve()
    scratch = ROOT/"target/fixture-tool"
    scratch.mkdir(exist_ok=True, parents=True)
    with tempfile.TemporaryDirectory(dir=scratch) as temp:
        work = Path(temp)
        platform = args.platform
        if not platform:
            archive, platform = work/"platform.bin", work/"platform"
            sha, count, _ = pack_archive(args.jdk/"jmods/java.base.jmod", archive)
            if extract_classes(archive, platform) != count+1:
                raise ValueError("platform extraction inventory changed")
            print(f"java.base.jmod {sha}, {count} ordinary classes", flush=True)
        javac = args.jdk/"bin"/("javac.exe" if os.name == "nt" else "javac")
        product_suite(binaries, platform, javac, work/"cases", args.openj9_platform)


if __name__ == "__main__":
    main()
