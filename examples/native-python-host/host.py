"""Synthetic host qualification: encryption/persistence stay outside the SDK."""
from __future__ import annotations
from dataclasses import dataclass
from contextlib import contextmanager
from pathlib import Path
import os
import sqlite3
from typing import Callable
from cryptography.exceptions import InvalidTag
from cryptography.hazmat.primitives.ciphers.aead import AESGCM
from datafog_core import Workspace, WorkspaceHead

MAX_BUNDLE_BYTES = 16 * 1024 * 1024
DOMAIN = b"datafog/synthetic-python-host/v1\0"

class HostError(Exception):
    def __init__(self, code: str):
        self.code = code
        super().__init__(code)

@dataclass(frozen=True)
class Scope:
    # Routing/authentication is supplied by the trusted application, not model text.
    tenant: bytes
    workspace: bytes
    key_id: bytes
    def __post_init__(self):
        if any(type(value) is not bytes or len(value) != 16 for value in (self.tenant, self.workspace, self.key_id)):
            raise HostError("invalid_context")

@dataclass(frozen=True)
class RequestContext:
    scope: Scope
    request_id: bytes
    def __post_init__(self):
        if type(self.scope) is not Scope or type(self.request_id) is not bytes or len(self.request_id) != 16:
            raise HostError("invalid_context")


def aad(scope: Scope, head: WorkspaceHead) -> bytes:
    return DOMAIN + scope.tenant + scope.workspace + scope.key_id + head.revision.to_bytes(8, "big") + head.commit_id


def parse_head(revision, commit_id):
    if type(revision) is not str or not 1 <= len(revision) <= 20 or not revision.isascii() or not revision.isdecimal():
        raise HostError("invalid_envelope")
    number = int(revision)
    if number > 2**64 - 1 or str(number) != revision or type(commit_id) is not bytes or len(commit_id) != 16:
        raise HostError("invalid_envelope")
    return WorkspaceHead(number, commit_id)


class EncryptedStore:
    """One authorized routing scope and externally resolved synthetic host key."""
    def __init__(self, path: Path, scope: Scope, key: bytes):
        if type(key) is not bytes or len(key) != 32:
            raise HostError("invalid_key")
        self.path, self.scope = Path(path), scope
        self._cipher = AESGCM(key)
        with self.connection() as db:
            db.execute("""CREATE TABLE IF NOT EXISTS workspaces (
                tenant BLOB NOT NULL, workspace BLOB NOT NULL, key_id BLOB NOT NULL,
                revision TEXT NOT NULL, commit_id BLOB NOT NULL, snapshot BLOB NOT NULL,
                PRIMARY KEY (tenant, workspace),
                CHECK(length(tenant)=16 AND length(workspace)=16 AND length(key_id)=16 AND length(commit_id)=16),
                CHECK(length(snapshot) BETWEEN 28 AND 16777244))""")

    @contextmanager
    def connection(self):
        db = sqlite3.connect(self.path, isolation_level=None, timeout=5)
        try:
            db.execute("PRAGMA synchronous=FULL")
            yield db
        finally:
            db.close()

    def seal(self, bundle: bytes, head: WorkspaceHead) -> bytes:
        if type(bundle) is not bytes or len(bundle) > MAX_BUNDLE_BYTES:
            raise HostError("resource_limit")
        nonce = os.urandom(12)
        return nonce + self._cipher.encrypt(nonce, bundle, aad(self.scope, head))

    def create(self, workspace: Workspace):
        if workspace.namespace() != self.scope.workspace:
            raise HostError("invalid_context")
        head = workspace.head()
        envelope = self.seal(workspace.export_bundle(), head)
        try:
            with self.connection() as db:
                db.execute("INSERT INTO workspaces VALUES(?,?,?,?,?,?)", (
                    self.scope.tenant, self.scope.workspace, self.scope.key_id,
                    str(head.revision), head.commit_id, envelope))
        except sqlite3.IntegrityError:
            raise HostError("workspace_conflict") from None

    def open(self) -> Workspace:
        try:
            with self.connection() as db:
                db.execute("BEGIN")
                # Bound the SQLite blob before materializing attacker-modified data.
                row = db.execute("SELECT key_id,revision,commit_id,length(snapshot) FROM workspaces WHERE tenant=? AND workspace=?",
                    (self.scope.tenant, self.scope.workspace)).fetchone()
                if row is None: raise HostError("state_unavailable")
                key_id, revision, commit_id, length = row
                head = parse_head(revision, commit_id)
                if key_id != self.scope.key_id or type(length) is not int or not 28 <= length <= MAX_BUNDLE_BYTES + 28:
                    raise HostError("invalid_envelope")
                envelope = db.execute("SELECT snapshot FROM workspaces WHERE tenant=? AND workspace=?",
                    (self.scope.tenant, self.scope.workspace)).fetchone()[0]
                if type(envelope) is not bytes: raise HostError("invalid_envelope")
                plaintext = self._cipher.decrypt(envelope[:12], envelope[12:], aad(self.scope, head))
                workspace = Workspace.open_bundle(plaintext)
                if workspace.namespace() != self.scope.workspace or workspace.head() != head:
                    workspace.close(); raise HostError("invalid_envelope")
                return workspace
        except InvalidTag:
            raise HostError("invalid_envelope") from None
        except sqlite3.Error:
            raise HostError("storage_failure") from None

    def compare_and_save(self, candidate: bytes, expected: WorkspaceHead, saved: WorkspaceHead,
                         *, cancelled: Callable[[], bool] = lambda: False, fault: str | None = None):
        # The trusted host verifies exactly the candidate whose save it attests.
        with Workspace.open_bundle(candidate) as checked:
            if checked.namespace() != self.scope.workspace or checked.head() != saved:
                raise HostError("invalid_context")
        envelope = self.seal(candidate, saved)
        try:
            with self.connection() as db:
                db.execute("BEGIN IMMEDIATE")
                try:
                    if cancelled(): raise HostError("cancelled")
                    changed = db.execute("""UPDATE workspaces SET revision=?,commit_id=?,snapshot=?
                        WHERE tenant=? AND workspace=? AND key_id=? AND revision=? AND commit_id=?""", (
                        str(saved.revision), saved.commit_id, envelope,
                        self.scope.tenant, self.scope.workspace, self.scope.key_id,
                        str(expected.revision), expected.commit_id)).rowcount
                    if changed != 1: raise HostError("workspace_conflict")
                    if fault == "abort_after_update": raise HostError("storage_failure")
                    if cancelled(): raise HostError("cancelled")
                    db.execute("COMMIT")
                except BaseException:
                    if db.in_transaction: db.execute("ROLLBACK")
                    raise
            if fault == "lose_ack": raise HostError("save_uncertain")
        except sqlite3.Error:
            raise HostError("storage_failure") from None


def prepare_and_handoff(store: EncryptedStore, context: RequestContext, text: str, findings,
                        *, approved: bool, acknowledge_upgrade: bool = False, cancelled: Callable[[], bool] = lambda: False,
                        checkpoint: Callable[[str], None] = lambda phase: None, fault: str | None = None):
    if context.scope != store.scope: raise HostError("invalid_context")
    if cancelled(): raise HostError("cancelled")
    with store.open() as workspace:
        with workspace.prepare(text, findings, os.urandom(16)) as change:
            checkpoint("prepared")
            if approved is not True or cancelled(): raise HostError("cancelled")
            version=change.snapshot_format()
            if version["currentVersion"]==1 and version["candidateVersion"]==2 and acknowledge_upgrade is not True:
                raise HostError("upgrade_notice_required")
            released_text = change.preview()["text"]
            parent, saved = change.expected_head(), change.candidate_head()
            checkpoint("before_save")
            store.compare_and_save(change.export_candidate_bundle(), parent, saved,
                cancelled=cancelled, fault=fault)
            checkpoint("after_save")
            if cancelled(): raise HostError("cancelled")
            workspace.activate_saved(change, parent, saved)
            checkpoint("after_activation")
            if cancelled(): raise HostError("cancelled")
            # The preview is released only after acknowledged save + activation.
            return {"context": context, "text": released_text}


def restore_response(store: EncryptedStore, expected: RequestContext, transport: RequestContext, body: dict):
    # These contexts belong to the server's outbound-request registry, not fields
    # extracted from the provider's model/tool text or supplied chat history.
    if expected != transport or expected.scope != store.scope:
        raise HostError("invalid_context")
    if type(body) is not dict or type(body.get("text")) is not str:
        raise HostError("invalid_response")
    with store.open() as workspace:
        text = workspace.restore(body["text"])
    return {**body, "text": text}
