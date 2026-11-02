/** Installed JavaScript SDK retirement lifecycle; synthetic values only. */
export function verifyWorkspaceForget({Workspace, DataFogError}) {
  const check = (value, message) => { if (!value) throw Error(message); };
  const id = value => new Uint8Array(16).fill(value);
  const code = marker => marker.slice(2, -2).split('_').at(-1);
  const reject = (run, expected) => {
    try { run(); } catch (error) {
      check(error instanceof DataFogError && error.code === expected && error.message === expected, `expected ${expected}`);
      return error;
    }
    throw Error(`missing ${expected} rejection`);
  };
  const activate = (workspace, plan) => {
    const {expectedHead, candidateHead} = plan.heads();
    const bytes = plan.exportCandidateBundle();
    const candidate = Workspace.openBundle(bytes);
    candidate.close(); bytes.fill(0);
    workspace.activateSaved(plan, expectedHead, candidateHead);
  };
  const prepare = (workspace, operation) => workspace.prepare('Jane Roe', [{entityType: 'PERSON', matchedText: 'Jane Roe',
    range: {unit: 'utf8_bytes', start: 0, end: 8}}], id(operation));
  let workspace = Workspace.create(id(1), id(2));
  try {
    const seed = prepare(workspace, 3), marker = seed.preview().text, retiredCode = code(marker);
    activate(workspace, seed);
    const before = workspace.exportBundle();
    const accessor = [];
    Object.defineProperty(accessor, '0', {get() { throw Error('code getter must not run'); }});
    for (const invalid of [null, {}, 'ABC123', [null], [1], ['bad'], [marker], Array(1), accessor,
      [], [retiredCode, retiredCode], ['ZZZZZZ'], [retiredCode, 'ZZZZZZ']]) {
      reject(() => workspace.prepareForget(invalid, id(4)), 'invalid_marker');
    }
    reject(() => workspace.prepareForget(Array(20001).fill(retiredCode), id(4)), 'resource_limit');
    reject(() => workspace.prepareForget([retiredCode], id(3)), 'invalid_operation_id');
    const after = workspace.exportBundle();
    check(before.length === after.length && before.every((byte, index) => after[index] === byte), 'rejected forget mutated workspace');
    after.fill(0);
    before.fill(0);
    const discarded = workspace.prepareForget([retiredCode], id(4));
    check(discarded.previews().length === 0 && discarded.policyPreview() === null, 'forget exposed a value preview');
    discarded.discard();
    check(workspace.restore(marker) === 'Jane Roe', 'discard forgot a value');
    const buffered = workspace.bufferedRestore(), progressive = workspace.progressiveRestore();
    buffered.feed(new TextEncoder().encode(marker));
    progressive.feed(new TextEncoder().encode(marker.slice(0, -2)));
    const stale = prepare(workspace, 5);
    const plan = workspace.prepareForget(Object.freeze([retiredCode]), id(6));
    check(JSON.stringify(plan.snapshotFormat()) === JSON.stringify({currentVersion: 3, candidateVersion: 4}), 'missing v4 upgrade');
    check(workspace.restore(marker) === 'Jane Roe', 'preparation forgot before activation');
    activate(workspace, plan);
    reject(() => stale.exportCandidateBundle(), 'inactive_plan');
    reject(() => buffered.finalize(), 'workspace_conflict');
    reject(() => progressive.feed(new TextEncoder().encode(']]')), 'workspace_conflict');
    buffered.close(); progressive.close();
    reject(() => workspace.restore(marker), 'invalid_marker');
    reject(() => workspace.prepareForget([retiredCode], id(7)), 'invalid_marker');
    reject(() => workspace.prepare(marker, [], id(7)), 'invalid_marker');
    check(workspace.recall('Jane Roe').findings.length === 0, 'forgotten value recalled');
    const bundle = workspace.exportBundle();
    workspace.close(); workspace = Workspace.openBundle(bundle); bundle.fill(0);
    const text = `👋 ${marker}`, report = workspace.restoreReport(text);
    check(report.text === null && report.issues.length === 1 && report.issues[0].kind === 'expired', 'retirement lost on reopen');
    const issue = report.issues[0];
    check(issue.byteRange.start === 5 && issue.codepointRange.start === 2 && issue.utf16Range.start === 3 &&
      text.slice(issue.utf16Range.start, issue.utf16Range.end) === marker, 'expired ranges changed');
    const stream = workspace.progressiveRestore();
    const failure = reject(() => stream.feed(new TextEncoder().encode(marker)), 'invalid_marker');
    check(failure.markerIssues[0].kind === 'expired', 'progressive restore lost expired category');
    stream.close();
    check(workspace.operations().at(-1).kind === 'forget' && workspace.policy().snapshotVersion === 4, 'forget metadata missing');
    const again = prepare(workspace, 8), nextMarker = again.preview().text;
    check(code(nextMarker) !== retiredCode, 'retired code reissued');
    activate(workspace, again);
    check(workspace.restore(nextMarker) === 'Jane Roe' && workspace.restoreReport(marker).issues[0].kind === 'expired', 'new issuance revived old marker');
  } finally { workspace.close(); }
}
