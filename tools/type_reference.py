"""Generate/check Java 21 cast and type-test reference outcomes."""
from numeric_reference import main

if __name__ == "__main__":
    main("CastCases", "types", r"(?:I:-?[0-9]+|E:java/lang/(?:ClassCastException|NegativeArraySizeException))")
