"""Generate/check Java 21 checked-reference-field reference outcomes."""
from numeric_reference import main

if __name__ == "__main__":
    main("ReferenceFieldCases", "reference_fields", r"(?:I:-?[0-9]+|E:java/lang/(?:ClassCastException|NegativeArraySizeException|ArrayIndexOutOfBoundsException|ArithmeticException|NullPointerException))")
