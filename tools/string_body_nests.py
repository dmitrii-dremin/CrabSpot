"""Construct original class files that exercise the Java 21 nest-host access test."""
from type_state_encodings import ClassEncoding, attribute, op, table, u2


HOST = "peer/Host"
MEMBER = "peer/Host$Inner"
CASES = [
    ("genuine-names", HOST, [MEMBER], None),
    ("no-host", None, [MEMBER], None),
    ("no-members", HOST, None, None),
    ("neither", None, None, None),
    ("absent-host", "peer/Missing", [MEMBER], None),
    ("unlisted", HOST, ["peer/Missing"], None),
    ("extra-missing", HOST, ["peer/Missing", MEMBER], None),
    ("other-package", "elsewhere/Other", [MEMBER], "elsewhere/Other"),
    ("other-host", "peer/Other", [MEMBER], "peer/Other"),
]


def encode(klass, fields, methods, host=None, members=None):
    attributes = []
    if host is not None:
        attributes.append(attribute(klass.utf("NestHost"), u2(klass.cls(host))))
    if members is not None:
        name = klass.utf("NestMembers")
        members_table = table([u2(klass.cls(member)) for member in members])
        attributes.append(attribute(name, members_table))
    raw = klass.finish(fields, methods)
    if raw[-2:] != u2(0):
        raise ValueError("expected an empty class attribute table")
    return raw[:-2] + table(attributes)


def make(out, label, designated, members, additional=None):
    caller = ClassEncoding(HOST)
    store = op(0xb3, caller.ref(9, HOST, "phase", "I"))
    invoke = op(0xb8, caller.ref(10, MEMBER, "secret", "()I"))
    # Record that the call was reached, then store the returned private value.
    main = (9, "main", "([Ljava/lang/String;)V", b"\x04"+store+invoke+store+b"\xb1", 1, 1)
    files = {
        HOST: encode(caller, [(9, "phase", "I")], [main], members=members),
        MEMBER: encode(ClassEncoding(MEMBER), [], [(10, "secret", "()I", bytes.fromhex("10 07 ac"), 1, 0)],
                       host=designated),
    }
    if additional is not None:
        files[additional] = encode(ClassEncoding(additional), [], [], members=[MEMBER])
    folder = out/"nest-encoded"/label
    for name, data in files.items():
        path = folder/(name+".class")
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
    return folder
