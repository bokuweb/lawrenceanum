import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFileSync, spawnSync } from 'node:child_process';

const original = readFileSync(new URL('./build-ndlocr-wasm.sh', import.meta.url), 'utf8');
const git = execFileSync('which', ['git'], { encoding: 'utf8' }).trim();
function fixture() {
  const root = mkdtempSync(join(tmpdir(), 'kanpo-source-checkout-'));
  const remote = join(root, 'source');
  mkdirSync(join(remote, 'tools'), { recursive: true });
  writeFileSync(join(remote, 'tools/build_web.sh'), 'printf "builder\\n" >> "$KANPO_TEST_RECEIPT"\n');
  const run = (...args) => execFileSync(git, ['-C', remote, ...args], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
  run('init', '--quiet');
  run('config', 'uploadpack.allowFilter', 'true');
  run('add', '.');
  run('-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', 'commit', '--quiet', '-m', 'pinned');
  const pin = run('rev-parse', 'HEAD');
  writeFileSync(join(remote, 'later.txt'), 'a moving branch must not replace the pin');
  run('add', '.'); run('-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', 'commit', '--quiet', '-m', 'later');
  const scripts = join(root, 'figma/scripts'); mkdirSync(scripts, { recursive: true });
  const script = join(scripts, 'build-ndlocr-wasm.sh');
  writeFileSync(script, original.replace(/revision="[a-f0-9]+"/, `revision="${pin}"`)
    .replace('source_url="https://github.com/bokuweb/ndlocr-lite-rs.git"', `source_url="${remote}"`));
  const bin = join(root, 'bin'); mkdirSync(bin);
  const receipt = join(root, 'receipt');
  for (const command of ['npm', 'rustup']) {
    writeFileSync(join(bin, command), `#!/bin/sh\nprintf '${command}\\n' >> "$KANPO_TEST_RECEIPT"\n`, { mode: 0o755 });
  }
  writeFileSync(join(bin, 'git'), [
    "#!/bin/bash",
    "set -eu",
    "args=(\"$@\")",
    "for arg in \"$@\"; do",
    "  if [ \"$arg\" = fetch ]; then",
    "    echo fetch >> \"$KANPO_TEST_FETCHES\"",
    "    if [ \"${KANPO_TEST_WRONG_PIN:-0}\" = 1 ]; then",
    "      args[${#args[@]}-1]=HEAD",
    "    elif [ \"${KANPO_TEST_FALLBACK:-0}\" = 1 ]; then",
    "      for option in \"$@\"; do",
    "        if [ \"$option\" = protocol.version=2 ]; then exit 128; fi",
    "      done",
    "    fi",
    "  fi",
    "done",
    "exec \"$KANPO_TEST_GIT\" \"${args[@]}\"",
  ].join("\n"), { mode: 0o755 });
  const cache = join(root, `target/ndlocr-web-src-${pin}`);
  return { root, remote, pin, script, receipt, cache,
    env: { ...process.env, PATH: `${bin}:${process.env.PATH}`, NDLOCR_FETCH_MODELS: '0', NDLOCR_SOURCE: '',
      KANPO_TEST_GIT: git, KANPO_TEST_RECEIPT: receipt, KANPO_TEST_FETCHES: join(root, 'fetches') } };
}
function build(f, env = {}) {
  return spawnSync('bash', [f.script], { env: { ...f.env, ...env }, encoding: 'utf8' });
}

test('a stale cached origin and protocol failure still resolve the exact pin', () => {
  const f = fixture();
  try {
    mkdirSync(f.cache, { recursive: true });
    execFileSync(git, ['-C', f.cache, 'init', '--quiet']);
    execFileSync(git, ['-C', f.cache, 'remote', 'add', 'origin', join(f.root, 'wrong-remote')]);
    const result = build(f, { KANPO_TEST_FALLBACK: '1' });
    assert.equal(result.status, 0, result.stderr);
    assert.equal(execFileSync(git, ['-C', f.cache, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim(), f.pin);
    assert.equal(execFileSync(git, ['-C', f.cache, 'remote', 'get-url', 'origin'], { encoding: 'utf8' }).trim(), f.remote);
    assert.equal(readFileSync(f.env.KANPO_TEST_FETCHES, 'utf8').trim().split('\n').length, 2);
    assert.match(readFileSync(f.receipt, 'utf8'), /builder/);
  } finally { rmSync(f.root, { recursive: true, force: true }); }
});

test('a successful fetch of a different commit is rejected before building', () => {
  const f = fixture();
  try {
    const result = build(f, { KANPO_TEST_WRONG_PIN: '1' });
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /did not resolve to the pinned commit/);
    assert.equal(existsSync(f.receipt), false);
  } finally { rmSync(f.root, { recursive: true, force: true }); }
});

test('an explicit local source remains independent of cache checkout policy', () => {
  const f = fixture();
  try {
    const result = build(f, { NDLOCR_SOURCE: f.remote });
    assert.equal(result.status, 0, result.stderr);
    assert.equal(existsSync(f.env.KANPO_TEST_FETCHES), false);
    assert.match(readFileSync(f.receipt, 'utf8'), /builder/);
  } finally { rmSync(f.root, { recursive: true, force: true }); }
});


test('inherited Git directory variables do not redirect the source checkout', () => {
  const f = fixture();
  try {
    execFileSync(git, ['-C', f.root, 'init', '--quiet']);
    const originalRemote = join(f.root, 'unrelated-outer-origin');
    execFileSync(git, ['-C', f.root, 'remote', 'add', 'origin', originalRemote]);
    const result = build(f, { GIT_DIR: join(f.root, '.git'), GIT_WORK_TREE: f.root, GIT_COMMON_DIR: join(f.root, '.git') });
    assert.equal(result.status, 0, result.stderr);
    assert.equal(execFileSync(git, ['-C', f.cache, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim(), f.pin);
    assert.equal(execFileSync(git, ['-C', f.root, 'remote', 'get-url', 'origin'], { encoding: 'utf8' }).trim(), originalRemote);
  } finally { rmSync(f.root, { recursive: true, force: true }); }
});
