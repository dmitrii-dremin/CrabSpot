"""Generate/check fresh-loader Java 21 construction outcomes."""
from numeric_reference import main

if __name__ == "__main__":
    main("ConstructionCases", "construction", r"(?:[IL]:-?[0-9]+|F:[0-9a-f]{8}|D:[0-9a-f]{16}|T:first|R:(?:escaped|null))")
