import sodium from "libsodium-wrappers-sumo";

// Persistent profile identifiers: never tune these values in place.
const memory = new Map([[1, 64 * 1024 * 1024], [2, 256 * 1024 * 1024]]);
let started = false;
self.onmessage = async ({ data }) => {
  if (started) return; // One operation per worker; the caller terminates it.
  started = true;
  let password, deriving = false;
  try {
    const profile = data.profile;
    if (typeof data.password !== "string" || !data.password.isWellFormed() || !memory.has(profile) ||
        Object.keys(data).some(key => !["password", "salt", "profile"].includes(key)) ||
        !(data.salt instanceof Uint8Array) || data.salt.length !== 16) throw new Error();
    password = new TextEncoder().encode(data.password);
    if (password.length === 0 || password.length > 1024) throw new Error();
    deriving = true;
    await sodium.ready;
    // libsodium Argon2id v1.3 uses one lane; allocation failure never retries cheaper.
    const key = sodium.crypto_pwhash(32, password, data.salt, 3, memory.get(profile), sodium.crypto_pwhash_ALG_ARGON2ID13);
    self.postMessage({ key }, [key.buffer]);
  } catch { self.postMessage({ error: deriving ? "kdf_unavailable" : "invalid_credentials" }); }
  finally { password?.fill(0); }
};
