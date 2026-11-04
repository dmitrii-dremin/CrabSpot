"""Generate/check Java 21 reference-argument and result outcomes."""
from numeric_reference import main

if __name__ == "__main__":
    main("ReferenceCallCases", "reference_calls", r"(?:I:-?[0-9]+|E:java/lang/(?:ArrayStoreException|ArrayIndexOutOfBoundsException|NullPointerException))")
