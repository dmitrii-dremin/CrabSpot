"""Generate/check Java 21 supplied-Throwable dispatch outcomes."""
from numeric_reference import main

if __name__ == "__main__":
    main("ExceptionDispatchCases", "exceptions", r"(?:I:-?[0-9]+|T:(?:first|second))")
