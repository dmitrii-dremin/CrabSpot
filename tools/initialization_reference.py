"""Generate/check fresh-identity Java 21 initialization and raw numeric-CV outcomes."""
from numeric_reference import main

if __name__ == "__main__":
    main("InitializationCases", "initialization", r"(?:I:-?[0-9]+|T:first)")
