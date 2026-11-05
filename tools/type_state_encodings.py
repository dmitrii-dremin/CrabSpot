"""Original Java 21 verifier probes. All bytecode is authored for these fixtures."""
from struct import pack


def u2(value):
    return pack(">H", value)


def u4(value):
    return pack(">I", value)


def op(opcode, index):
    return bytes([opcode]) + u2(index)


OBJECT = "java/lang/Object"
THROWABLE = "java/lang/Throwable"
RETURN_ONE = (9, "run", "()I", bytes.fromhex("04 ac"), 1, 0)


def attribute(name_index, body):
    return u2(name_index) + u4(len(body)) + body


def table(rows):
    return u2(len(rows)) + b"".join(rows)


class ClassEncoding:
    def __init__(self, name, parent=OBJECT):
        self.name = name
        self.parent = parent
        self.entries = {}

    def constant(self, data):
        return self.entries.setdefault(data, len(self.entries) + 1)

    def utf(self, text):
        data = text.encode("ascii")
        return self.constant(b"\x01" + u2(len(data)) + data)

    def cls(self, name):
        return self.constant(op(7, self.utf(name)))

    def ref(self, tag, owner, name, descriptor):
        owner_index = self.cls(owner)
        name_type = self.constant(op(12, self.utf(name)) + u2(self.utf(descriptor)))
        return self.constant(op(tag, owner_index) + u2(name_type))

    def invoke_init(self, owner, descriptor="()V"):
        return op(0xb7, self.ref(10, owner, "<init>", descriptor))

    def allocation(self, owner):
        return op(0xbb, self.cls(owner)) + b"\x59" + self.invoke_init(owner)

    def method(self, flags, name, descriptor, code=None, stack=0, locals_=0, handlers=(), frame=None):
        header = u2(flags) + u2(self.utf(name)) + u2(self.utf(descriptor))
        if code is None:
            return header + u2(0)
        code_name = self.utf("Code")
        attributes = [] if frame is None else [attribute(self.utf("StackMapTable"), frame)]
        code_body = (u2(stack) + u2(locals_) + u4(len(code)) + code
                     + table([pack(">4H", *row) for row in handlers]) + table(attributes))
        return header + table([attribute(code_name, code_body)])

    def finish(self, fields, methods, *, major=65, minor=0, flags=0x21):
        this, parent = self.cls(self.name), self.cls(self.parent)
        fields = [pack(">4H", flags, self.utf(name), self.utf(desc), 0)
                  for flags, name, desc in fields]
        methods = [self.method(*method) for method in methods]
        return (bytes.fromhex("cafebabe") + pack(">3H", minor, major, len(self.entries) + 1)
                + b"".join(self.entries) + pack(">4H", flags, this, parent, 0)
                + table(fields) + table(methods) + u2(0))


def full_frame(pc, locals_, stack):
    return u2(1) + b"\xff" + u2(pc) + table(locals_) + table(stack)


def ctor(code, stack=1, locals_=1, *extra):
    return (1, "<init>", "()V", code, stack, locals_, *extra)


def run(code, stack, locals_=0, *extra):
    return (9, "run", "()I", code, stack, locals_, *extra)


def post_super(c):
    field = c.ref(9, c.name, "value", "I")
    code = bytes.fromhex("2a 4c 2a") + c.invoke_init(OBJECT) + b"\x2b" + op(0xb4, field)
    return [(1, "value", "I")], [ctor(code + bytes.fromhex("57 b1"), 1, 2), RETURN_ONE]


def caught_helper(c):
    call = op(0xb8, c.ref(10, c.name, "helper", "()V"))
    chain = b"\x2a" + c.invoke_init(OBJECT) + b"\xb1"
    catch = c.cls(THROWABLE)
    frame = full_frame(8, [b"\x06"], [op(7, catch)])
    body = ctor(call + chain + b"\x57" + chain, 1, 1, [(0, 3, 8, catch)], frame)
    return [], [body, (9, "helper", "()V", bytes.fromhex("01 bf"), 1, 0), RETURN_ONE]


def monitor_this(c):
    code = bytes.fromhex("2a 59 c2 c3 2a") + c.invoke_init(OBJECT) + b"\xb1"
    return [], [ctor(code, 2), RETURN_ONE]


def constructor_cases(add):
    for base, original, make in [
        ("CaughtHelperBefore", "Chain", caught_helper),
        ("MonitorUninitializedThis", "", monitor_this),
        ("PostSuperAlias", "CurrentType", post_super),
    ]:
        for suffix in [original, "Executed"]:
            c = ClassEncoding("probe/" + base + suffix)
            fields, methods = make(c)
            if suffix == "Executed":
                methods[-1] = run(c.allocation(c.name) + bytes.fromhex("57 04 ac"), 2)
            add(c, fields, methods, "<init>", "()V")

    for owner in ["Super", "This"]:
        for kind in ["Top", "UninitializedThis", "CurrentClass"]:
            c = ClassEncoding("probe/Caught" + owner + "Chain" + kind)
            if owner == "Super":
                chain = b"\x2a" + c.invoke_init(OBJECT)
            else:
                chain = bytes.fromhex("2a 03") + c.invoke_init(c.name, "(I)V")
            catch = c.cls(THROWABLE)
            local = {"Top": b"\x00", "UninitializedThis": b"\x06"}.get(kind)
            if local is None:
                local = op(7, c.cls(c.name))
            handler_pc = len(chain) + 1
            frame = full_frame(handler_pc, [local], [op(7, catch)])
            handlers = [(len(chain) - 3, len(chain), handler_pc, catch)]
            methods = [ctor(chain + bytes.fromhex("b1 bf"), 1 if owner == "Super" else 2,
                            1, handlers, frame)]
            if owner == "This":
                chain_to_object = b"\x2a" + c.invoke_init(OBJECT) + b"\xb1"
                methods.append((1, "<init>", "(I)V", chain_to_object, 1, 2))
            add(c, [], methods + [RETURN_ONE], "<init>", "()V")


def reference_cases(add):
    for prefix in ["Object", "PrimitiveArray"]:
        c = ClassEncoding("probe/" + prefix + "AsInterfaceReturn")
        value = c.allocation(OBJECT) if prefix == "Object" else bytes.fromhex("03 bc 0a")
        descriptor = "()Ljava/lang/Runnable;"
        invoke = op(0xb8, c.ref(10, c.name, "value", descriptor))
        add(c, [], [(9, "value", descriptor, value + b"\xb0", 2, 0),
                    run(invoke + bytes.fromhex("57 04 ac"), 1)], "value", descriptor)
    c = ClassEncoding("probe/ObjectInterfaceInvoke")
    code = c.allocation(OBJECT) + op(0xb9, c.ref(11, "java/lang/Runnable", "run", "()V"))
    add(c, [], [run(code + bytes.fromhex("01 00 04 ac"), 2)])
    c = ClassEncoding("probe/RuntimeAastoreComponentMismatch")
    array = b"\x04" + op(0xbd, c.cls("java/lang/String"))
    add(c, [], [run(array + b"\x03" + c.allocation(OBJECT) + bytes.fromhex("53 04 ac"), 4)])
    c = ClassEncoding("probe/MonitorUninitializedNew")
    add(c, [], [run(op(0xbb, c.cls(OBJECT)) + bytes.fromhex("59 c2 c3 04 ac"), 2)])
    for null in [True, False]:
        c = ClassEncoding("probe/" + ("Null" if null else "NonNull") + "MissingToObject")
        source = b"\x01" if null else c.allocation(OBJECT)
        code = source + op(0xc0, c.cls("absent/Missing")) + b"\xb0"
        add(c, [], [(9, "run", "()Ljava/lang/Object;", code, 2, 0)], "run", "()Ljava/lang/Object;")


def allocation_cases(add):
    for kind in ["Top", "Uninitialized", "Initialized"]:
        for action in ["Rethrow", "Return", "Load", "Retry"]:
            c = ClassEncoding("probe/Allocated" + kind + action)
            owner, init, catch = c.cls(c.name), c.invoke_init(c.name), c.cls(THROWABLE)
            throwing = b"\x2a" + c.invoke_init(OBJECT) + bytes.fromhex("01 bf")
            code = op(0xbb, owner) + bytes.fromhex("59 4b") + init + bytes.fromhex("04 ac")
            handler = {"Rethrow": b"\xbf", "Return": bytes.fromhex("57 05 ac"),
                       "Load": bytes.fromhex("57 2a 57 05 ac"),
                       "Retry": bytes.fromhex("57 2a") + init + bytes.fromhex("05 ac")}[action]
            local = {"Top": b"\x00", "Uninitialized": op(8, 0), "Initialized": op(7, owner)}[kind]
            frame = full_frame(len(code), [local], [op(7, catch)])
            add(c, [], [ctor(throwing), run(code + handler, 2, 1, [(5, 8, 10, catch)], frame)])

        c = ClassEncoding("probe/BeforeChain" + kind)
        broken = ClassEncoding(c.name + "Broken")
        throwing = b"\x2a" + broken.invoke_init(OBJECT) + bytes.fromhex("01 bf")
        add(broken, [], [ctor(throwing)], None)
        owner, catch = c.cls(broken.name), c.cls(THROWABLE)
        chain = b"\x2a" + c.invoke_init(OBJECT) + b"\xb1"
        code = op(0xbb, owner) + bytes.fromhex("59 4c") + c.invoke_init(broken.name) + chain
        local = {"Top": b"\x00", "Uninitialized": op(8, 0), "Initialized": op(7, owner)}[kind]
        frame = full_frame(len(code), [b"\x06", local], [op(7, catch)])
        body = ctor(code + b"\x57" + chain, 2, 2, [(5, 8, 13, catch)], frame)
        add(c, [], [body, run(c.allocation(c.name) + bytes.fromhex("57 04 ac"), 2)], "<init>", "()V")


def handler_cases(add):
    for name, handler in [("TopReturn", "b1"), ("TopPopReturn", "57 b1"),
                          ("TopStoreReturn", "4b b1"), ("ThrowablePopReturn", "57 b1")]:
        c = ClassEncoding("probe/" + name)
        token = b"\x00" if name.startswith("Top") else op(7, c.cls(THROWABLE))
        frame = u2(1) + bytes([66]) + token  # same_locals_1_stack_item at offset 2
        code = bytes.fromhex("01 bf " + handler)
        add(c, [], [(9, "run", "()V", code, 1, 1, [(0, 2, 2, 0)], frame)], "run", "()V")


def protected_cases(add):
    c = ClassEncoding("p/Base")
    field = c.ref(9, c.name, "value", "I")
    chain = b"\x2a" + c.invoke_init(OBJECT) + bytes.fromhex("2a 10 07") + op(0xb5, field) + b"\xb1"
    method = (4, "value", "()I", b"\x2a" + op(0xb4, field) + b"\xac", 1, 1)
    add(c, [(4, "value", "I")], [ctor(chain, 2), method], None)
    c = ClassEncoding("q/Middle", "p/Base")
    add(c, [], [ctor(b"\x2a" + c.invoke_init(c.parent) + b"\xb1")], None)
    for package, own in [("p", False), ("q", False), ("q", True)]:
        for kind in ["Field", "Method"]:
            c = ClassEncoding(package + "/Child" + ("Own" if own else "") + kind, "q/Middle")
            code = c.allocation(c.name if own else c.parent)
            if kind == "Field":
                code += op(0xb4, c.ref(9, c.parent, "value", "I"))
            else:
                code += op(0xb6, c.ref(10, c.parent, "value", "()I"))
            methods = [run(code + b"\xac", 2)]
            if own:
                methods.append(ctor(b"\x2a" + c.invoke_init(c.parent) + b"\xb1"))
            add(c, [], methods)


def final_override_cases(add):
    for name, flags in [("PkgFinal", 0x10), ("PublicFinal", 0x11), ("ProtectedFinal", 0x14),
                        ("PrivateFinal", 0x12), ("StaticFinal", 0x19)]:
        c = ClassEncoding("p/" + name)
        add(c, [], [(flags, "value", "()I", bytes.fromhex("04 ac"), 1, 0 if flags & 8 else 1)], None)
    for name in ["MiddleDeclares", "MiddleInherits"]:
        c = ClassEncoding("q/" + name, "p/PkgFinal")
        methods = [(1, "value", "()I", bytes.fromhex("05 ac"), 1, 1)] if name == "MiddleDeclares" else []
        add(c, [], methods, None)
    for name, parent, flags in [
        ("p/PkgChild", "p/PkgFinal", 1), ("q/PkgChild", "p/PkgFinal", 1),
        ("q/PublicChild", "p/PublicFinal", 1), ("q/ProtectedChild", "p/ProtectedFinal", 1),
        ("p/PrivateParentChild", "p/PrivateFinal", 1), ("q/PrivateChild", "p/PublicFinal", 2),
        ("q/StaticChild", "p/PublicFinal", 9), ("q/StaticHiding", "p/StaticFinal", 9),
        ("q/InstanceOverStatic", "p/StaticFinal", 1),
        ("p/BridgeDeclaresChild", "q/MiddleDeclares", 1), ("q/BridgeDeclaresChild", "q/MiddleDeclares", 1),
        ("p/BridgeInheritsChild", "q/MiddleInherits", 1), ("q/BridgeInheritsChild", "q/MiddleInherits", 1),
    ]:
        c = ClassEncoding(name, parent)
        method = (flags, "value", "()I", bytes.fromhex("05 ac"), 1, 0 if flags & 8 else 1)
        add(c, [], [method, RETURN_ONE], "value", "()I")


def build():
    files, selected = {}, {}

    def add(encoding, fields, methods, name="run", descriptor="()I"):
        if encoding.name in files:
            raise ValueError(f"duplicate fixture class {encoding.name}")
        files[encoding.name] = encoding.finish(fields, methods)
        if name is not None:
            selected[encoding.name] = (name, descriptor)

    for group in (constructor_cases, reference_cases, allocation_cases, handler_cases,
                  protected_cases, final_override_cases):
        group(add)
    return files, selected
