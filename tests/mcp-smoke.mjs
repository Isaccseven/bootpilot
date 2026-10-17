import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { createInterface } from 'node:readline';
import { once } from 'node:events';
import path from 'node:path';
import { writeFileSync } from 'node:fs';

// Deliberately build a separate, explicitly gated fixture server. Never replace
// the normal boot-mcp binary and never invoke a Windows broker in this suite.
const live = process.argv.includes('--live');
const refreshOutput = process.argv.includes('--refresh-output') ? process.argv[process.argv.indexOf('--refresh-output') + 1] : null;
assert.ok(!refreshOutput || live, 'a real refresh is available only with explicit --live');
if (!live) {
  const build = spawnSync('cargo', ['build', '--locked', '-p', 'boot-mcp', '--features', 'test-fixture', '--bin', 'boot-mcp-fixture'], { stdio: 'inherit' });
  assert.equal(build.status, 0, 'fixture server must build');
}
const binary = path.resolve(process.env.CARGO_TARGET_DIR || 'target', 'debug', `${live ? 'boot-mcp' : 'boot-mcp-fixture'}${process.platform === 'win32' ? '.exe' : ''}`);
const child = spawn(binary, [], { stdio: ['pipe', 'pipe', 'pipe'] });
const closed = once(child, 'close');
let stderr = '';
child.stderr.on('data', bytes => { stderr += bytes; });
child.on('error', error => { throw error; });
const pending = new Map();
const unexpected = [];
const lines = createInterface({ input: child.stdout });
lines.on('line', line => {
  try {
    const value = JSON.parse(line);
    assert.equal(value.jsonrpc, '2.0', 'stdout_contains_only_jsonrpc');
    if (pending.has(value.id)) { pending.get(value.id)(value); pending.delete(value.id); }
    else if (!value.method) unexpected.push(value);
  } catch (error) { unexpected.push({ invalid_stdout: line, message: error.message }); }
});
let id = 0;
function send(method, params = {}, timeoutMs = 15000) {
  const requestId = ++id;
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => { pending.delete(requestId); reject(new Error(`${method} timed out; stderr=${stderr}`)); }, timeoutMs);
    pending.set(requestId, response => { clearTimeout(timer); resolve(response); });
    child.stdin.write(JSON.stringify({ jsonrpc: '2.0', id: requestId, method, params }) + '\n');
  });
}
function notify(method) { child.stdin.write(JSON.stringify({ jsonrpc: '2.0', method }) + '\n'); }
try {
  const initialize = await send('initialize', { protocolVersion: '2025-03-26', capabilities: {}, clientInfo: { name: 'bootpilot-smoke', version: '1' } });
  assert.ok(initialize.result.serverInfo);
  notify('notifications/initialized');
  const listed = await send('tools/list');
  const tools = listed.result.tools;
  const expected = ['boot_get_status', 'boot_refresh', 'boot_diagnose', 'boot_plan_order', 'boot_plan_menu', 'boot_plan_cachyos', 'boot_apply_plan', 'boot_list_backups', 'boot_plan_restore'];
  assert.deepEqual(tools.map(tool => tool.name).sort(), expected.sort(), 'initialize_lists_expected_tools');
  const call = (name, args = {}) => send('tools/call', { name, arguments: args });
  const status = await call('boot_get_status');
  assert.equal(status.result.isError ?? false, false);
  assert.equal(status.result.structuredContent.schema_version, 1);
  assert.ok('captured_at' in status.result.structuredContent, 'status_has_freshness_and_completeness');
  assert.ok('is_stale' in status.result.structuredContent);
  if (live) {
    console.log('PASS actual initialize, tools/list (9), boot_get_status; no elevation requested');
    if (refreshOutput) {
      const refreshed = await send('tools/call', { name: 'boot_refresh', arguments: {} }, 90000);
      const after = await call('boot_get_status');
      writeFileSync(refreshOutput, JSON.stringify({ initialize, listed, status, refreshed, after }, null, 2));
      assert.equal(refreshed.result?.isError, false, 'actual read-only refresh must succeed');
      const snapshot = refreshed.result.structuredContent.data;
      assert.equal(after.result.structuredContent.snapshot.id, snapshot.id);
      console.log('PASS actual read-only boot_refresh, persisted boot_get_status; sources=' + JSON.stringify(snapshot.sources) + '; firmware=' + snapshot.firmware_entries.length + '; linux=' + snapshot.linux_candidates.length);
    }
  } else {
    assert.equal(status.result.structuredContent.snapshot, null, 'no cache is null');
    assert.match(status.result.content[0].text, /Noch keine Erfassung/);
    const refreshed = await call('boot_refresh');
    assert.equal(refreshed.result.isError ?? false, false);
    assert.ok(refreshed.result.structuredContent.data.sources);
    const after = await call('boot_get_status');
    assert.ok(after.result.structuredContent.snapshot.sources);
    assert.ok(after.result.structuredContent.age_seconds >= 0);
    assert.equal(after.result.structuredContent.is_stale, true, 'old fixture capture is explicitly stale');
    const diagnosis = await call('boot_diagnose');
    assert.equal(diagnosis.result.isError, false);
    assert.ok(diagnosis.result.structuredContent.data.some(finding => finding.code === 'source_error'));
    const backups = await call('boot_list_backups');
    assert.equal(backups.result.isError, false);
    assert.deepEqual(backups.result.structuredContent.data, []);
    for (const [name, args] of [
      ['boot_plan_order', { entry_ids: ['00000000-0000-0000-0000-000000000001'] }],
      ['boot_plan_menu', { default_entry_id: 'test', timeout_seconds: 5 }],
      ['boot_plan_cachyos', { candidate_id: '00000000-0000-0000-0000-000000000001', title: 'Test' }],
    ]) {
      const plan = await call(name, args);
      assert.equal(plan.result.isError, true);
      assert.equal(plan.result.structuredContent.error.code, 'incomplete_snapshot');
    }
    const missing = await call('boot_apply_plan');
    assert.ok(missing.error || missing.result.isError, 'missing_plan_id_is_invalid');
    const malformed = await call('boot_apply_plan', { plan_id: 'not-a-uuid' });
    assert.ok(malformed.error || malformed.result.isError);
    const denied = await call('boot_apply_plan', { plan_id: '00000000-0000-0000-0000-000000000001' });
    assert.equal(denied.result.isError, true, 'errors_are_tool_errors_not_success_text');
    assert.equal(denied.result.structuredContent.error.code, 'access_denied');
    const restore = await call('boot_plan_restore', { backup_id: '00000000-0000-0000-0000-000000000002' });
    assert.equal(restore.result.isError, true);
    assert.equal(restore.result.structuredContent.error.code, 'invalid_backup', 'restore routed through prepare');
    assert.equal(tools.find(tool => tool.name === 'boot_apply_plan').annotations.destructiveHint, true);
    for (const name of ['boot_get_status', 'boot_diagnose', 'boot_list_backups']) assert.equal(tools.find(tool => tool.name === name).annotations.readOnlyHint, true);
    console.log('PASS initialize_lists_expected_tools, missing_plan_id_is_invalid, status_has_freshness_and_completeness, stdout_contains_only_jsonrpc, errors_are_tool_errors_not_success_text');
  }
} finally {
  child.stdin.end();
  const timer = setTimeout(() => child.kill(), 3000);
  const [code] = await closed;
  clearTimeout(timer);
  assert.equal(code, 0, 'server exits cleanly after stdin closes');
  assert.deepEqual(unexpected, [], 'stdout_contains_only_jsonrpc');
}
