"""Generate/check fresh-loader cross-class static-call Java 21 outcomes."""
from numeric_reference import main

if __name__ == "__main__":
    main("CrossStaticCases", "cross_static", r"(?:[IL]:-?[0-9]+|F:[0-9a-f]{8}|D:[0-9a-f]{16}|T:(?:first|second))")
