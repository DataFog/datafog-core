import {cpSync, existsSync, mkdirSync, readFileSync, realpathSync, writeFileSync} from 'node:fs';
import path from 'node:path';
import {createHash} from 'node:crypto';

const version = '1.1.0-rc.1';
if (process.argv.length !== 3) throw Error('Usage: node copy-browser-rc-assets.mjs <built-unpacked-extension-directory>');
const output = path.resolve(process.argv[2]);
const manifest = JSON.parse(readFileSync(path.join(output, 'manifest.json'), 'utf8'));
if (manifest.manifest_version !== 3) throw Error('Expected an already built Manifest V3 extension');
const packages = {
  wasm: {version, files: ['index.js', 'dist/datafog_wasm.js', 'dist/datafog_wasm_bg.wasm', 'cnn/index.js', 'cnn/worker.js', 'cnn/workspace.mjs']},
  'core-sqlite-browser': {version, files: ['index.mjs', 'binding.mjs', 'vault.mjs', 'credentials.mjs', 'worker.mjs', 'vendor/sqlite3.mjs', 'vendor/sqlite3.wasm']},
  'core-fogvault-browser': {version, files: ['index.mjs', 'binding.mjs']},
  'fogvault-browser': {version: '0.1.0', files: ['dist/client.mjs']},
};
const sources = Object.entries(packages).map(([name, expected]) => {
  const source = realpathSync(path.join(process.cwd(), 'node_modules', '@datafog', name));
  const metadata = JSON.parse(readFileSync(path.join(source, 'package.json'), 'utf8'));
  if (metadata.name !== `@datafog/${name}` || metadata.version !== expected.version) throw Error(`Install exact @datafog/${name}@${expected.version} first`);
  for (const file of expected.files) if (!existsSync(path.join(source, file))) throw Error(`Missing published asset: @datafog/${name}/${file}`);
  return {name, source, expected};
});
const receipt = {version, packages: {}};
mkdirSync(path.join(output, 'datafog'), {recursive: true});
for (const {name, source, expected} of sources) {
  const target = path.join(output, 'datafog', name);
  cpSync(source, target, {recursive: true, force: true});
  const files = Object.fromEntries(expected.files.map(file => {
    const bytes = readFileSync(path.join(target, file));
    return [file, {bytes: bytes.length, sha256: createHash('sha256').update(bytes).digest('hex')}];
  }));
  receipt.packages[`@datafog/${name}`] = {version: expected.version, files};
}
writeFileSync(path.join(output, 'datafog', 'assets.json'), JSON.stringify(receipt, null, 2) + '\n');
console.log(`Copied exact RC packages into ${path.join(output, 'datafog')}`);
