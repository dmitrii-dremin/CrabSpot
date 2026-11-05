"""Build exact Java 21 CONSTANT_String inputs, including code units javac rarely emits."""
from type_state_encodings import ClassEncoding, op, u2

CASES = {
    "zero": ((), 0x12),
    "latin": ((0, 1, 255), 0x12),
    "isolated-both": ((0xdfff, 0, 0xd800), 0x13),
    "paired": ((0xd83d, 0xde80), 0x13),
    "composed": ((233,), 0x12),
    "decomposed": ((101, 769), 0x13),
    "max-mutf": ((0xdfff,) * 21845, 0x13),
    "wrong-category": ((0, 0xd800), 0x14),
}


def modified_utf8(units):
    data = bytearray()
    for unit in units:
        if not 0 <= unit <= 0xffff:
            raise ValueError("expected one UTF-16 code unit")
        if 0 < unit < 0x80:
            data.append(unit)
        elif unit < 0x800:
            data.extend((0xc0 | (unit >> 6), 0x80 | (unit & 63)))
        else:
            data.extend((0xe0 | (unit >> 12), 0x80 | ((unit >> 6) & 63), 0x80 | (unit & 63)))
        if len(data) > 65535:
            raise ValueError("modified UTF-8 exceeds CONSTANT_Utf8 capacity")
    return bytes(data)


def literal_class(units, opcode):
    owner = ClassEncoding("sample/Value")
    if opcode != 0x12:
        for n in range(258):
            owner.utf(f"unused{n}")
    data = modified_utf8(units)
    utf = owner.constant(b"\x01" + u2(len(data)) + data)
    index = owner.constant(op(8, utf))
    if (index > 255) != (opcode != 0x12):
        raise ValueError("unexpected String constant-pool width")
    load = bytes((opcode, index)) if opcode == 0x12 else op(opcode, index)
    return owner.finish([], [(9, "value", "()Ljava/lang/String;", load + b"\xb0", 2, 0)])


def field_readers():
    classes = {}
    for package, target, field, method in (("texts", "Holder", "VALUE", "value"), ("split", "Late", "VALUE", "read")):
        reader = ClassEncoding(package+"/Read")
        code = op(0xb2, reader.ref(9, package+"/"+target, field, "Ljava/lang/String;")) + b"\xb0"
        classes[reader.name+".class"] = reader.finish([], [(9, method, "()Ljava/lang/String;", code, 1, 0)])
    reader = ClassEncoding("constants/Read")
    methods = []
    for method, field in (("first", "FIRST"), ("second", "SECOND")):
        code = op(0xb2, reader.ref(9, "constants/Entry", field, "Ljava/lang/String;")) + b"\xb0"
        methods.append((9, method, "()Ljava/lang/String;", code, 1, 0))
    classes[reader.name+".class"] = reader.finish([], methods)
    return classes


def duplicate_literals():
    """Two distinct String and UTF8 entries, with equal units after classfile emission."""
    c = ClassEncoding("duplicates/Entry")
    spelling = modified_utf8((120, 0, 0xdfff, 49))
    sentinel = spelling[:-1]+b"2"
    utf = b"\x01"+u2(len(spelling))
    first = c.constant(op(8, c.constant(utf+spelling)))
    second = c.constant(op(8, c.constant(utf+sentinel)))
    get = lambda name: c.ref(9, c.name, name, "Ljava/lang/String;")
    store_phase = op(0xb3, c.ref(9, c.name, "phase", "I"))
    code = (op(0x13, first) + op(0xb3, get("first")) + b"\x04" + store_phase +
            bytes((0x12, second)) + op(0xb3, get("second")) +
            op(0xb2, get("first")) + op(0xb2, get("second")) + b"\xa6\x00\x07\x05" + store_phase + b"\xb1")
    frame = u2(1) + bytes([len(code)-1])
    fields = [(9, name, "Ljava/lang/String;") for name in ("first", "second")] + [(9, "phase", "I")]
    blob = c.finish(fields, [(9, "main", "([Ljava/lang/String;)V", code, 2, 1, (), frame)])
    if blob.count(utf+sentinel) != 1 or blob.count(utf+spelling) != 1:
        raise ValueError("ambiguous duplicate String encoding")
    return blob.replace(utf+sentinel, utf+spelling)
