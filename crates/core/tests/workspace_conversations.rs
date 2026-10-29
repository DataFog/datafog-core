use datafog_core::{
    TextRange,
    workspace::{
        Conversation, ConversationInput, ErrorCode, MarkerIssueKind, OperationContext,
        PreparedChange, RestoreEncoding, ReviewInput, SendIntentId, SendStatus, Workspace,
        WorkspaceFinding,
    },
};
fn id(value: u8) -> [u8; 16] {
    [value; 16]
}
fn operation(value: u8) -> OperationContext {
    OperationContext {
        id: id(value),
        occurred_at_ms: value as u64,
    }
}
fn context(value: u8, site: Option<&str>) -> Conversation {
    Conversation {
        id: id(value),
        surface: "synthetic_chat".into(),
        site_id: site.map(str::to_owned),
        site_id_byte_budget: 128,
    }
}
fn prepare(
    workspace: &Workspace,
    text: &str,
    mark: bool,
    conversation: Conversation,
    op: u8,
) -> PreparedChange {
    let findings = if mark {
        vec![WorkspaceFinding::new(
            "PERSON",
            text,
            TextRange {
                start: 0,
                end: text.len(),
            },
        )]
    } else {
        Vec::new()
    };
    workspace
        .prepare_conversation_send(
            &[ConversationInput {
                input: ReviewInput {
                    text,
                    scanner_findings: &[],
                    user_selections: &findings,
                    corrections: &[],
                },
                conversation,
            }],
            operation(op),
        )
        .unwrap()
}
fn commit(workspace: &mut Workspace, plan: &mut PreparedChange) {
    let bytes = plan.export_candidate_bundle().unwrap();
    let reopened = Workspace::open_bundle(bytes).unwrap();
    assert_eq!(reopened.head().unwrap(), plan.candidate_head().unwrap());
    workspace
        .activate_saved(
            plan,
            plan.expected_head().unwrap(),
            plan.candidate_head().unwrap(),
        )
        .unwrap();
}
#[test]
fn intention_is_not_issuance_and_one_time_binding_is_atomic() {
    let mut workspace = Workspace::create_with_operation(id(1), operation(2)).unwrap();
    let mut first = prepare(&workspace, "Synthetic Name", true, context(10, None), 3);
    let intent = first.send_intents().unwrap()[0];
    let marker = first.preview().unwrap().text.clone();
    assert_eq!(workspace.counts().unwrap().confirmed_handoffs, 0);
    assert_eq!(
        workspace.conversation(id(10)).err().unwrap().code,
        ErrorCode::ConversationUnknown
    );
    commit(&mut workspace, &mut first);
    assert_eq!(
        workspace.send_intent_status(intent).unwrap(),
        SendStatus::Pending
    );
    let mut noop = workspace.prepare("plain", &[], id(99)).unwrap();
    assert!(
        noop.send_intents().unwrap().is_empty(),
        "a generic no-op cannot claim the current head's intents"
    );
    noop.discard();
    for text in ["plain", marker.as_str()] {
        assert_eq!(
            workspace
                .restore_in_conversation(text, id(10))
                .err()
                .unwrap()
                .code,
            ErrorCode::ConversationPending
        );
    }
    assert_eq!(
        workspace
            .buffered_restore_in_conversation(id(10), RestoreEncoding::Utf8)
            .err()
            .unwrap()
            .code,
        ErrorCode::ConversationPending
    );
    assert_eq!(
        workspace
            .progressive_restore_in_conversation(id(10), RestoreEncoding::Utf8)
            .err()
            .unwrap()
            .code,
        ErrorCode::ConversationPending
    );
    assert_eq!(
        workspace.restore(&marker).unwrap(),
        "Synthetic Name",
        "explicit compatibility mode is unchanged"
    );
    let mut observed = workspace
        .prepare_observed_send(intent, id(10), "synthetic_chat", "thread_a", operation(4))
        .unwrap();
    assert_eq!(
        workspace.send_intent_status(intent).unwrap(),
        SendStatus::Pending
    );
    assert_eq!(
        workspace.conversation(id(10)).unwrap().conversation.site_id,
        None
    );
    observed.discard();
    assert_eq!(workspace.counts().unwrap().confirmed_handoffs, 0);
    let mut observed = workspace
        .prepare_observed_send(intent, id(10), "synthetic_chat", "thread_a", operation(5))
        .unwrap();
    commit(&mut workspace, &mut observed);
    assert_eq!(workspace.counts().unwrap().confirmed_handoffs, 1);
    assert_eq!(
        workspace.send_intent_status(intent).unwrap(),
        SendStatus::ObservedSent
    );
    assert_eq!(
        workspace
            .conversation_for_site("synthetic_chat", "thread_a")
            .unwrap(),
        id(10)
    );
    assert_eq!(
        workspace.restore_in_conversation(&marker, id(10)).unwrap(),
        "Synthetic Name"
    );
    assert_eq!(
        workspace
            .prepare_observed_send(intent, id(10), "synthetic_chat", "thread_a", operation(6))
            .err()
            .unwrap()
            .code,
        ErrorCode::InvalidSendIntent
    );
    let mut follow = prepare(&workspace, &marker, false, context(10, Some("thread_a")), 7);
    let next = follow.send_intents().unwrap()[0];
    commit(&mut workspace, &mut follow);
    assert_eq!(
        workspace
            .prepare_observed_send(next, id(10), "synthetic_chat", "thread_b", operation(8))
            .err()
            .unwrap()
            .code,
        ErrorCode::InvalidConversation
    );
    let mut cancel = workspace
        .prepare_definitely_not_sent(next, id(10), operation(9))
        .unwrap();
    commit(&mut workspace, &mut cancel);
    assert_eq!(workspace.counts().unwrap().confirmed_handoffs, 1);
    assert_eq!(
        workspace.restore_in_conversation(&marker, id(10)).unwrap(),
        "Synthetic Name"
    );
}
#[test]
fn same_chat_batch_pause_survives_reopen_recovery_and_partial_confirmation() {
    let mut workspace = Workspace::create_with_operation(id(1), operation(2)).unwrap();
    let finding =
        WorkspaceFinding::new("PERSON", "Synthetic Batch", TextRange { start: 0, end: 15 });
    let inputs: Vec<_> = (0..2)
        .map(|_| ConversationInput {
            input: ReviewInput {
                text: "Synthetic Batch",
                scanner_findings: &[],
                user_selections: std::slice::from_ref(&finding),
                corrections: &[],
            },
            conversation: context(10, None),
        })
        .collect();
    let mut plan = workspace
        .prepare_conversation_send(&inputs, operation(3))
        .unwrap();
    let intents = plan.send_intents().unwrap();
    let marker = plan.previews().unwrap()[0].text.clone();
    commit(&mut workspace, &mut plan);
    let mut one = workspace
        .prepare_observed_send(
            intents[0],
            id(10),
            "synthetic_chat",
            "thread_a",
            operation(4),
        )
        .unwrap();
    commit(&mut workspace, &mut one);
    assert_eq!(workspace.counts().unwrap().confirmed_handoffs, 1);
    workspace = Workspace::open_bundle(&workspace.export_bundle().unwrap()).unwrap();
    let mut recovery = workspace
        .prepare_recovery_with_operation(operation(5))
        .unwrap();
    commit(&mut workspace, &mut recovery);
    assert_eq!(
        workspace
            .restore_in_conversation("plain", id(10))
            .err()
            .unwrap()
            .code,
        ErrorCode::ConversationPending
    );
    assert_eq!(
        workspace.send_intent_status(intents[1]).unwrap(),
        SendStatus::Pending
    );
    let mut two = workspace
        .prepare_observed_send(
            intents[1],
            id(10),
            "synthetic_chat",
            "thread_a",
            operation(6),
        )
        .unwrap();
    commit(&mut workspace, &mut two);
    assert_eq!(workspace.counts().unwrap().confirmed_handoffs, 2);
    assert_eq!(
        workspace.restore_in_conversation(&marker, id(10)).unwrap(),
        "Synthetic Batch"
    );
    assert_eq!(workspace.conversation(id(10)).unwrap().issued_markers, 1);
}
#[test]
fn scoped_strict_and_streams_preserve_all_offset_and_failure_contracts() {
    let mut workspace = Workspace::create_with_operation(id(1), operation(2)).unwrap();
    let mut a = prepare(&workspace, "First Synthetic", true, context(10, None), 3);
    let aid = a.send_intents().unwrap()[0];
    let amarker = a.preview().unwrap().text.clone();
    commit(&mut workspace, &mut a);
    let mut a = workspace
        .prepare_observed_send(aid, id(10), "synthetic_chat", "thread_a", operation(4))
        .unwrap();
    commit(&mut workspace, &mut a);
    let mut b = prepare(&workspace, "Second Synthetic", true, context(11, None), 5);
    let bid = b.send_intents().unwrap()[0];
    let bmarker = b.preview().unwrap().text.clone();
    commit(&mut workspace, &mut b);
    assert_eq!(
        workspace.restore_in_conversation(&amarker, id(10)).unwrap(),
        "First Synthetic",
        "other pending chat stays isolated"
    );
    let mut b = workspace
        .prepare_observed_send(bid, id(11), "synthetic_chat", "thread_b", operation(6))
        .unwrap();
    commit(&mut workspace, &mut b);
    let response = format!("👋 {amarker} {bmarker}");
    let report = workspace
        .restore_report_in_conversation(&response, id(10))
        .unwrap();
    assert!(report.text.is_none());
    assert_eq!(
        report.restored_markers, 1,
        "only the marker issued into this context is resolvable"
    );
    assert_eq!(report.issues.len(), 1);
    assert_eq!(
        report.issues[0].kind,
        MarkerIssueKind::NotIssuedInConversation
    );
    let start = response.find(&bmarker).unwrap();
    assert_eq!(
        report.issues[0].byte_range,
        TextRange {
            start,
            end: start + bmarker.len()
        }
    );
    assert_eq!(
        report.issues[0].codepoint_range.start,
        response[..start].chars().count()
    );
    assert_eq!(
        report.issues[0].utf16_range.start,
        response[..start].encode_utf16().count()
    );
    let mut buffered = workspace
        .buffered_restore_in_conversation(id(10), RestoreEncoding::Utf8)
        .unwrap();
    buffered.feed(response.as_bytes()).unwrap();
    assert!(buffered.finalize().unwrap().text.is_none());
    let mut progressive = workspace
        .progressive_restore_in_conversation(id(10), RestoreEncoding::Utf8)
        .unwrap();
    let safe = progressive.feed("safe 👋 ".as_bytes()).unwrap();
    assert_eq!(safe.text, "safe 👋 ");
    let failure = progressive.feed(bmarker.as_bytes()).err().unwrap();
    assert!(!failure.complete);
    assert_eq!(failure.delivered, safe.delivered);
    assert_eq!(
        failure.error.marker_issues[0].kind,
        MarkerIssueKind::NotIssuedInConversation
    );
    assert_eq!(
        workspace
            .restore_in_conversation("plain", id(99))
            .err()
            .unwrap()
            .code,
        ErrorCode::ConversationUnknown
    );
}
#[test]
fn immutable_identity_and_bound_budget_reject_without_a_partial_plan() {
    let mut workspace = Workspace::create_with_operation(id(1), operation(2)).unwrap();
    let mut plan = prepare(&workspace, "ordinary", false, context(10, None), 3);
    let intent = plan.send_intents().unwrap()[0];
    commit(&mut workspace, &mut plan);
    let saved = workspace.export_bundle().unwrap();
    for (logical, surface, site) in [
        (id(11), "synthetic_chat", "thread_a"),
        (id(10), "wrong_surface", "thread_a"),
        (id(10), "synthetic_chat", ""),
    ] {
        assert!(
            workspace
                .prepare_observed_send(intent, logical, surface, site, operation(4))
                .is_err()
        );
        assert_eq!(workspace.export_bundle().unwrap(), saved);
    }
    assert_eq!(
        workspace
            .prepare_observed_send(
                intent,
                id(10),
                "synthetic_chat",
                &"a".repeat(129),
                operation(4)
            )
            .err()
            .unwrap()
            .code,
        ErrorCode::InvalidConversation
    );
    assert_eq!(
        workspace
            .send_intent_status(SendIntentId {
                preparation_id: id(99),
                input_index: 0
            })
            .err()
            .unwrap()
            .code,
        ErrorCode::InvalidSendIntent
    );
}
