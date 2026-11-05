"""Original Java 21 bytecode probes for class literals, native selection and allocation."""
from type_state_encodings import ClassEncoding, op, u2

RESULT = "()Ljava/lang/Class;"
LITERALS = (
    ("class-short", "p/Public", 0x12), ("class-wide", "p/Public", 0x13),
    ("private-package", "p/Hidden", 0x12), ("inaccessible-array", "[[Lp/Hidden;", 0x13),
    ("missing", "p/Missing", 0x12), ("missing-array", "[Lp/Missing;", 0x13),
    ("int-array", "[I", 0x12), ("arr-of-I", "[LI;", 0x13),
    ("named-I", "I", 0x12), ("named-V", "V", 0x13),
    ("class-of-class", "java/lang/Class", 0x13), ("wrong-category", "p/Public", 0x14),
    ("closure-only", "java/lang/reflect/GenericDeclaration", 0x12),
)
CALLS = ("object", "inherited", "interface-object", "array-symbol", "null", "static-mismatch",
         "overload", "native-overload", "unfinished", "super-getclass", "cloneable-array",
         "serializable-array", "nonimplementor", "covariant-array", "visible-array", "through-object")
ALLOCATIONS = ("abandon", "unfinished-lock", "constructor", "array", "real-literal")


def literal_class(name, opcode):
    caller = ClassEncoding("q/Caller")
    if opcode != 0x12:
        for i in range(260):
            caller.utf(f"unused{i}")
    index = caller.cls(name)
    if (index > 255) != (opcode != 0x12):
        raise ValueError(f"unexpected constant pool index {index} for opcode {opcode:#x}")
    instruction = bytes([opcode, index]) if opcode == 0x12 else op(opcode, index)
    return caller.finish([], [(9, "run", RESULT, instruction + b"\xb0", 2, 0)])


def invocation_classes(case):
    caller = ClassEncoding("q/Caller")
    thing = ClassEncoding("p/Thing")
    overload = "(I)Ljava/lang/Class;"
    methods = [(1, "<init>", "()V", b"\x2a" + thing.invoke_init("java/lang/Object") + b"\xb1", 1, 1)]
    if case == "native-overload":
        methods.append((0x101, "getClass", overload))
    else:
        body = op(0x13, thing.cls("java/lang/Object")) + b"\xb0"
        methods.append((1, "getClass", overload, body, 1, 2))
    super_call = op(0xb7, thing.ref(10, "java/lang/Object", "getClass", RESULT))
    methods.append((1, "superType", RESULT, b"\x2a" + super_call + b"\xb0", 1, 1))
    receiver = caller.allocation("p/Thing")
    caller_methods = []
    if case == "interface-object":
        receiver = caller.allocation("p/Impl")
        code = receiver + op(0xb9, caller.ref(11, "p/Face", "getClass", RESULT)) + b"\x01\x00"
    elif case == "array-symbol":
        code = bytes.fromhex("04 bc 0a") + op(0xb6, caller.ref(10, "[I", "getClass", RESULT))
    elif case == "covariant-array":
        receiver = b"\x04" + op(0xbd, caller.cls("p/Thing"))
        code = receiver + op(0xb6, caller.ref(10, "[Ljava/lang/Object;", "getClass", RESULT))
    elif case in ("cloneable-array", "serializable-array", "nonimplementor"):
        owner = "java/lang/Cloneable" if case == "cloneable-array" else "java/io/Serializable"
        receiver = caller.allocation("java/lang/Object") if case == "nonimplementor" else bytes.fromhex("04 bc 0a")
        code = receiver + op(0xb9, caller.ref(11, owner, "getClass", RESULT)) + b"\x01\x00"
    elif case in ("visible-array", "through-object"):
        receiver = bytes.fromhex("04 bc 0a")
        interface_call = op(0xb9, caller.ref(11, "p/Face", "getClass", RESULT)) + b"\x01\x00"
        if case == "visible-array":
            code = receiver + interface_call
        else:
            descriptor = "(Ljava/lang/Object;)Ljava/lang/Class;"
            caller_methods.append((9, "helper", descriptor, b"\x2a" + interface_call + b"\xb0", 1, 1))
            code = receiver + op(0xb8, caller.ref(10, caller.name, "helper", descriptor))
    elif case == "static-mismatch":
        code = op(0xb8, caller.ref(10, "java/lang/Object", "getClass", RESULT))
    elif case in ("overload", "native-overload"):
        code = receiver + b"\x03" + op(0xb6, caller.ref(10, "p/Thing", "getClass", overload))
    elif case == "super-getclass":
        code = receiver + op(0xb6, caller.ref(10, "p/Thing", "superType", RESULT))
    else:
        if case == "null":
            receiver = b"\x01"
        elif case == "unfinished":
            receiver = op(0xbb, caller.cls("p/Thing"))
        owner = "p/Thing" if case == "inherited" else "java/lang/Object"
        code = receiver + op(0xb6, caller.ref(10, owner, "getClass", RESULT))
    caller_methods.append((9, "run", RESULT, code + b"\xb0", 3, 0))
    return {"q/Caller.class": caller.finish([], caller_methods),
            "p/Thing.class": thing.finish([], methods)}


def allocation_class(case):
    caller = ClassEncoding("q/NewClass")
    klass = caller.cls("java/lang/Class")
    store_phase = op(0xb3, caller.ref(9, caller.name, "phase", "I"))
    create = op(0xbb, klass)
    probes = {
        "abandon": create + b"\x57",
        "unfinished-lock": create + b"\x59\xc2\xc3",
        "constructor": create + b"\x59" + caller.invoke_init("java/lang/Class") + b"\x57",
        "array": b"\x04" + op(0xbd, klass) + b"\x57",
        "real-literal": op(0x13, klass) + b"\x57",
    }
    code = b"\x10\x01" + store_phase + probes[case] + b"\x10\x02" + store_phase + b"\x04\xac"
    return caller.finish([(9, "phase", "I")], [(9, "run", "()I", code, 3, 0)])


def encodings():
    files = {}
    for case, name, opcode in LITERALS:
        files[f"literal/{case}/q/Caller.class"] = literal_class(name, opcode)
    for name, flags in (("p/Public", 0x21), ("p/Hidden", 0x20), ("I", 0x21), ("V", 0x21)):
        files[f"literal-support/{name}.class"] = ClassEncoding(name).finish([], [], flags=flags)
    for case in CALLS:
        for path, data in invocation_classes(case).items():
            files[f"call/{case}/{path}"] = data
    for case in ALLOCATIONS:
        files[f"new/{case}/q/NewClass.class"] = allocation_class(case)
    for name in ("Original", "Fresh", "Peer"):
        caller = ClassEncoding("late/" + name)
        read = op(0x13, caller.cls("late/Peer")) + b"\xb0"
        files[f"sticky/late/{name}.class"] = caller.finish([], [(9, "run", RESULT, read, 1, 0)])
    return files


def application_driver(expected_class):
    """A real main writes phase 1 before the original probe and 2 on the expected result."""
    driver = ClassEncoding("probe/Entry")
    phase = op(0xb3, driver.ref(9, driver.name, "phase", "I"))
    prefix = b"\x04" + phase
    if expected_class is None:
        # q.NewClass.run returns 1 only after reaching its own second phase.
        invoke = op(0xb8, driver.ref(10, "q/NewClass", "run", "()I"))
        compare = b"\x04\xa0"
    else:
        invoke = op(0xb8, driver.ref(10, "q/Caller", "run", RESULT))
        compare = op(0x13, driver.cls(expected_class)) + b"\xa6"
    success = b"\x05" + phase
    # if_icmpne / if_acmpne skips the successful write to the common return.
    code = prefix + invoke + compare + u2(3 + len(success)) + success + b"\xb1"
    frame = u2(1) + bytes([len(code) - 1])
    main = (9, "main", "([Ljava/lang/String;)V", code, 2, 1, (), frame)
    return driver.finish([(9, "phase", "I")], [main])


def driver_encodings():
    # For an expected abrupt outcome, a reachable fallback literal would disguise a bug.
    # Use Object for the comparison and require the exact abrupt outcome in the observer.
    failures = {"private-package", "inaccessible-array", "missing", "missing-array", "wrong-category"}
    expected_calls = {"object": "p/Thing", "inherited": "p/Thing", "interface-object": "p/Impl",
                      "array-symbol": "[I", "super-getclass": "p/Thing", "cloneable-array": "[I",
                      "serializable-array": "[I", "covariant-array": "[Lp/Thing;"}
    targets = {"literal/"+case: "java/lang/Object" if case in failures else name for case, name, _ in LITERALS}
    targets.update({"call/"+case: expected_calls.get(case, "java/lang/Object") for case in CALLS})
    targets.update({"new/"+case: None for case in ALLOCATIONS})
    return {key+"/probe/Entry.class": application_driver(target) for key, target in targets.items()}
