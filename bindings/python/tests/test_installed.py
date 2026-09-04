"""Verify the installed Python wheel against the checked-in fixtures."""

from __future__ import annotations

import json
import asyncio
from pathlib import Path

from datafog_core import (
    DataFogConfigurationError,
    DataFogFindingError,
    DataFogKeyProviderError,
    Finding,
    PrivacyManager,
    TextRange,
    scan,
    scan_structured,
    transform_structured,
    scan_and_transform_structured,
    discover_fields,
    scan_and_transform,
    transform,
)


ROOT = Path(__file__).resolve().parents[3]


def expected_entities(record: dict[str, object]) -> list[dict[str, object]]:
    return record["entities"]  # type: ignore[return-value]


def actual_entities(text: str) -> list[dict[str, object]]:
    return [
        {
            "label": finding.entity_type,
            "text": finding.matched_text,
            "start": finding.codepoint_range.start,
            "end": finding.codepoint_range.end,
        }
        for finding in scan(text)
    ]


def verify_contract(text: str) -> None:
    encoded = text.encode("utf-8")
    for finding in scan(text):
        assert (
            encoded[finding.byte_range.start : finding.byte_range.end].decode("utf-8")
            == finding.matched_text
        )
        assert (
            text[finding.codepoint_range.start : finding.codepoint_range.end]
            == finding.matched_text
        )
        assert finding.confidence is None
        assert finding.detector_name.startswith("datafog-core/")
        assert finding.detector_version


def verify_fixture(name: str) -> None:
    path = ROOT / "fixtures" / name
    for line in path.read_text().splitlines():
        record = json.loads(line)
        actual = actual_entities(record["text"])
        expected = expected_entities(record)
        assert actual == expected, record["id"]
        verify_contract(record["text"])


def verify_structured() -> None:
    for line in (ROOT / "fixtures" / "structured.jsonl").read_text().splitlines():
        record = json.loads(line)
        result = scan_structured(record["data"], record.get("config"))
        mappings = [dict(path=m.path, entity_type=m.entity_type, source=m.source, rule=m.rule) for m in result.mappings]
        assert mappings == record["mappings"], record["id"]
        discovered = discover_fields(record["data"], record.get("config"))
        assert [m.path for m in discovered] == [m.path for m in result.mappings]
        actual = []
        for located in result.findings:
            text = record["data"]
            for part in located.path[1:].split("/"):
                key = part.replace("~1", "/").replace("~0", "~")
                text = text[int(key)] if isinstance(text, list) else text[key]
            f = located.finding
            assert text.encode()[f.byte_range.start:f.byte_range.end].decode() == f.matched_text
            assert text[f.codepoint_range.start:f.codepoint_range.end] == f.matched_text
            assert f.confidence is None
            actual.append(dict(path=located.path, label=f.entity_type, text=f.matched_text, start=f.codepoint_range.start, end=f.codepoint_range.end))
        assert actual == record["findings"], record["id"]
    for line in (ROOT / "fixtures" / "structured-transform.jsonl").read_text().splitlines():
        record = json.loads(line)
        result = scan_and_transform_structured(record["data"],record["config"])
        explicit = transform_structured(record["data"],scan_structured(record["data"]).findings,record["config"]["transform"])
        assert result.data == record["expected_data"], record["id"]
        assert explicit.data == result.data
        assert all(not hasattr(r.transformation,"matched_text") for r in result.transformations)
    cycle = {}
    cycle["cycle"] = cycle
    try:
        scan_structured({}, cycle)
    except DataFogConfigurationError:
        pass
    else:
        raise AssertionError("cyclic options accepted")
    for data in [None, "secret-value", {1: "name"}, {"n": 2**100}, {"n": float("nan")}, {"n": float("inf")}, {"tuple": (1, 2)}, cycle]:
        try:
            scan_structured(data)
        except DataFogConfigurationError as error:
            assert error.code == "invalid_configuration"
            assert error.path == "/data"
            assert "secret-value" not in str(error)
        else:
            raise AssertionError("invalid structured input accepted")


def main() -> None:
    verify_structured()
    verify_fixture("development.jsonl")
    verify_fixture("final.jsonl")
    emoji_finding = scan("👋 jane@example.com")[0]
    assert (emoji_finding.byte_range.start, emoji_finding.byte_range.end) == (5, 21)
    assert (emoji_finding.codepoint_range.start, emoji_finding.codepoint_range.end) == (2, 18)
    assert scan("Email jane@example.com", {"locale": "en-US"}) == scan(
        "Email jane@example.com"
    )

    text = "👋 jane@example.com and jane@example.com"
    explicit = transform(text, scan(text), {"default": {"strategy": "redact"}})
    convenience = scan_and_transform(
        text, {"transform": {"default": {"strategy": "redact"}}}
    )
    assert explicit == convenience
    assert explicit.text == "👋 [EMAIL] and [EMAIL]"
    assert len(explicit.transformations) == 2
    first = explicit.transformations[0]
    assert not hasattr(first, "finding")
    assert not hasattr(first, "matched_text")
    assert first.entity_type == "EMAIL"
    assert first.detector_name.startswith("datafog-core/")
    assert first.replacement == "[EMAIL]"
    assert (first.output_byte_range.start, first.output_byte_range.end) == (5, 12)
    assert (first.output_codepoint_range.start, first.output_codepoint_range.end) == (2, 9)

    masked = scan_and_transform(
        "Email jane@example.com",
        {"transform": {"default": {"strategy": "mask"}}},
    )
    assert masked.text == "Email ****************"

    partially_masked = scan_and_transform(
        "Email jane@example.com",
        {
            "transform": {
                "default": {
                    "strategy": "mask",
                    "character": "•",
                    "reveal": {"direction": "last", "count": 4},
                }
            }
        },
    )
    assert partially_masked.text == "Email ••••••••••••.com"
    assert partially_masked.transformations[0].strategy == "mask"
    assert partially_masked.transformations[0].replacement == "••••••••••••.com"
    assert (
        partially_masked.transformations[0].output_byte_range.start,
        partially_masked.transformations[0].output_byte_range.end,
    ) == (6, 46)

    unchanged = scan_and_transform(
        "Email jane@example.com",
        {
            "transform": {
                "default": {
                    "strategy": "mask",
                    "reveal": {"direction": "first", "count": 99},
                }
            }
        },
    )
    assert unchanged.text == "Email jane@example.com"

    removed = scan_and_transform(
        "Email jane@example.com today",
        {"transform": {"default": {"strategy": "remove"}}},
    )
    assert removed.text == "Email  today"
    assert removed.transformations[0].strategy == "remove"
    assert removed.transformations[0].replacement == ""
    assert (
        removed.transformations[0].output_codepoint_range.start,
        removed.transformations[0].output_codepoint_range.end,
    ) == (6, 6)

    invalid_configs = [
        {"strategy": "mask", "character": ""},
        {"strategy": "mask", "character": "**"},
        {"strategy": "mask", "character": " "},
        {"strategy": "mask", "unexpected": True},
        {"strategy": "remove", "character": "*"},
        {
            "strategy": "mask",
            "reveal": {"direction": "last", "count": -1},
        },
        {
            "strategy": "mask",
            "reveal": {"direction": "last", "count": True},
        },
        {
            "strategy": "mask",
            "reveal": {"direction": "middle", "count": 4},
        },
    ]
    for invalid_config in invalid_configs:
        try:
            scan_and_transform(
                "Email jane@example.com",
                {"transform": {"default": invalid_config}},
            )
        except DataFogConfigurationError as error:
            assert error.code == "invalid_configuration"
            assert error.path.startswith("/transform/default")
        else:
            raise AssertionError(f"invalid configuration was accepted: {invalid_config}")

    invalid = Finding(
        "EMAIL",
        "jane@example.com",
        TextRange(5, 21),
        TextRange(2, 18),
        "test-detector",
        confidence=2.0,
    )
    try:
        transform(text, [invalid], {"default": {"strategy": "redact"}})
    except DataFogFindingError as error:
        assert error.code == "invalid_finding"
        assert error.reason == "invalid_confidence"
        assert error.path == "/findings/0/confidence"
        assert error.finding_index == 0
    else:
        raise AssertionError("invalid caller-supplied finding was accepted")

    selected = scan_and_transform(
        "Email support@example.com or call (212) 555-0100",
        {
            "scan": {"locale": "en-US"},
            "transform": {
                "default": {"strategy": "redact"},
                "entities": ["EMAIL", "PHONE"],
                "overrides": {
                    "PHONE": {
                        "strategy": "mask",
                        "reveal": {"direction": "last", "count": 4},
                    }
                },
                "allow": {
                    "exact": {"EMAIL": ["support@example.com"]},
                    "regex": {},
                },
            },
        },
    )
    assert selected.text == "Email support@example.com or call **********0100"
    assert len(selected.transformations) == 1

    pseudonym_config = {
        "default": {
            "strategy": "pseudonymize",
            "key_ref": "customers/email",
            "key_version": "7",
        }
    }
    try:
        transform(text, scan(text), pseudonym_config)
    except DataFogKeyProviderError as error:
        assert error.code == "key_provider_required"
        assert error.path == "/default/key_ref"
    else:
        raise AssertionError("providerless pseudonymization was accepted")

    class Provider:
        def __init__(self, key: bytes = bytes(range(32))) -> None:
            self.key = key
            self.calls: list[tuple[str, str | None]] = []

        async def resolve_key(
            self, key_ref: str, key_version: str | None
        ) -> dict[str, object]:
            self.calls.append((key_ref, key_version))
            return {"key": self.key, "resolved_version": "7"}

    provider = Provider()
    async def provider_transform(active_provider: Provider):
        return await PrivacyManager(active_provider).scan_and_transform(
            "jane@example.com jane@example.com",
            {"transform": pseudonym_config},
        )

    pseudonymized = asyncio.run(provider_transform(provider))
    expected_token = "lIdYiXR1nTA9XURAF5GmA62F/aknbUP3Q2B31wnZ2hA="
    assert pseudonymized.text == f"{expected_token} {expected_token}"
    assert provider.calls == [("customers/email", "7")]
    for record in pseudonymized.transformations:
        assert record.strategy == "pseudonymize"
        assert record.replacement == expected_token
        assert record.key_ref == "customers/email"
        assert record.resolved_key_version == "7"
        assert not hasattr(record, "finding")
        assert not hasattr(record, "matched_text")

    try:
        asyncio.run(provider_transform(Provider(b"short")))
    except DataFogKeyProviderError as error:
        assert error.code == "invalid_key_material"
        assert error.path == "/transform/default/key_ref"
    else:
        raise AssertionError("invalid provider key material was accepted")

    class TokenProvider:
        def __init__(self) -> None:
            self.next_payload = 0
            self.records: dict[bytes, tuple[str, str, str, str]] = {}

        async def tokenize_batch(self, scope: str, items: list[dict[str, object]]):
            results = []
            for item in items:
                self.next_payload += 1
                payload = bytes([self.next_payload])
                self.records[payload] = (
                    scope,
                    str(item["token_ref"]),
                    "active-1",
                    str(item["exact_value"]),
                )
                results.append(
                    {
                        "id": item["id"],
                        "payload": payload,
                        "resolved_version": "active-1",
                    }
                )
            return results

        async def restore_batch(self, scope: str, items: list[dict[str, object]]):
            results = []
            for item in items:
                record = self.records.get(bytes(item["payload"]))
                if record is None or record[:3] != (
                    scope,
                    item["token_ref"],
                    item["resolved_version"],
                ):
                    error = RuntimeError("denied")
                    error.code = "token_access_denied"
                    raise error
                results.append({"id": item["id"], "value": record[3]})
            return results

    async def structured_round_trip():
        original = {"users":[{"first_name":"👋 José"},{"full_name":"May"}],"count":2}
        provider = Provider()
        pseudonyms = await PrivacyManager(provider).scan_and_transform_structured(original,{"transform":pseudonym_config})
        assert len(provider.calls) == 1
        assert pseudonyms.data != original
        manager = PrivacyManager(None,TokenProvider())
        context = {"scope":"tenant/α"}
        config = {"transform":{"default":{"strategy":"tokenize","token_ref":"names"}}}
        tokens = await manager.scan_and_transform_structured(original,config,context)
        restored = await manager.restore_structured(tokens.data,context)
        assert restored.data == original
        assert len(restored.restorations) == 2
        try:
            await manager.restore_structured(tokens.data,{"scope":"wrong"})
        except DataFogKeyProviderError as error:
            assert error.code == "token_access_denied"
        else:
            raise AssertionError("wrong scope was accepted")
    asyncio.run(structured_round_trip())

    async def token_round_trip():
        manager = PrivacyManager(None, TokenProvider())
        context = {"scope": "tenant/α"}
        tokenized = await manager.scan_and_transform(
            "👋 jane@example.com jane@example.com",
            {
                "transform": {
                    "default": {
                        "strategy": "tokenize",
                        "token_ref": "customers/default",
                    }
                }
            },
            context,
        )
        assert tokenized.transformations[0].replacement != tokenized.transformations[1].replacement
        assert tokenized.transformations[0].token_ref == "customers/default"
        assert tokenized.transformations[0].resolved_token_version == "active-1"
        restored = await manager.restore(tokenized.text, context)
        assert restored.text == "👋 jane@example.com jane@example.com"
        assert len(restored.restorations) == 2
        try:
            await manager.restore(tokenized.text, {"scope": "tenant/b"})
        except DataFogKeyProviderError as error:
            assert error.code == "token_access_denied"
        else:
            raise AssertionError("wrong-scope restoration was accepted")

    asyncio.run(token_round_trip())

    try:
        transform(
            text,
            scan(text),
            {"default": {"strategy": "redact"}, "overides": {}},
        )
    except DataFogConfigurationError as error:
        assert error.reason == "unknown_field"
        assert error.path == "/overides"
    else:
        raise AssertionError("unknown configuration field was accepted")

    try:
        transform(text, scan(text), {"default": object()})
    except DataFogConfigurationError as error:
        assert error.code == "invalid_configuration"
        assert error.reason == "invalid_type"
        assert error.path == "/default"
    else:
        raise AssertionError("non-JSON configuration value was accepted")

    print("Installed datafog_core wheel matches fixtures and transform contracts.")


if __name__ == "__main__":
    main()
