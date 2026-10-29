/** Real MV3/IndexedDB/crypto; controlled synthetic transport, no provider calls. */
export async function verifyConversationHost({launch,close,check,password}) {
  let page=await launch("conversation-source");
  const checkpoint=await page.evaluate(async password=>(await import("./conversation-proof.js")).createPendingFixture(password),password);
  check(checkpoint.transportCalls===1,"one explicit synthetic handoff before failed confirmation save");
  await close();
  const imports=[];
  for(const method of ["password","recovery"]) {
    const profile=`conversation-backup-${method}`,credential=method==="password"?password:checkpoint.recoveryCode;
    page=await launch(profile);
    const imported=await page.evaluate(async({credential,method,checkpoint})=>(await import("./conversation-proof.js")).importPendingBackup(credential,method,checkpoint),{credential,method,checkpoint});
    check(imported.profile===2 && imported.preserved,"strong backup preserves conversation pending metadata");
    await close();page=await launch(profile);
    const reopened=await page.evaluate(async({credential,method,checkpoint})=>(await import("./conversation-proof.js")).inspectPendingFixture(credential,method,checkpoint),{credential,method,checkpoint});
    check(reopened.pending && reopened.profile===2 && reopened.counts.confirmedHandoffs===0,"fresh process backup reopen retains pause");
    imports.push({method,profile:reopened.profile,pending:true});await close();
  }
  page=await launch("conversation-source");
  const pending=await page.evaluate(async({password,checkpoint})=>(await import("./conversation-proof.js")).inspectPendingFixture(password,"password",checkpoint),{password,checkpoint});
  check(pending.pending && pending.counts.confirmedHandoffs===0,"original process restart retains unresolved send");
  const second=await page.evaluate(async({password,checkpoint})=>(await import("./conversation-proof.js")).retryThenLoseAck(password,checkpoint),{password,checkpoint});
  check(second.transportCalls===2,"confirmation retry does not resend");await close();
  page=await launch("conversation-source");
  const final=await page.evaluate(async({password,checkpoint})=>(await import("./conversation-proof.js")).verifyCommittedAndIsolation(password,checkpoint),{password,checkpoint:second});
  check(final.transportCalls===3 && final.counts.confirmedHandoffs===3,"saved confirmations count exact synthetic handoffs only");
  await close();checkpoint.backup.fill(0);
  page=await launch("conversation-installed-contract");
  const contract=await page.evaluate(async()=>{
    const api=await import("./vendor/package/index.js"),{verifyWorkspaceConversations}=await import("./workspace-conversation-conformance.mjs");
    return verifyWorkspaceConversations(api);
  });await close();
  const graphs=[];
  for(const profile of ["near-ordinary","hard"]) {
    page=await launch(`conversation-graph-${profile}`);
    const graph=await page.evaluate(async profile=>{
      const api=await import("./vendor/package/index.js");await api.init();
      const wasm=await(await import("./vendor/package/dist/datafog_wasm.js")).default();
      const bytes=new Uint8Array(await fetch(`./conversation-fixtures/${profile}.cbor`).then(response=>response.arrayBuffer()));
      const started=performance.now(),workspace=api.Workspace.openBundle(bytes);bytes.fill(0);
      const openMs=performance.now()-started,counts=workspace.counts(),contexts=profile==="hard"?49871:49870;
      const id=workspace.conversationForSite("synthetic_chat",`thread_${contexts-1}`),definition=workspace.conversation(id).conversation;
      const before=wasm.memory.buffer.byteLength,plans=[],preparing=performance.now();
      try {
        if(profile==="near-ordinary")for(let index=0;index<32;index++)plans.push(workspace.prepareConversationSend([{input:{text:"plain",scannerFindings:[],userSelections:[]},conversation:definition}],{id:crypto.getRandomValues(new Uint8Array(16)),occurredAtMs:0}));
        const prepareMs=performance.now()-preparing,retained=wasm.memory.buffer.byteLength;
        for(const plan of plans)plan.discard();
        if(workspace.counts().operations!==counts.operations || workspace.restoreInConversation("plain",id)!=="plain")throw new Error("graph mutation or scoped lookup mismatch");
        return {profile,contexts,rows:counts.operations,confirmedHandoffs:counts.confirmedHandoffs,pendingPlans:plans.length,openMs,prepareMs,wasmLinearBeforePlans:before,wasmLinearWithPlans:retained,wasmLinearAfterDiscard:wasm.memory.buffer.byteLength,
          note:"Reserved WASM linear memory does not shrink after discard; native cleanup is measured separately. No browser RSS/low-hardware SLA is claimed."};
      } finally {for(const plan of plans){try{plan.discard();}catch(error){if(error.code!=="inactive_plan")throw error;}}workspace.close();}
    },profile);
    check(graph.rows===(profile==="hard"?100000:99742) && graph.confirmedHandoffs===graph.contexts,"many distinct conversation graph import/counts");
    check(graph.pendingPlans===(profile==="hard"?0:32),"real installed plan ownership capacity");graphs.push(graph);await close();
  }
  return {scope:"actual synthetic MV3 persistence, controlled transport and Core-backed large graphs; no real site observation verification",imports,contract,graphs,final};
}
