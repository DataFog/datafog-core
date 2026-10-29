"""Installed-SDK qualification of explicit text slots, never provider translation."""
from __future__ import annotations
import copy
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
import json
import os
from pathlib import Path
import tempfile
from threading import Barrier
import unittest

from datafog_core import Workspace, WorkspaceFinding, TextRange, WorkspaceHead, DataFogWorkspaceError
from host import EncryptedStore, Scope, RequestContext, HostError, HostStreamFailure, prepare_and_handoff, restore_response, restore_text_chunks

VALUE = "Synthetic Shared Person"
UNKNOWN = "[[PERSON_AAAAAA]]"
_OMITTED = object()
OPAQUE_ARGUMENTS = '{\n  "literal": "[[PERSON_AAAAAA]]", "name": "Opaque Tool Person", "emoji": "👋"\n}'


def identifier(value):
    return bytes([value]) * 16


def envelope(text, *, outbound=False):
    opaque = {"id": "untrusted_provider_id", "metadata": {"tenant": "forged", "workspace": "forged", "requestId": "forged"},
        "tools": [{"name": "opaque_tool", "input_schema": {"type": "object", "properties": {"literal": {"const": UNKNOWN}}}}],
        "rawToolArguments": OPAQUE_ARGUMENTS,
        "content": [{"type": "text", "text": text}, {"type": "tool_use", "id": UNKNOWN, "name": "opaque_tool", "input": {"name": "Opaque Tool Person", "literal": UNKNOWN}}]}
    if outbound:
        opaque["content"][1] = {"type": "tool_result", "tool_use_id": UNKNOWN, "content": "Opaque Tool Person"}
    return opaque


def replaced(body, text):
    result = copy.deepcopy(body)
    result["content"][0]["text"] = text
    return result


def opaque_bytes(body):
    return json.dumps(replaced(body, "<explicit text slot>"), ensure_ascii=False, separators=(",", ":")).encode()


@dataclass(frozen=True)
class Route:
    # Owned by the synthetic trusted transport registry, never deserialized from a response.
    context: RequestContext
    head: WorkspaceHead
    store: EncryptedStore


class CheckedRestoreStore:
    """TEST-ONLY adapter, not an expected_head option in the actual Python host.

    The actual host calls open once and uses THIS checked native workspace for the
    whole stream. No load/check/reopen gap and no replaced Core restoration logic.
    """
    def __init__(self, actual, expected_head=None):
        self.actual, self.scope, self.expected_head = actual, actual.scope, expected_head
        self.opens = 0

    def open(self):
        self.opens += 1
        workspace = self.actual.open()
        if self.expected_head is not None and workspace.head() != self.expected_head:
            workspace.close()
            raise HostError("workspace_conflict")
        return workspace


class ToolEnvelopeTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="datafog-python-tool-envelope-")
        self.database = Path(self.temporary.name) / "state.sqlite"
        self.stores = []
        for value in (10, 20):
            scope = Scope(identifier(1), identifier(value), identifier(value + 1))
            store = EncryptedStore(self.database, scope, os.urandom(32))
            with Workspace.create(scope.workspace, identifier(value + 2)) as workspace:
                store.create(workspace)
            self.stores.append(store)
            # Commit the common alias first. Concurrent same-alias requests below
            # are acknowledged no-op saves, not a claim of two state-changing CAS winners.
            self.prepare(store, VALUE, register=False)
        self.channels, self.capabilities = {}, {}
        self.restore_stores = []

    def tearDown(self):
        self.temporary.cleanup()

    def prepare(self, store, value=VALUE, *, register=True, barrier=None):
        original = envelope(value, outbound=True)
        context = RequestContext(store.scope, os.urandom(16))
        result = prepare_and_handoff(store, context, original["content"][0]["text"],
            [WorkspaceFinding("PERSON", value, TextRange(0, len(value.encode())))], approved=True,
            checkpoint=lambda phase: barrier.wait(timeout=5) if barrier is not None and phase == "prepared" else None)
        sent = replaced(original, result["text"])
        self.assertEqual(opaque_bytes(sent), opaque_bytes(original))
        self.assertEqual(sent["rawToolArguments"].encode(), OPAQUE_ARGUMENTS.encode())
        if not register:
            return sent
        # Registration snapshots an authenticated saved head. The concurrent
        # fixture above reuses a committed alias and cannot change that head.
        with store.open() as workspace:
            route = Route(context, workspace.head(), store)
        channel, capability = object(), object()
        self.channels[channel] = route
        self.capabilities[capability] = route
        return {"channel": channel, "capability": capability, "owner": store.scope, "sent": sent}

    def authorize(self, item, *, capability=_OMITTED, owner=_OMITTED, channel=_OMITTED):
        capability = item["capability"] if capability is _OMITTED else capability
        owner = item["owner"] if owner is _OMITTED else owner
        channel = item["channel"] if channel is _OMITTED else channel
        if type(channel) is not object or type(capability) is not object or type(owner) is not Scope:
            raise HostError("invalid_response_context")
        route = self.channels.get(channel)
        if route is None or self.capabilities.get(capability) is not route or owner != route.context.scope:
            raise HostError("invalid_response_context")
        return route

    def strict(self, item, body, **routing):
        route = self.authorize(item, **routing)
        checked = CheckedRestoreStore(route.store)
        self.restore_stores.append(checked)
        restored = restore_response(checked, route.context, route.context, {"text": body["content"][0]["text"]})
        result = replaced(body, restored["text"])
        self.assertEqual(opaque_bytes(body), opaque_bytes(result))
        return result

    def progressive(self, item, body, chunks, accept, *, cancelled=lambda: False, **routing):
        route = self.authorize(item, **routing)
        checked = CheckedRestoreStore(route.store, route.head)
        self.restore_stores.append(checked)
        before = opaque_bytes(body)
        try:
            return restore_text_chunks(checked, route.context, route.context, chunks, accept,
                progressive=True, cancelled=cancelled)
        finally:
            self.assertEqual(checked.opens, 1, "the same authenticated checked workspace must feed the whole stream")
            self.assertEqual(opaque_bytes(body), before)
            self.assertEqual(body["rawToolArguments"].encode(), OPAQUE_ARGUMENTS.encode())

    def test_concurrent_equal_values_same_alias_and_unforgeable_request_routes(self):
        with ThreadPoolExecutor(max_workers=2) as pool:
            barrier = Barrier(2)
            different = list(pool.map(lambda store: self.prepare(store, barrier=barrier), self.stores))
            barrier = Barrier(2)
            same = list(pool.map(lambda store: self.prepare(store, barrier=barrier), [self.stores[0], self.stores[0]]))
        self.assertEqual(same[0]["sent"]["content"][0]["text"], same[1]["sent"]["content"][0]["text"])
        for item in different + same:
            body = envelope(item["sent"]["content"][0]["text"])
            original = copy.deepcopy(body)
            self.assertEqual(self.strict(item, body)["content"][0]["text"], VALUE)
            self.assertEqual(body, original)
        first, other = different
        invalid = [(first, {"capability": other["capability"]}),
            (first, {"owner": other["owner"]}), (first, {"capability": object()}),
            (first, {"channel": object()}), (same[0], {"capability": same[1]["capability"]}),
            (first, {"capability": None}), (first, {"owner": None}), (first, {"channel": None}),
            (first, {"capability": {}}), (first, {"channel": "untrusted_provider_id"}), (first, {"owner": {}})]
        for item, routing in invalid:
            prior = len(self.restore_stores)
            with self.assertRaises(HostError) as caught:
                self.strict(item, envelope(item["sent"]["content"][0]["text"]), **routing)
            self.assertEqual(caught.exception.code, "invalid_response_context")
            self.assertEqual(len(self.restore_stores), prior, "authorization must precede every restore/open")
            consumed, delivered = [], []
            def chunks():
                consumed.append(True)
                yield b"unused"
            with self.assertRaises(HostError) as caught:
                self.progressive(item, envelope("unused"), chunks(), delivered.append, **routing)
            self.assertEqual(caught.exception.code, "invalid_response_context")
            self.assertEqual(len(self.restore_stores), prior)
            self.assertEqual(consumed, [])
            self.assertEqual(delivered, [])
        for store in self.stores:
            with store.open() as workspace:
                self.assertEqual(workspace.recall("Opaque Tool Person")["findings"], [])
            with store.connection() as db:
                ciphertext = db.execute("SELECT snapshot FROM workspaces WHERE tenant=? AND workspace=?", (store.scope.tenant, store.scope.workspace)).fetchone()[0]
            self.assertNotIn(VALUE.encode(), ciphertext)
            self.assertNotIn(OPAQUE_ARGUMENTS.encode(), ciphertext)

    def test_strict_no_partial_envelope_and_explicit_current_workspace_compatibility_limit(self):
        item = self.prepare(self.stores[0])
        marker = item["sent"]["content"][0]["text"]
        self.assertNotEqual(marker, UNKNOWN)
        invalid = envelope(marker + " " + UNKNOWN)
        before = copy.deepcopy(invalid)
        with self.assertRaises(DataFogWorkspaceError) as caught:
            self.strict(item, invalid)
        self.assertEqual(caught.exception.code, "invalid_marker")
        self.assertFalse(hasattr(caught.exception, "text"))
        self.assertEqual(invalid, before)
        # A trusted request route is NOT an issuance ledger. The existing strict
        # helper intentionally sees the current workspace, including a later alias.
        later = self.prepare(self.stores[0], "Synthetic Later Person")
        output = self.strict(item, envelope(later["sent"]["content"][0]["text"]))
        self.assertEqual(output["content"][0]["text"], "Synthetic Later Person")

    def test_progressive_checks_both_head_fields_before_reading_input(self):
        item = self.prepare(self.stores[0])
        original = self.authorize(item)
        for head in [WorkspaceHead(original.head.revision + 1, original.head.commit_id),
                WorkspaceHead(original.head.revision, identifier(99))]:
            changed = Route(original.context, head, original.store)
            self.channels[item["channel"]] = changed
            self.capabilities[item["capability"]] = changed
            consumed, delivered = [], []
            def chunks():
                consumed.append(True)
                yield item["sent"]["content"][0]["text"].encode()
            with self.assertRaises(HostStreamFailure) as caught:
                self.progressive(item, envelope("unused text slot"), chunks(), delivered.append)
            self.assertEqual(caught.exception.code, "workspace_conflict")
            self.assertFalse(caught.exception.complete)
            self.assertEqual(caught.exception.accepted["bytes"], 0)
            self.assertEqual(consumed, [])
            self.assertEqual(delivered, [])

    def test_progressive_unicode_completion_late_failure_cancellation_and_stale_head(self):
        item = self.prepare(self.stores[0])
        marker = item["sent"]["content"][0]["text"]
        body = envelope(marker)
        text = ("👋 é " + marker + "!").encode()
        events = []
        with self.stores[0].open() as workspace:
            before = workspace.export_bundle()
        result = self.progressive(item, body, [text[:2], text[2:8], text[8:]], events.append)
        self.assertTrue(result["complete"])
        self.assertEqual("".join(event["text"] for event in events), "👋 é " + VALUE + "!")
        self.assertTrue(events[-1]["complete"])
        partial = []
        safe = "safe 👋 ".encode()
        with self.assertRaises(HostStreamFailure) as caught:
            self.progressive(item, body, [safe, UNKNOWN.encode()], partial.append)
        error = caught.exception
        self.assertEqual(error.code, "invalid_marker")
        self.assertFalse(error.complete)
        self.assertEqual(error.accepted, {"bytes":10, "codepoints":7, "utf16":8, "restoredMarkers":0})
        self.assertEqual([event["text"] for event in partial], ["safe 👋 "])
        self.assertFalse(hasattr(error, "text"))
        cancelled, events = False, []
        def accept(event):
            nonlocal cancelled
            events.append(event)
            cancelled = True
        with self.assertRaises(HostStreamFailure) as caught:
            self.progressive(item, body, [safe, text], accept, cancelled=lambda: cancelled)
        self.assertEqual(caught.exception.code, "cancelled")
        self.assertFalse(caught.exception.complete)
        self.assertEqual(caught.exception.accepted["bytes"], 10)
        self.assertEqual([event["text"] for event in events], ["safe 👋 "])
        other = self.prepare(self.stores[1])
        with self.assertRaises(HostError) as caught:
            self.progressive(item, body, [text], events.append, capability=other["capability"])
        self.assertEqual(caught.exception.code, "invalid_response_context")
        with self.stores[0].open() as workspace:
            self.assertEqual(workspace.export_bundle(), before, "response restoration is read-only")
        self.prepare(self.stores[0], "Synthetic Later Person")
        consumed, delivered = [], []
        def chunks():
            consumed.append(True)
            yield text
        with self.assertRaises(HostStreamFailure) as caught:
            self.progressive(item, body, chunks(), delivered.append)
        self.assertEqual(caught.exception.code, "workspace_conflict")
        self.assertFalse(caught.exception.complete)
        self.assertEqual(caught.exception.accepted["bytes"], 0)
        self.assertEqual(consumed, [])
        self.assertEqual(delivered, [])
