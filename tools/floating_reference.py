"""Run the JDK-only floating oracle; keep raw non-NaN bits and classify NaNs."""
from numeric_reference import main


if __name__ == "__main__":
    main("FloatingCases", "floating", r"(?:[IL]:-?[0-9]+|F:(?:[0-9a-f]{8}|NaN)|D:(?:[0-9a-f]{16}|NaN))")
