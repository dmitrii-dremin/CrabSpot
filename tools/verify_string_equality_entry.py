"""Replay String equality, active Class initialization and bootstrap assertions on CrabSpot."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

from application_entry_reference import inventory
from loading_constraint_reference import version
from string_body_reference import FLAGS, ROOT, capture_inputs, compile_captured, run
from string_equality_reference import prepare as prepare_equality
from verify_application_entry import extract_classes
from verify_class_mirror_entry import check_api, platform_cases
from verify_java_base import pack_archive
from verify_string_body_entry import replace_instruction
from verify_string_entry import physical_entry, process

FIXTURES = ROOT/"tests/fixtures"
COUNTERS = ("proved", "witnesses", "admitted", "argv", "args", "evaluations", "backings",
            "bytes", "work", "discovery", "compact", "registered", "queries")
ASCII = ["A~a", "A~a", "A~b", "Wxyz", "Wxyz", "WxyQ", "same", "same", "Wxy"]


def observe(binary, platform, classes, entry, shape, options, status, fields, **expected):
    rows = process([binary, platform, classes, entry, shape, *options]).splitlines()
    body = [status] if fields is None else [status, "fields="+str(fields)]
    if rows[:-1] != body:
        raise ValueError(f"{entry} {shape} {options}: expected {body}, got {rows}")
    pairs = [token.partition("=") for token in rows[-1].split()]
    if tuple(k for k, _, _ in pairs) != COUNTERS or any(not v.isascii() or not v.isdecimal() for _,_,v in pairs):
        raise ValueError(f"invalid counters: {rows[-1]}")
    usage = {k:int(v) for k,_,v in pairs}
    if fields is not None:
        if not usage["proved"] or usage["proved"] != usage["witnesses"] or usage["proved"] != usage["admitted"]:
            raise ValueError(f"inconsistent executable class proofs: {usage}")
        expected.update(argv=1, evaluations=1)
    if any(usage[key] != value for key,value in expected.items()):
        raise ValueError(f"{entry} {options}: expected {expected}, actual {usage}")
    return usage


def captures(work, javac):
    directory = FIXTURES/"string-equality"
    manifest = json.loads((directory/"manifest.json").read_bytes())
    inputs, original, rebuilt, controls = prepare_equality(directory, work/"equality", javac)
    pinned = version(javac) == manifest["compiler"]
    sources = {g+"/"+key:v for g in ("src","controls") for key,v in inputs[g].items()}
    if inputs["classes"] != manifest["original"] or sources != manifest["sources"]:
        raise ValueError("changed equality application")
    if pinned and (inventory(rebuilt) != manifest["compiled"] or inventory(controls) != manifest["controls"]):
        raise ValueError("pinned equality classes changed")
    directory = FIXTURES/"string-equality-execution"
    recorded = json.loads((directory/"manifest.json").read_bytes())
    capture = work/"cuts"
    extra = capture_inputs(directory, capture)
    if extra["encoded"] or any(extra[group] != recorded[group] for group in ("src", "controls", "classes")):
        raise ValueError("changed equality execution fixture inventory")
    if recorded["flags"] != FLAGS or any(recorded[k] != manifest[k] for k in ("compiler","primary","secondary")):
        raise ValueError("unexpected fixture compiler settings")
    apps, observers = work/"cuts-rebuilt", work/"cut-controls"
    compile_captured(javac, capture, extra, apps, observers)
    for folder, key in ((apps,"classes"),(observers,"compiled_controls")):
        actual = inventory(folder)
        if actual.keys() != recorded[key].keys() or pinned and actual != recorded[key]:
            raise ValueError(f"unexpected compiled execution {key}")
    return [(work/"original",original,capture/"classes"), (work/"rebuilt",rebuilt,apps)], observers, controls


def reference_queries(vm, name, apps, observers, pinned):
    recorded = json.loads((FIXTURES/"string-equality-execution/observations.json").read_bytes())[name]
    expected = {"alias":"2/1", "equal":"2/1", "first":"2/0", "last":"2/0", "cross-coder":"2/0"}
    for target in range(3):
        expected[f"query/{target}"] = "2/0/0"
        expected[f"query/{target}/-esa"] = "2/"+str(int(target == 0 or target == 2 and name == "openj9"))+"/0"
    if expected != recorded:
        raise ValueError(f"unexpected {name} archived activation observations")
    for label, result in expected.items():
        if label == "query/2/-esa" and not pinned:
            continue  # Implementations can differ on assertion queries for arrays.
        parts = label.split("/")
        args = parts if parts[0] != "query" else parts[:2]
        options = ["-Xshareclasses:none"] if name == "openj9" else []
        actual = run(vm, [observers,apps], "ObserveEquality", args, options+parts[2:])
        if actual != result:
            raise ValueError(f"{name} {label}: expected {result}, observed {actual}")


def altered_images(binary, platform, text, work):
    # Only the selected class is changed. All other bootstrap bytes come from the supplied image.
    image = work/"altered-platform"
    shutil.copytree(platform, image)
    alias = rb"\x00\x00\x00\x38(?P<code>\x2a\x2b\xa6\x00\x05\x04\xac\x2b\xc1.{47})\x00\x00"
    step = rb"(?P<code>\x84\x02\x01\xa7\xff\xec\x04\xac\x03\xac)\x00\x00"
    monitor = rb"(?P<code>\xb4..\x59\x4d\xc2\x2b\xb4..\xc6\x00\x0e)"
    # The last catch-all target changes from 44 to 45, even though bootstrap queries skip this branch.
    handlers = rb"\x00\x03\x00\x15\x00\x26\x00\x2c\x00\x00\x00\x27\x00\x29\x00\x2c\x00\x00(?P<code>\x00\x2c\x00\x2f\x00\x2c\x00\x00)"
    changes = [
        ("String",alias,5,0x03,None),
        ("StringLatin1",step,2,0x02,"StringLatin1 method differs"),
        ("Class",monitor,5,0xc3,"Class method recipe differs"),
        ("Class",handlers,5,45,"Class method recipe differs"),
    ]
    for name,pattern,position,replacement,claimed_error in changes:
        path = image/"java/lang"/(name+".class")
        original = path.read_bytes()
        path.write_bytes(replace_instruction(original,pattern,position,replacement))
        if claimed_error:
            for options in ([], ["strict"]):
                observe(binary,image,text,"app.TextEquality","text",options,
                        "image Verification InvalidBinding "+claimed_error,None,
                        argv=0,evaluations=0,registered=0,queries=0)
        else:
            observe(binary,image,text,"app.TextEquality","text",[],
                    "MainEvaluation equal@10 via equal@10 OpaqueStringObject",[0,1],registered=0,queries=0)
            observe(binary,image,text,"app.TextEquality","text",["strict"],
                    "image Verification UnsupportedShape",None,argv=0,evaluations=0)
            folder, main = platform_cases(work)["platform/active-forname"]
            mirror = binary.with_name("replay_class_mirror_entry"+binary.suffix)
            check_api([mirror,image,folder,main],
                      "MainEvaluation initializing UnboundNative registerNatives@entry via <clinit>@0 fields=1")
        path.write_bytes(original)


def suite(binary, cli, platform, group, inputs, apps, secondary):
    def check(folder, main, shape, options, status, fields, costs=(0,0,0), registered=0, queries=0, **extra):
        return observe(binary, platform, folder, main, shape, options, status, fields,
                       work=costs[0], backings=costs[1], bytes=costs[2], registered=registered, queries=queries, **extra)
    for name in ("TextEquality","BootstrapStatus","Policy"):
        folder = group/name/"app"
        folder.mkdir(parents=True)
        shutil.copyfile(inputs/"app"/(name+".class"), folder/(name+".class"))
    text, bootstrap = group/"TextEquality", group/"BootstrapStatus"
    for switches, bits in (([],0),(["enabled"],3)):
        u = check(text, "app.TextEquality", "text", ["strict", *switches], "returned", [511,10],
                  (77,9,31), 1, 1, args=9)
        if not u["discovery"] or not u["compact"]:
            raise ValueError("matched image without paid discovery")
        check(bootstrap, "app.BootstrapStatus", "none", switches, "returned", [bits,2], registered=1,queries=2)
    for shape, same, costs, native in (("alias",1,(0,0,0),0),("equal",1,(516,2,256),1),
                                       ("first",0,(516,2,256),1),("last",0,(516,2,256),1),
                                       ("cross-coder",0,(7,2,3),0)):
        check(apps,"probe.Compare",shape,[],"returned",[2,same],costs,native,native)
    changed = ASCII.copy()
    changed[1] = changed[2]
    for flags, good, bad in ((511, ASCII, changed),(507, changed, ASCII)):
        path = group/str(flags)
        physical_entry(path, text, "app.TextEquality", "app/TextEquality", [("flags",flags),("calls",10)])
        cmd = [cli,"run","--platform",platform,"--classes",path,"physical.Entry","--"]
        if process([*cmd,*good]) or process([*cmd,*bad],1,"ArithmeticException"):
            raise ValueError("unexpected stdout from physical equality check")
    print(f"{group.name}: actual equality and assertion bodies, nine real Java arguments and physical CLI passed", flush=True)
    if group.name != "original":
        return
    check(group/"Policy","app.Policy","none",[],"image Dependencies UnsupportedPlatformReference cp=26",None,
          argv=0, evaluations=0)
    for shape, kind in (("one","class"),("two","array")):
        check(apps,"query.Query",shape,[],"MainEvaluation desiredAssertionStatus@1 via main@31 UnsupportedAssertionTarget "+kind,
              [1,0,0])
    for cap, pc in ((1,4),(2,11)):
        check(bootstrap,"app.BootstrapStatus","none",[f"arrays={cap}"],
              f"MainEvaluation <clinit>@{pc} via desiredAssertionStatus@10 Arrays {cap}/{cap+1}",[0,1], registered=1)
    for cap, outcome in (("handlers=2","desiredAssertionStatus@entry via main@6 Handlers 2/3"),
                         ("code=94","desiredAssertionStatus@entry via main@6 Code 94/95"),
                         ("code=112","<clinit>@entry via desiredAssertionStatus@10 Code 112/113"),
                         ("frames=2","<clinit>@entry via desiredAssertionStatus@10 Frames 2/3")):
        check(bootstrap,"app.BootstrapStatus","none",[cap],"MainEvaluation "+outcome,[0,1])
    check(bootstrap,"app.BootstrapStatus","none",["handlers=3","arrays=3","frames=3","code=113"],
          "returned",[0,2], registered=1, queries=2)
    check(apps,"probe.Compare","first",["fuel=250"],"returned",[2,0],(516,2,256),1,1)
    for shape in ("equal","last"):
        check(apps,"probe.Compare",shape,["fuel=250"],"MainEvaluation equals@29 via equals@44 Fuel 250",
              [1,0],(516,2,256),1,1)
    limit = u["discovery"]
    check(text,"app.TextEquality","text",[f"discovery={limit}"],"returned",[511,10],(77,9,31),1,1)
    check(text,"app.TextEquality","text",[f"discovery={limit-1}"],
          f"image Verification DiscoveryWork {limit-1}/{limit}",None,argv=0,evaluations=0)
    altered_images(binary,platform,text,group)
    if secondary:
        for options,status,fields in ((["strict"],"image Verification UnsupportedShape",None),
                                      ([],"MainEvaluation equal@10 via equal@10 OpaqueStringObject",[0,1])):
            observe(binary,secondary,text,"app.TextEquality","text",options,status,fields,
                    work=0,backings=0,bytes=0,registered=0,queries=0,discovery=0)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--jdk", type=Path, default=os.environ.get("JAVA_HOME"))
    parser.add_argument("--platform", type=Path)
    parser.add_argument("--openj9-platform", type=Path)
    parser.add_argument("--compare-openj9", type=Path)
    args = parser.parse_args()
    if not args.jdk:
        parser.error("provide a Java 21 JDK with JAVA_HOME or --jdk")
    suffix = ".exe" if os.name == "nt" else ""
    java, javac = (args.jdk/"bin"/(name+suffix) for name in ("java", "javac"))
    parent = json.loads((FIXTURES/"string-equality/manifest.json").read_bytes())
    primary = version(java)
    if "Server VM" not in primary or "OpenJ9" in primary:
        parser.error("the primary reference must be Java 21 HotSpot")
    if args.compare_openj9 and version(args.compare_openj9) != parent["secondary"]:
        raise ValueError("unexpected OpenJ9 runtime")
    subprocess.run(["cargo","build","--release","--locked","-p","crabspot-launcher","--bin","crabspot",
                    "--example","replay_string_equality_entry","--example","replay_class_mirror_entry"], cwd=ROOT, check=True)
    binaries = ROOT/os.environ.get("CARGO_TARGET_DIR","target")/"release"
    scratch = ROOT/"target/fixture-tool"
    scratch.mkdir(exist_ok=True, parents=True)
    with tempfile.TemporaryDirectory(dir=scratch) as tmp:
        work = Path(tmp)
        platform = args.platform
        if not platform:
            archive, platform = work/"packed", work/"platform"
            _, count, _ = pack_archive(args.jdk/"jmods/java.base.jmod", archive)
            if extract_classes(archive,platform) != count+1:
                raise ValueError("incomplete platform extraction")
        def identities(directory):
            return [name+"="+hashlib.sha256((directory/"java/lang"/(name+".class")).read_bytes()).hexdigest()
                    for name in ("String","StringLatin1","Class","ClassLoader")]
        print("Supplied java.base " + " ".join(identities(platform)), flush=True)
        if args.openj9_platform and identities(args.openj9_platform) != parent["platform_classes"]["openj9"]:
            raise ValueError("OpenJ9 classfiles differ from the documented negative control")
        groups, observers, identity_controls = captures(work, javac)
        primary_pinned = (primary == parent["primary"] and
                          run(java,[identity_controls],"PlatformIdentity",[],[]).splitlines() == parent["platform_classes"]["hotspot"])
        secondary_pinned = (args.compare_openj9 is not None and
                            run(args.compare_openj9,[identity_controls],"PlatformIdentity",[],["-Xshareclasses:none"]).splitlines()
                            == parent["platform_classes"]["openj9"])
        for group, inputs, apps in groups:
            reference_queries(java,"hotspot",apps,observers,primary_pinned)
            if args.compare_openj9:
                reference_queries(args.compare_openj9,"openj9",apps,observers,secondary_pinned)
            suite(binaries/"examples"/("replay_string_equality_entry"+suffix), binaries/("crabspot"+suffix),
                  platform,group,inputs,apps,args.openj9_platform)


if __name__ == "__main__":
    main()
