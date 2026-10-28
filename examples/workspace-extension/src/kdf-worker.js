import sodium from "libsodium-wrappers-sumo";

self.onmessage = async ({ data }) => {
  const password = new TextEncoder().encode(data.password);
  try {
    if (!data.password?.isWellFormed() || password.length === 0 || password.length > 1024 || !(data.salt instanceof Uint8Array) || data.salt.length !== 16) throw new Error();
    await sodium.ready;
    const key = sodium.crypto_pwhash(32, password, data.salt, 3, 64 * 1024 * 1024, sodium.crypto_pwhash_ALG_ARGON2ID13);
    sodium.memzero(password);
    self.postMessage({ key }, [key.buffer]);
  } catch { password.fill(0); self.postMessage({ error: "invalid_credentials" }); }
};
