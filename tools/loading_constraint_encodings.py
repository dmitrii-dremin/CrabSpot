"""Original class files for the Java 21 loading-constraint stage probes."""

from type_state_encodings import ClassEncoding, op, table, u2


def encode(encoding, methods, interface_indices, flags):
    data = encoding.finish([], methods, flags=flags)
    at = 10 + sum(map(len, encoding.entries)) + 6
    if data[at:at + 2] != u2(0):
        raise ValueError("expected an empty interface table in the base encoding")
    return data[:at] + table(interface_indices) + data[at + 2:]


def build():
    files = {}

    def emit(encoding, methods=(), interfaces=(), flags=0x21):
        # Allocate interface CP entries first to reproduce the original probe bytes.
        interface_indices = [u2(encoding.cls(name)) for name in interfaces]
        if encoding.name in files:
            raise ValueError(f"duplicate loading probe {encoding.name}")
        files[encoding.name] = encode(encoding, methods, interface_indices, flags)

    def take(descriptor, flags=1, stack=1, locals_=2):
        declaration = (flags, "take", descriptor)
        if flags & 0x400:
            return declaration
        body = b"\xb1" if descriptor.endswith(")V") else b"\x01\xb0"
        return (*declaration, body, stack, locals_)

    for leaf in ("K", "P", "Q"):
        emit(ClassEncoding("value/" + leaf))
    nominal = "(Lvalue/K;)V"
    for family, descriptor, base, child in [
        ("Argument", nominal, 1, 1), ("Return", "()Lvalue/K;", 1, 1),
        ("Array", "([[Lvalue/K;)[[Lvalue/K;", 1, 1), ("Primitive", "([I)[I", 1, 1),
        ("Private", nominal, 2, 1), ("Static", nominal, 9, 9), ("Package", nominal, 0, 1),
    ]:
        parent = "parent/Base" + family
        emit(ClassEncoding(parent), [take(descriptor, base)])
        emit(ClassEncoding("child/Sub" + family, parent), [take(descriptor, child)])

    service = "iface/Service"
    emit(ClassEncoding(service), [take(nominal, 0x401)], flags=0x601)
    emit(ClassEncoding("parent/Impl"), [take(nominal)])
    emit(ClassEncoding("child/Inherit", "parent/Impl"), interfaces=[service])
    emit(ClassEncoding("child/Declare"), [take(nominal)], [service])

    for kind, base_flags, child_flags in [
        ("SameSpelling", 0, 1), ("Public", 1, 1), ("PrivateChild", 1, 2),
        ("StaticChild", 1, 9), ("Protected", 4, 1), ("StaticParent", 9, 1),
    ]:
        parent = "shared/Base" + kind
        emit(ClassEncoding(parent), [take(nominal, base_flags, stack=0)])
        emit(ClassEncoding("shared/Child" + kind, parent), [take(nominal, child_flags, stack=0)])
    emit(ClassEncoding("shared/Hidden"), [take(nominal, 0, stack=0)])
    emit(ClassEncoding("shared/Bridge", "shared/Hidden"), [take(nominal, stack=0)])
    emit(ClassEncoding("other/Leaf", "shared/Bridge"), [take(nominal, stack=0)])

    for kind, desc in [
        ("First", nominal), ("Second", nominal), ("PQ", "(Lvalue/P;Lvalue/Q;)V"),
        ("QP", "(Lvalue/Q;Lvalue/P;)V"), ("PRet", "(Lvalue/Q;)Lvalue/P;"),
        ("QRet", "(Lvalue/P;)Lvalue/Q;"),
    ]:
        parent = "parent/" + kind
        emit(ClassEncoding(parent), [take(desc, locals_=3)])
        emit(ClassEncoding("child/" + kind, parent), [take(desc, locals_=3)])

    desc = "(Lvalue/K;)I"
    for kind, flags in [("Abstract", 0x401), ("Default", 1), ("Private", 2)]:
        for side, value in [("Left", 4), ("Right", 5)]:
            method = (flags, "take", desc)
            if kind != "Abstract":
                method += (bytes([value, 0xac]), 1, 2)
            emit(ClassEncoding("iface/" + side + kind), [method], flags=0x601)
    for left, right in [
        ("Abstract", "Abstract"), ("Default", "Abstract"), ("Abstract", "Default"),
        ("Default", "Default"), ("Private", "Default"), ("Default", "Private"),
    ]:
        left_name, right_name = "iface/Left" + left, "iface/Right" + right
        name = "child/" + left + right
        enc = ClassEncoding(name)
        ctor = (1, "<init>", "()V", b"\x2a" + enc.invoke_init("java/lang/Object") + b"\xb1", 1, 1)
        owner = right_name if left == "Private" else left_name
        body = enc.allocation(name) + b"\x01" + op(0xb9, enc.ref(11, owner, "take", desc)) + b"\x02\x00\xac"
        emit(enc, [ctor, (9, "run", "()I", body, 2, 0)], [left_name, right_name])

    for kind, names in enumerate([("alpha", "beta"), ("beta", "alpha"), ("take", "take")]):
        for reverse in (0, 1):
            leaf = f"Order{kind}{reverse}"
            rows = [(1, name, f"(Lvalue/{value};)V", b"\xb1", 0, 2) for name, value in zip(names, "PQ")]
            if reverse:
                rows.reverse()
            parent = "parent/" + leaf
            emit(ClassEncoding(parent), rows)
            emit(ClassEncoding("child/" + leaf, parent), rows)
    return files


def selection_edges(with_constructors):
    """Byte-exact variants for direct initialization and later reflective invocation."""
    files = {}

    def emit(name, methods=(), parent="java/lang/Object", interfaces=(), flags=0x21):
        enc = ClassEncoding(name, parent)
        indices = [u2(enc.cls(interface)) for interface in interfaces]
        methods = list(methods)
        if with_constructors and not flags & 0x200:
            code = b"\x2a" + enc.invoke_init(parent) + b"\xb1"
            methods.append((1, "<init>", "()V", code, 1, 1))
        files[name] = encode(enc, methods, indices, flags)

    def declaration(flags):
        header = (flags, "take", "(Lvalue/K;)V")
        return header if flags & 0x400 else (*header, b"\xb1", 0, 2)

    emit("value/K")
    emit("iface/Service", [declaration(0x401)], flags=0x601)
    for kind, flags in [("Abs", 0x401), ("Prot", 4), ("Package", 0),
                        ("Private", 2), ("Static", 9), ("Bridge", 0x1041)]:
        parent = "parent/" + kind
        emit(parent, [declaration(flags)], flags=0x421 if flags & 0x400 else 0x21)
        emit("child/" + kind, parent=parent, interfaces=["iface/Service"])
    emit("iface/Default", [declaration(1)], flags=0x601)
    emit("iface/Reabstract", [declaration(0x401)], interfaces=["iface/Default"], flags=0x601)
    emit("child/Suppress", interfaces=["iface/Reabstract", "iface/Service"])
    emit("child/RootInterface", interfaces=["iface/Default", "iface/Service"], flags=0x601)
    emit("child/DeclaredInterface", [declaration(0x401)], interfaces=["iface/Service"], flags=0x601)
    if with_constructors:
        for name in ("RootInterface", "DeclaredInterface"):
            emit("helper/" + name, interfaces=["child/" + name])
    return files
