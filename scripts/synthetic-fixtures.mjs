// Construct explicitly synthetic credentials at test time, never provider-issued secrets.
export function expandSyntheticFixture(row) {
  const {synthetic_credentials: recipes = {}, ...data} = row;
  function replace(value, marker, credential) {
    if (typeof value === "string") return value.replaceAll(marker, credential);
    if (Array.isArray(value)) return value.map(item => replace(item, marker, credential));
    if (value && typeof value === "object") return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, replace(item, marker, credential)]));
    return value;
  }
  let expanded = data;
  for (const [name, {prefix, body_unit: unit, repeat}] of Object.entries(recipes)) {
    if (!["sk_test_", "sk_live_", "rk_test_", "rk_live_"].includes(prefix) || !/^[A-Za-z0-9]+$/.test(unit) || !Number.isInteger(repeat) || repeat < 1 || repeat > 1000) throw new Error("Invalid synthetic fixture recipe");
    expanded = replace(expanded, `{{synthetic:${name}}}`, prefix + unit.repeat(repeat));
  }
  return expanded;
}
