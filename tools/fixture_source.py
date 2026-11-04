"""Read canonical fixture source bytes without silently changing their hashes."""
from pathlib import Path


def read_java_source(path: Path) -> bytes:
    data = path.read_bytes()
    if b"\r" in data:
        raise ValueError(f"{path}: Java fixture source must use LF line endings (see .gitattributes); convert CRLF/bare CR before generating provenance")
    return data
