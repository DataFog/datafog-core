"""Compile-only consumers of the actual installed candidate; no provider/storage runs."""
from __future__ import annotations
from typing import TYPE_CHECKING
from collections.abc import Awaitable, Callable, Iterable, Mapping, Sequence
from os import urandom
from time import time_ns
from datafog_core import (
    DataFogWorkspaceError, DataFogWorkspaceStreamError, Workspace,
    WorkspaceHead, WorkspaceInput, WorkspaceReviewInput,
)

if TYPE_CHECKING:
    # Actual installed annotations; no runtime imports or invented SDK exports.
    from datafog_core import (
        _WorkspaceOperation, _WorkspaceConversationInput, _WorkspaceSendIntent,  # pyright: ignore[reportPrivateUsage]
        _WorkspaceConversationState,  # pyright: ignore[reportPrivateUsage]
    )

# Trusted host callbacks must bind exact input/generation/approval and durable CAS.
SaveCandidate = Callable[[bytes, WorkspaceHead, WorkspaceHead], Awaitable[WorkspaceHead]]
Approve = Callable[[Sequence[Mapping[str, object]], str | None], Awaitable[bool]]


def create_candidate() -> Workspace:
    # Unsaved bootstrap: the host must persist its exported bundle before use.
    return Workspace.create_with_operation(
        urandom(16), {"id": urandom(16), "occurredAtMs": time_ns() // 1_000_000},
    )


async def current_review(
    workspace: Workspace, source: WorkspaceReviewInput, operation_id: bytes,
    save: SaveCandidate, approve: Approve,
) -> list[str] | None:
    if workspace.review(source)["conflicts"]:
        return None  # Edit/review; this is not an approved empty handoff.
    with workspace.prepare_review([source], operation_id) as plan:
        previews = plan.previews()
        version = plan.snapshot_format()
        warning = None
        if version["currentVersion"] < 3 and version["candidateVersion"] == 3:
            warning = "Saving v3 prevents 0.5.0-beta.1 and 0.5.0-beta.2 from opening this vault."
        elif version["currentVersion"] == 1 and version["candidateVersion"] == 2:
            warning = "Saving v2 prevents 0.5.0-beta.1 from opening this vault."
        if (await approve(previews, warning)) is not True:
            return None
        # Python returns immutable caller-owned bytes; reference release is not erasure.
        candidate = plan.export_candidate_bundle()
        saved = await save(candidate, plan.expected_head(), plan.candidate_head())
        workspace.activate_saved(plan, plan.expected_head(), saved)
        return [preview["text"] for preview in previews]
    return None


def recorded_outcome(workspace: Workspace) -> None:
    # Staged/discarded is intentionally not a committed outcome or successful send.
    with workspace.prepare_outcome(
        {"kind": "user_cancelled", "issueCount": 0},
        {"id": urandom(16), "occurredAtMs": time_ns() // 1_000_000},
    ) as plan:
        assert plan.previews() == []


def recorded_preparation(workspace: Workspace, inputs: list[WorkspaceInput]) -> None:
    # Supplied findings only; combined automatic recall uses current_review instead.
    with workspace.prepare_batch_with_operation(
        inputs, {"id": urandom(16), "occurredAtMs": time_ns() // 1_000_000},
    ) as plan:
        plan.previews()  # No implicit save or activation.


def visit_pages(workspace: Workspace, consume: Callable[[Mapping[str, object]], None]) -> None:
    # Host holds the same committed head throughout this read; do not retain all pages.
    offset = 0
    while True:
        page = workspace.operations_page(offset, 1000)
        consume(page)
        next_offset: int | None = page["nextOffset"]
        if next_offset is None:
            return
        offset = next_offset


def strict_response(workspace: Workspace, chunks: Iterable[bytes]) -> str | None:
    with workspace.buffered_restore() as response:
        for chunk in chunks:
            response.feed(chunk)  # None; no prefix leaves strict mode.
        return response.finalize()["text"]  # None is review-required, not empty success.
    return None


def progressive_response(workspace: Workspace, chunks: Iterable[bytes], release: Callable[[str], None]) -> int:
    response = workspace.progressive_restore()
    try:
        for chunk in chunks:
            release(response.feed(chunk)["text"])
        final = response.finalize()
        release(final["text"])
        return final["delivered"]["bytes"]
    finally:
        response.close()
    # Core/host/provider errors propagate as incomplete. Earlier output is irretractable.


def typed_failure(error: DataFogWorkspaceError) -> tuple[str, int | None, int | None]:
    prior_bytes: int | None = None
    if isinstance(error, DataFogWorkspaceStreamError):
        prior_bytes = error.delivered["bytes"]
        assert error.complete is False
    return error.code, error.input_index, prior_bytes  # No raw provider message.


def typed_reads(workspace: Workspace) -> tuple[int, bool, str]:
    rows = workspace.operations()
    time: int | None = rows[0]["occurredAtMs"] if rows else None
    count: int = workspace.counts()["operations"]
    coverage = workspace.history_coverage()
    del time
    return count, coverage["recordedHistoryOnly"], coverage["preUpgradeHistory"]


def rejected_types(workspace: Workspace) -> None:
    # Negative checks reject current unsupported types, not promises of future names.
    workspace.operations_page("0", 1)  # type: ignore[arg-type]
    workspace.operations_page(0, 1.5)  # type: ignore[arg-type]
    workspace.prepare_outcome({"kind": "handoff_confirmed", "issueCount": 0}, {"id": bytes(16), "occurredAtMs": 0})  # type: ignore[typeddict-item]
    workspace.prepare_with_operation("", [], {"id": bytes(16), "occurredAtMs": 0, "text": "forbidden"})  # type: ignore[typeddict-unknown-key]
    workspace.conversations()  # type: ignore[attr-defined]  # No plural enumeration API.


async def save_conversation_preparation(
    workspace: Workspace, inputs: list[_WorkspaceConversationInput], operation: _WorkspaceOperation,
    save: SaveCandidate, approve: Approve,
) -> tuple[list[str], list[_WorkspaceSendIntent]] | None:
    # Save pending intentions/mappings before transport; capture IDs before activation.
    with workspace.prepare_conversation_send(inputs, operation) as plan:
        intents = plan.send_intents()
        previews = plan.previews()
        version = plan.snapshot_format()
        warning = None
        if version["currentVersion"] < 3:
            warning = "Saving v3 prevents 0.5.0-beta.1 and 0.5.0-beta.2 from opening this vault."
        if (await approve(previews, warning)) is not True:
            return None
        saved = await save(plan.export_candidate_bundle(), plan.expected_head(), plan.candidate_head())
        workspace.activate_saved(plan, plan.expected_head(), saved)
        return [preview["text"] for preview in previews], intents
    return None


async def save_observed_conversation_send(
    workspace: Workspace, intent: _WorkspaceSendIntent, logical_id: bytes,
    surface: str, site_id: str, operation: _WorkspaceOperation,
    save: SaveCandidate, approve: Approve,
) -> bool:
    # Trusted observation of the exact saved send attempt is a precondition.
    # This example performs no send; approval alone is not an observation.
    with workspace.prepare_observed_send(intent, logical_id, surface, site_id, operation) as plan:
        if (await approve(plan.previews(), None)) is not True:
            return False
        saved = await save(plan.export_candidate_bundle(), plan.expected_head(), plan.candidate_head())
        workspace.activate_saved(plan, plan.expected_head(), saved)
        return True
    return False


def stage_definite_non_delivery(
    workspace: Workspace, intent: _WorkspaceSendIntent, logical_id: bytes, operation: _WorkspaceOperation,
) -> None:
    # Definite non-delivery only, never uncertainty. Discarding does not clear a pause.
    with workspace.prepare_definitely_not_sent(intent, logical_id, operation):
        pass


def scoped_conversation_types(
    workspace: Workspace, logical_id: bytes, surface: str, site_id: str,
    intent: _WorkspaceSendIntent, text: str,
) -> tuple[_WorkspaceConversationState, bytes, str, str]:
    # Call on a confirmed context. Pending contexts reject even plain text.
    state = workspace.conversation(logical_id)
    binding: bytes = workspace.conversation_for_site(surface, site_id)
    status = workspace.send_intent_status(intent)
    restored: str = workspace.restore_in_conversation(text, logical_id)
    report = workspace.restore_report_in_conversation(text, logical_id)
    assert isinstance(report["restoredMarkers"], int)
    with workspace.buffered_restore_in_conversation(logical_id, "utf8") as strict:
        strict.feed(b"")
    with workspace.progressive_restore_in_conversation(logical_id, "utf16") as progressive:
        progressive.feed_utf16([])
    return state, binding, status, restored
