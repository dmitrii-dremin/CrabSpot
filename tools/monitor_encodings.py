"""Original Java 21 bytecode for monitor behavior that javac cannot express."""
from type_state_encodings import ClassEncoding, full_frame, op


DESC = "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/RuntimeException;)I"


def raw_case(case):
    c = ClassEncoding("locks/Raw")
    state = c.ref(9, c.name, "phase", "I")

    def phase(number):
        return bytes((0x10, number)) + op(0xb3, state)

    enter_a, exit_a = b"\x2a\xc2", b"\x2a\xc3"
    enter_b, exit_b = b"\x2b\xc2", b"\x2b\xc3"
    held = op(0xb8, c.ref(10, "java/lang/Thread", "holdsLock", "(Ljava/lang/Object;)Z"))
    finish = phase(3) + b"\x10\x29\xac"
    run = c.ref(10, c.name, "inner", DESC)
    invoke = b"\x2a\x2b\x2c" + op(0xb8, run)
    inner = exit_a + phase(2) + b"\x04\xac"
    cases = {
        "balanced": enter_a + phase(2) + exit_a + finish,
        "reentrant": enter_a * 2 + exit_a + b"\x2a" + held + b"\x3e" + exit_a + b"\x1d\xac",
        "crossed": enter_a + enter_b + exit_a + exit_b + finish,
        "unowned": exit_a + finish,
        "null-enter": b"\x01\xc2" + finish, "null-exit": b"\x01\xc3" + finish,
        "wrong-receiver": enter_a + phase(2) + exit_b + finish,
        "unbalanced-return": enter_a + finish,
        "unbalanced-throw": enter_a + phase(2) + b"\x2c\xbf",
        "nested-exit": enter_a + invoke + b"\x57" + exit_a + finish,
        "sync-held": b"\x2a" + held + b"\xac",
        "sync-reenter": enter_a + exit_a + b"\x2a" + held + b"\xac",
        "sync-release": exit_a + finish,
        "sync-release-throw": exit_a + phase(2) + b"\x2c\xbf",
        "sync-supplied-throw": phase(2) + b"\x2c\xbf",
        "sync-leak": enter_a + finish,
        "unfinished": op(0xbb, c.cls("java/lang/Object")) + b"\x59\x4e\xc2\x2d\xc3\x2d"
                      + c.invoke_init("java/lang/Object") + finish,
    }
    flags = 0x29 if case.startswith("sync-") else 9
    return c.finish([(9, "phase", "I")], [
        (9, "inner", DESC, inner, 2, 3),
        (flags, "run", DESC, phase(1) + cases[case], 3, 4),
    ])


RAW_CASES = (
    "balanced", "reentrant", "crossed", "unowned", "null-enter", "null-exit", "wrong-receiver",
    "unbalanced-return", "unbalanced-throw", "nested-exit", "sync-held", "sync-reenter", "sync-release",
    "sync-release-throw", "sync-supplied-throw", "sync-leak", "unfinished",
)


PARAMETERS = "(Ljava/lang/Object;Ljava/lang/RuntimeException;)"


RETURNS = {
    "void": ("V", "b1"), "int": ("I", "08 ac"), "long": ("J", "0a ad"),
    "float": ("F", "0d ae"), "double": ("D", "0f af"), "ref": ("Ljava/lang/Object;", "2a b0"),
}


def return_case(shape, mode):
    c = ClassEncoding("locks/Returns")
    enter, release = bytes.fromhex("2a c2"), bytes.fromhex("2a c3")
    held = op(0xb8, c.ref(10, "java/lang/Thread", "holdsLock", "(Ljava/lang/Object;)Z"))
    record = b"\x2a" + held + op(0xb3, c.ref(9, c.name, "heldInside", "I"))
    descriptor, ret = RETURNS[shape]
    handlers, frames = [], None
    if mode.startswith("catch"):
        # handler_pc=4, initial argument locals survive, stack contains the supplied exception
        exc = c.cls("java/lang/RuntimeException")
        body = enter + bytes.fromhex("2b bf") + record + release
        body += b"\xbf" if mode == "catch-rethrow" else bytes.fromhex("57") + bytes.fromhex(ret)
        handlers = [(2, 4, 4, exc)]
        frames = full_frame(4, [op(7, c.cls("java/lang/Object")), op(7, exc)], [op(7, exc)])
    else:
        body = enter + record + (release if mode == "balanced" else b"") + bytes.fromhex(ret)
    return c.finish([(9, "heldInside", "I")],
                    [(9, "run", PARAMETERS + descriptor, body, 3, 2, handlers, frames)])


def flag_case(init_flags, ctor_flags):
    c = ClassEncoding("locks/Flags")
    holds = op(0xb8, c.ref(10, "java/lang/Thread", "holdsLock", "(Ljava/lang/Object;)Z"))
    check = op(0x13, c.cls(c.name)) + holds
    def put(name):
        return op(0xb3, c.ref(9, c.name, name, "I"))

    invoke_sync = op(0xb8, c.ref(10, c.name, "test", "()V"))
    constructor = b"\x2a" + c.invoke_init("java/lang/Object") + b"\xb1"
    return c.finish([(9, "init", "I"), (9, "call", "I")], [
        (init_flags, "<clinit>", "()V", check + put("init") + invoke_sync + b"\xb1", 1, 0),
        (0x29, "test", "()V", check + put("call") + b"\xb1", 1, 0),
        (ctor_flags, "<init>", "()V", constructor, 1, 1)])


ENTRY_CASES = (
    "balanced", "crossed", "recursive", "null-enter", "null-exit", "unowned",
    "wrong-object", "leak", "callee-exit", "unfinished", "synchronized-instance-exit",
)


def entry_class(case):
    c = ClassEncoding("locks/RawEntry")
    phase_field = c.ref(9, c.name, "phase", "I")

    def phase(n):
        return bytes([0x10, n]) + op(0xb3, phase_field)

    load_a, load_b = b"\x2a", b"\x2b"
    enter, leave, ret = b"\xc2", b"\xc3", b"\xb1"
    # args is a real reference array; the second identity is a newly allocated int[].
    other = op(0xb3, c.ref(9, c.name, "other", "Ljava/lang/Object;"))
    init = bytes.fromhex("04 bc 0a 59 4c") + other + phase(1)
    enter_a, leave_a = load_a + enter, load_a + leave
    enter_b, leave_b = load_b + enter, load_b + leave
    object_name = "java/lang/Object"
    constructor = (1, "<init>", "()V", load_a + c.invoke_init(object_name) + ret, 1, 1)
    steal = (9, "steal", "(Ljava/lang/Object;)V", leave_a + phase(2) + ret, 1, 1)
    drop = (0x21, "drop", "()V", leave_a + phase(2) + ret, 1, 1)
    cases = {
        "balanced": enter_a + phase(2) + leave_a,
        "crossed": enter_a + enter_b + leave_a + leave_b,
        "recursive": enter_b + enter_b + leave_b + phase(2) + leave_b,
        "null-enter": b"\x01" + enter,
        "null-exit": b"\x01" + leave,
        "unowned": leave_b,
        "wrong-object": enter_a + phase(2) + leave_b,
        "leak": enter_a,
        "callee-exit": enter_a + load_a + op(0xb8, c.ref(10, c.name, "steal", steal[2])) + leave_a,
        "unfinished": op(0xbb, c.cls(object_name)) + bytes.fromhex("59 4d c2 2c c3 2c") + c.invoke_init(object_name),
        "synchronized-instance-exit": c.allocation(c.name) + op(0xb6, c.ref(10, c.name, "drop", "()V")),
    }
    body = init + cases[case] + phase(3) + ret
    main = (9, "main", "([Ljava/lang/String;)V", body, 3, 3)
    return c.finish([(9, "phase", "I"), (9, "other", "Ljava/lang/Object;")], [constructor, steal, drop, main])
