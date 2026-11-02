//! Forgetting committed values so a long-lived workspace stays small.
use super::*;

impl Workspace {
    /// Prepare forgetting committed values, named by their marker codes.
    ///
    /// The host decides what to forget, for example values not restored within a
    /// time-to-live; Core keeps no clock. Each code must name a committed entry,
    /// at most once. The entry's value is dropped and its code is retired: Core
    /// never issues it again, restoration reports the marker as `Expired`, and
    /// conversation references to it stay valid. A value seen again later gets a
    /// new marker. Saving the result writes snapshot version 4.
    pub fn prepare_forget(&self, codes: &[String], operation: PublicId) -> Result<PreparedChange> {
        let state = self.state()?;
        ensure_fresh_operation(state, operation)?;
        if codes.len() > MAX_ENTRIES {
            return Err(error(ErrorCode::ResourceLimit));
        }
        let requested: BTreeSet<&str> = codes.iter().map(String::as_str).collect();
        if requested.is_empty() || requested.len() != codes.len() {
            return Err(error(ErrorCode::InvalidMarker));
        }
        let forgotten: Vec<Identity> = state
            .entries
            .iter()
            .filter(|(_, entry)| requested.contains(entry.code.as_str()))
            .map(|(identity, _)| identity.clone())
            .collect();
        if forgotten.len() != requested.len() {
            return Err(error(ErrorCode::InvalidMarker));
        }
        let mut candidate = state.clone();
        for identity in forgotten {
            let entry = candidate
                .entries
                .remove(&identity)
                .ok_or_else(|| error(ErrorCode::InvalidMarker))?;
            candidate.retired.insert(
                entry.code.clone(),
                Retired {
                    label: identity.entity_type.clone(),
                    issuance: entry.issuance,
                    counter: entry.counter,
                    operation,
                },
            );
        }
        candidate.head = Head {
            revision: state
                .head
                .revision
                .checked_add(1)
                .ok_or_else(|| error(ErrorCode::ResourceLimit))?,
            commit_id: operation,
        };
        candidate.record_operation(OperationRecord {
            id: operation,
            kind: OperationKind::Forget,
            occurred_at_ms: None,
            prepared_inputs: 0,
            transformed_spans: 0,
        })?;
        candidate.snapshot_version = 4;
        let bundle = Zeroizing::new(codec::encode_for_record(&candidate, false)?);
        Ok(PreparedChange {
            pending: Some(Pending {
                lifetime: Arc::clone(&self.lifetime),
                parent: state.head,
                parent_snapshot_version: state.snapshot_version,
                candidate,
                bundle,
                previews: Vec::new(),
                policy_preview: None,
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn activate(workspace: &mut Workspace, plan: &mut PreparedChange) {
        let parent = plan.expected_head().unwrap();
        let saved = plan.candidate_head().unwrap();
        workspace.activate_saved(plan, parent, saved).unwrap();
    }

    #[test]
    fn issuance_redraws_when_the_first_candidate_is_a_retired_code() {
        let mut workspace = Workspace::create([1; 16], [2; 16]).unwrap();
        let finding = WorkspaceFinding::new("PERSON", "Jane", TextRange { start: 0, end: 4 });
        let mut plan = workspace
            .prepare("Jane", std::slice::from_ref(&finding), [3; 16])
            .unwrap();
        let marker = plan.preview().unwrap().text.clone();
        let code = marker
            .trim_end_matches("]]")
            .rsplit_once('_')
            .unwrap()
            .1
            .to_owned();
        activate(&mut workspace, &mut plan);
        let mut plan = workspace
            .prepare_forget(std::slice::from_ref(&code), [4; 16])
            .unwrap();
        activate(&mut workspace, &mut plan);

        // A valid synthetic snapshot reserves exactly the next issuance's first
        // draw. This forces a collision instead of relying on random chance.
        let collision = issue_code([1; 16], [5; 16], "PERSON", 2, &mut BTreeSet::new()).unwrap();
        let mut state = workspace.state().unwrap().clone();
        let retired = state.retired.remove(&code).unwrap();
        state.retired.insert(collision.clone(), retired);
        let bundle = codec::encode(&state).unwrap();
        let mut reopened = Workspace::open_bundle(&bundle).unwrap();
        let mut plan = reopened.prepare("Jane", &[finding], [5; 16]).unwrap();
        let new_marker = plan.preview().unwrap().text.clone();
        assert_ne!(new_marker, super::super::marker("PERSON", &collision));
        activate(&mut reopened, &mut plan);
        assert_eq!(reopened.restore(&new_marker).unwrap(), "Jane");
        assert_eq!(
            reopened
                .restore_report(&super::super::marker("PERSON", &collision))
                .unwrap()
                .issues[0]
                .kind,
            MarkerIssueKind::Expired
        );
    }
}
