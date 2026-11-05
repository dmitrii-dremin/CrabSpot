"""Original Java 21 member references and deliberately mismatched invocation opcodes."""

from type_state_encodings import ClassEncoding, op


def field_method_order():
    descriptor = "Lvalue/K;"
    signature = "(Lvalue/K;)V"
    base = ClassEncoding("owner/Base")
    methods = [
        (9, "publicCall", signature, b"\xb1", 0, 1),
        (10, "privateCall", signature, b"\xb1", 0, 1),
        (1, "instanceCall", signature, b"\xb1", 0, 2),
    ]
    fields = [(flags, name, descriptor) for flags, name in [
        (9, "sf"), (1, "instanceField"), (10, "privateField"), (25, "finalField"),
    ]]
    caller = ClassEncoding("call/Steps")

    def field(opcode, name):
        return op(opcode, caller.ref(9, "sym/Sub", name, descriptor))

    def method(opcode, name):
        return op(opcode, caller.ref(10, "sym/Sub", name, signature))

    null, pop = b"\x01", b"\x57"
    # The valid and invalid opcodes intentionally share one CP index.
    bodies = {
        "getStatic": field(0xb2, "sf") + pop,
        "getStaticWrong": null + field(0xb4, "sf") + pop,
        "getInstance": null + field(0xb4, "instanceField") + pop,
        "getInstanceWrong": field(0xb2, "instanceField") + pop,
        "privateField": field(0xb2, "privateField") + pop,
        "finalWrite": null + field(0xb3, "finalField"),
        "missingField": field(0xb2, "absent") + pop,
        "callStatic": null + method(0xb8, "publicCall"),
        "callStaticWrong": null + null + method(0xb6, "publicCall"),
        "callInstance": null + null + method(0xb6, "instanceCall"),
        "callInstanceWrong": null + method(0xb8, "instanceCall"),
        "privateCall": null + method(0xb8, "privateCall"),
        "missingCall": null + method(0xb8, "absent"),
    }
    return {
        "value/K.class": ClassEncoding("value/K").finish([], []),
        "owner/Base.class": base.finish(fields, methods),
        "sym/Sub.class": ClassEncoding("sym/Sub", "owner/Base").finish([], []),
        "call/Steps.class": caller.finish([], [
            (9, name, "()I", code + b"\x04\xac", 2, 0) for name, code in bodies.items()
        ]),
    }


def signature_order():
    base = ClassEncoding("owner/Base")
    caller = ClassEncoding("call/Steps")
    targets, callers = [], []
    for name, descriptor, arguments in [
        ("PQ", "(Lvalue/P;Lvalue/Q;)V", 2), ("QP", "(Lvalue/Q;Lvalue/P;)V", 2),
        ("PRet", "(Lvalue/Q;)Lvalue/P;", 1), ("QRet", "(Lvalue/P;)Lvalue/Q;", 1),
        ("Array", "([[Lvalue/P;[Lvalue/Q;)V", 2),
    ]:
        returns_value = not descriptor.endswith(")V")
        targets.append((9, name, descriptor, b"\x01\xb0" if returns_value else b"\xb1", 1, arguments))
        code = b"\x01" * arguments + op(0xb8, caller.ref(10, "sym/Sub", name, descriptor))
        if returns_value:
            code += b"\x57"
        callers.append((9, name, "()I", code + b"\x04\xac", 2, 0))
    return {
        "value/P.class": ClassEncoding("value/P").finish([], []),
        "value/Q.class": ClassEncoding("value/Q").finish([], []),
        "owner/Base.class": base.finish([], targets),
        "sym/Sub.class": ClassEncoding("sym/Sub", "owner/Base").finish([], []),
        "call/Steps.class": caller.finish([], callers),
    }
