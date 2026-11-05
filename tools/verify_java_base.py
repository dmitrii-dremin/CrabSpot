"""Verify every ordinary definition in the installed JDK 21 java.base.jmod with CrabSpot."""
import argparse
import hashlib
import os
from pathlib import Path
import re
import struct
import subprocess
import tempfile
import zipfile


ROOT = Path(__file__).resolve().parents[1]
MAX_ARCHIVE = 128 << 20
MAX_ENTRIES = 32_768
MAX_CLASSES = 16_384
MAX_CLASS_BYTES = 16 << 20
MAX_TOTAL_BYTES = 128 << 20
MAX_WIRE_BYTES = 144 << 20


def check_directory(stream):
    # JMOD 1.0 uses a four-byte preamble and an ordinary single-disk ZIP.
    # Bound the central directory before ZipFile constructs per-entry objects.
    if stream.read(4) != b"JM\x01\x00":
        raise ValueError("expected a JMOD 1.0 archive")
    stream.seek(0, 2)
    size = stream.tell()
    if not 26 <= size <= MAX_ARCHIVE:
        raise ValueError("archive size outside corpus policy")
    stream.seek(-22, 2)
    signature, disk, start, on_disk, count, length, offset, comment = struct.unpack("<4s4H2IH", stream.read(22))
    if (signature != b"PK\x05\x06" or disk or start or comment or on_disk != count
            or not 0 < count <= MAX_ENTRIES or length > 8 << 20 or offset + length + 26 != size):
        raise ValueError("unsupported or oversized JMOD central directory")
    stream.seek(4 + offset)
    end = size - 22
    for _ in range(count):
        header = stream.read(46)
        if len(header) != 46 or header[:4] != b"PK\x01\x02":
            raise ValueError("invalid JMOD central directory record")
        name, extra, comment = struct.unpack_from("<3H", header, 28)
        if not 0 < name <= 4096 or stream.tell() + name + extra + comment > end:
            raise ValueError("invalid JMOD central directory lengths")
        stream.seek(name + extra + comment, 1)
    if stream.tell() != end:
        raise ValueError("JMOD central directory count differs from declared count")


def pack_archive(archive, packed):
    with archive.open("rb") as stream:
        check_directory(stream)
        stream.seek(0)
        archive_hash = hashlib.file_digest(stream, "sha256").hexdigest()
        with zipfile.ZipFile(stream) as jar:
            names, entries = set(), []
            total, wire_size = 0, 4
            for entry in jar.infolist():
                path = entry.orig_filename
                if path in names or "\x00" in path or "\\" in path or any(
                        component in ("", ".", "..") for component in path.removesuffix("/").split("/")):
                    raise ValueError(f"duplicate or invalid archive path: {path!r}")
                names.add(path)
                if not (path.startswith("classes/") and path.endswith(".class")):
                    continue
                name = path[len("classes/"):-len(".class")]
                if not name or any(c in name for c in ".;[") or name.endswith("/"):
                    raise ValueError(f"invalid binary class name: {path!r}")
                encoded = name.encode("utf-8")
                if not 0 < entry.file_size <= MAX_CLASS_BYTES or len(entries) >= MAX_CLASSES:
                    raise ValueError("java.base class size or count exceeds policy")
                total += entry.file_size
                wire_size += 6 + len(encoded) + entry.file_size
                if total > MAX_TOTAL_BYTES or wire_size > MAX_WIRE_BYTES:
                    raise ValueError("java.base aggregate input exceeds policy")
                entries.append((encoded, entry))
            if "classes/module-info.class" not in names or len(entries) <= 1:
                raise ValueError("missing java.base module descriptor or ordinary definitions")
            with packed.open("xb") as output:
                output.write(struct.pack(">I", len(entries)))
                for encoded, entry in sorted(entries, key=lambda pair: pair[0]):
                    output.write(struct.pack(">HI", len(encoded), entry.file_size))
                    output.write(encoded)
                    with jar.open(entry) as reader:
                        remaining = entry.file_size
                        while remaining:
                            chunk = reader.read(min(remaining, 64 << 10))
                            if not chunk:
                                raise ValueError(f"truncated class entry: {entry.filename}")
                            output.write(chunk)
                            remaining -= len(chunk)
                        if reader.read(1):
                            raise ValueError(f"oversized class entry: {entry.filename}")
                if output.tell() != wire_size:
                    raise ValueError("inconsistent packed byte count")
    return archive_hash, len(entries) - 1, total


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--jdk", type=Path, default=os.environ.get("JAVA_HOME"))
    args = parser.parse_args()
    if not args.jdk:
        parser.error("set JAVA_HOME or pass --jdk for a reference JDK 21")
    try:
        java = args.jdk / "bin" / ("java.exe" if os.name == "nt" else "java")
        version = subprocess.run([java, "-version"], capture_output=True, text=True, check=True, timeout=30)
        runtime = (version.stdout + version.stderr).strip()
        if not re.match(r'(?:openjdk|java) version "21(?:[."+\-])', runtime):
            raise ValueError(f"expected JDK 21, found {runtime!r}")
        directory = ROOT / "target/fixture-tool"
        directory.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="java-base-", dir=directory) as tmp:
            packed = Path(tmp) / "class-inputs"
            sha, count, size = pack_archive(args.jdk / "jmods/java.base.jmod", packed)
            print(f"{runtime}\njava.base.jmod sha256={sha} ordinary_classes={count} class_bytes={size}", flush=True)
            subprocess.run(["cargo", "run", "--release", "--locked", "-p", "crabspot-verifier",
                            "--example", "verify_java_base", "--", str(packed)], cwd=ROOT, check=True)
    except (OSError, ValueError, zipfile.BadZipFile, subprocess.SubprocessError) as exc:
        parser.exit(1, f"java.base verification failed: {exc}\n")


if __name__ == "__main__":
    main()
