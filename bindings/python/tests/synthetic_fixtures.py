"""Build deliberately synthetic credentials from readable test-only recipes."""


def expand_fixture(row):
    row = dict(row)
    recipes = row.pop("synthetic_credentials", {})

    def replace(value, marker, credential):
        if isinstance(value, str):
            return value.replace(marker, credential)
        if isinstance(value, list):
            return [replace(item, marker, credential) for item in value]
        if isinstance(value, dict):
            return {key: replace(item, marker, credential) for key, item in value.items()}
        return value

    for name, recipe in recipes.items():
        prefix, unit, repeat = recipe["prefix"], recipe["body_unit"], recipe["repeat"]
        assert prefix in {"sk_test_", "sk_live_", "rk_test_", "rk_live_"}
        assert unit and unit.isascii() and unit.isalnum()
        assert isinstance(repeat, int) and 1 <= repeat <= 1000
        credential = prefix + unit * repeat
        row = replace(row, "{{synthetic:" + name + "}}", credential)
    return row
