use datafog_core::{
    TextRange,
    workspace::{
        Conversation, ConversationInput, OperationContext, PreparedChange, ReviewInput, Workspace,
        WorkspaceFinding,
    },
};
fn id(value: u8) -> [u8; 16] {
    [value; 16]
}
fn op(value: u8) -> OperationContext {
    OperationContext {
        id: id(value),
        occurred_at_ms: 0,
    }
}
fn definition() -> Conversation {
    Conversation {
        id: id(10),
        surface: "synthetic_chat".into(),
        site_id: None,
        site_id_byte_budget: 64,
    }
}
fn save(workspace: &mut Workspace, plan: &mut PreparedChange) {
    let bytes = plan.export_candidate_bundle().unwrap();
    let opened = Workspace::open_bundle(bytes).unwrap();
    assert_eq!(opened.export_bundle().unwrap(), bytes);
    workspace
        .activate_saved(
            plan,
            plan.expected_head().unwrap(),
            plan.candidate_head().unwrap(),
        )
        .unwrap();
}
#[test]
fn all_32_empty_member_intents_and_dense_resolutions_reopen_canonically() {
    let mut workspace = Workspace::create_with_operation(id(1), op(2)).unwrap();
    let inputs: Vec<_> = (0..32)
        .map(|_| ConversationInput {
            input: ReviewInput {
                text: "plain",
                scanner_findings: &[],
                user_selections: &[],
                corrections: &[],
            },
            conversation: definition(),
        })
        .collect();
    let mut first = workspace.prepare_conversation_send(&inputs, op(3)).unwrap();
    let intents = first.send_intents().unwrap();
    save(&mut workspace, &mut first);
    for (index, intent) in intents.into_iter().enumerate() {
        let mut confirmed = workspace
            .prepare_observed_send(
                intent,
                id(10),
                "synthetic_chat",
                "thread_a",
                op(4 + index as u8),
            )
            .unwrap();
        save(&mut workspace, &mut confirmed);
    }
    assert_eq!(workspace.counts().unwrap().confirmed_handoffs, 32);
    assert_eq!(workspace.conversation(id(10)).unwrap().issued_markers, 0);
}
#[test]
fn dense_short_label_memberships_reopen_after_every_partial_confirmation() {
    let mut workspace = Workspace::create_with_operation(id(20), op(21)).unwrap();
    let texts: Vec<_> = (0..32).map(|index| format!("Name {index}")).collect();
    let findings: Vec<_> = texts
        .iter()
        .map(|text| {
            WorkspaceFinding::new(
                "A",
                text.as_str(),
                TextRange {
                    start: 0,
                    end: text.len(),
                },
            )
        })
        .collect();
    let inputs: Vec<_> = texts
        .iter()
        .zip(&findings)
        .map(|(text, finding)| ConversationInput {
            input: ReviewInput {
                text,
                scanner_findings: &[],
                user_selections: std::slice::from_ref(finding),
                corrections: &[],
            },
            conversation: definition(),
        })
        .collect();
    let mut first = workspace
        .prepare_conversation_send(&inputs, op(22))
        .unwrap();
    let intents = first.send_intents().unwrap();
    save(&mut workspace, &mut first);
    for (index, intent) in intents.into_iter().enumerate() {
        let mut confirmed = workspace
            .prepare_observed_send(
                intent,
                id(10),
                "synthetic_chat",
                "thread_b",
                op(40 + index as u8),
            )
            .unwrap();
        save(&mut workspace, &mut confirmed);
    }
    assert_eq!(workspace.conversation(id(10)).unwrap().issued_markers, 32);
    assert_eq!(workspace.counts().unwrap().confirmed_handoffs, 32);
}
