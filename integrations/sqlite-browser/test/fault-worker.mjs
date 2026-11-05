// Narrow fault injection around real packaged SQLite3MC/OPFS execution.
const mode = new URL(location.href).searchParams.get('fault');
let saving, armed = false, credentialArmed = false;
const waiting = [];
const send = self.postMessage.bind(self);
self.addEventListener('message', event => {
  const {data} = event;
  if (data.method === 'changePassword') credentialArmed = true;
  if (!self.onmessage) waiting.push(event);
  if (data.method === 'save') { saving = data.id; armed = true; }
});
if (mode === 'init') {
  const fetchAsset = self.fetch.bind(self);
  self.fetch = async (...args) => {
    if (String(args[0]).endsWith('.wasm')) { send({id: 1, error: {code: 'storage_failed'}}); return new Promise(() => {}); }
    return fetchAsset(...args);
  };
}
const put = IDBObjectStore.prototype.put;
IDBObjectStore.prototype.put = function(...args) {
  if (credentialArmed && mode === 'credential') throw new DOMException('Synthetic credential storage fault', 'QuotaExceededError');
  return put.apply(this, args);
};
const write = FileSystemSyncAccessHandle.prototype.write;
FileSystemSyncAccessHandle.prototype.write = function(...args) {
  if (armed && mode === 'crash') {
    const written = write.apply(this, args);
    send({id: saving, error: {code: 'outcome_unknown'}});
    self.close();
    throw new DOMException('Synthetic interrupted write', 'AbortError');
  }
  if (armed && mode === 'quota') throw new DOMException('Synthetic quota fault', 'QuotaExceededError');
  return write.apply(this, args);
};
self.postMessage = function(message, ...args) {
  if (message.id === saving && message.result?.saved && mode === 'lost') {
    send({id: message.id, error: {code: 'outcome_unknown'}}); return;
  }
  if (message.id === saving && message.result?.saved && mode === 'late') {
    send({testSaved: true}); setTimeout(() => send(message, ...args), 1000); return;
  }
  send(message, ...args);
};
await import('./sdk/core-sqlite-browser/worker.mjs');
for (const event of waiting) self.onmessage(event);
waiting.length = 0;
