"""Capability discovery contract against the installed, compiled Core wheel."""
import json
from pathlib import Path

import datafog_core as api

SUPPORTED = sorted([
    "BEARER_TOKEN", "CREDIT_CARD", "DATE", "DE_IBAN", "DE_PASSPORT_NUMBER", "DE_POSTAL_CODE", "API_KEY", "CREDENTIAL_URI",
    "DE_RESIDENCE_PERMIT_NUMBER", "DE_SOCIAL_SECURITY_NUMBER", "DE_TAX_ID",
    "DE_VAT_ID", "EMAIL", "IP_ADDRESS", "JWT", "NPI", "PERSON", "PHONE",
    "PRIVATE_KEY", "SSN", "US_ROUTING_NUMBER", "UUID", "ZIP_CODE",
])
GERMAN = [entity for entity in SUPPORTED if entity.startswith("DE_")]
DEFAULT = [entity for entity in SUPPORTED if entity not in GERMAN + ["PERSON", "UUID"]]


def verify():
    assert callable(api.capabilities)
    try:
        api.capabilities({})
    except TypeError:
        pass
    else:
        raise AssertionError("capabilities accepted arguments")
    snapshot = api.capabilities()
    assert type(snapshot) is dict
    assert snapshot["contract_version"] == 1
    assert json.loads(json.dumps(snapshot)) == snapshot
    assert snapshot["supported_entities"] == SUPPORTED
    assert snapshot["default_entities"] == DEFAULT
    assert sorted(snapshot["entities"]) == SUPPORTED
    for key in ("supported_entities", "default_entities"):
        assert snapshot[key] == sorted(set(snapshot[key]))
    assert set(snapshot["locales"]) == {"de", "de-DE", "de_DE", "en-US", "fr"}
    for locale, metadata in snapshot["locales"].items():
        assert metadata["enabled_entities"] == (GERMAN if locale.startswith("de") else [])
    for label, entity in snapshot["entities"].items():
        assert entity["scopes"] == (["structured"] if label == "PERSON" else ["structured", "text"])
        kind = "locale" if label in GERMAN else "config" if label == "UUID" else "structured" if label == "PERSON" else "default"
        assert entity["activation"]["kind"] == kind
    uuid_config = snapshot["entities"]["UUID"]["activation"]["scan_config"]
    assert uuid_config == {"detect_uuid": True}
    text = "👋 café é 550e8400-e29b-41d4-a716-446655440000"
    assert not any(f.entity_type == "UUID" for f in api.scan(text))
    uuid_findings = [f for f in api.scan(text, uuid_config) if f.entity_type == "UUID"]
    assert len(uuid_findings) == 1
    finding = uuid_findings[0]
    assert text[finding.codepoint_range.start:finding.codepoint_range.end] == finding.matched_text
    assert text.encode()[finding.byte_range.start:finding.byte_range.end].decode() == finding.matched_text
    samples = {}
    for line in (Path(__file__).resolve().parents[3] / "fixtures/german.jsonl").read_text().splitlines():
        row = json.loads(line)
        for expected in row["entities"]:
            samples.setdefault(expected["label"], row["text"])
    for locale in ("de", "de-DE", "de_DE", "DE-de", " De "):
        for label, text in samples.items():
            assert any(f.entity_type == label for f in api.scan(text, {"locale": locale})), (locale, label)
    for label, text in samples.items():
        activation = snapshot["entities"][label]["activation"]["scan_config"]
        assert any(f.entity_type == label for f in api.scan(text, activation))
    for locale in ("en-US", "fr"):
        assert api.scan("jane@example.com", {"locale": locale}) == api.scan("jane@example.com")
    for call, path in [
        (lambda: api.scan("", {"locale": "zz-ZZ"}), "/locale"),
        (lambda: api.scan_structured({}, {"locale": "zz-ZZ"}), "/locale"),
        (lambda: api.scan_and_transform("", {"scan": {"locale": "zz-ZZ"}, "transform": {"default": {"strategy": "redact"}}}), "/scan/locale"),
        (lambda: api.scan_and_transform_structured({}, {"scan": {"locale": "zz-ZZ"}, "transform": {"default": {"strategy": "redact"}}}), "/scan/locale"),
    ]:
        try:
            call()
        except api.DataFogConfigurationError as error:
            assert error.code == "invalid_configuration"
            assert error.reason == "invalid_value"
            assert error.path == path
        else:
            raise AssertionError("unsupported locale accepted")
    assert not any(f.entity_type == "PERSON" for f in api.scan("Jane Doe"))
    assert any(f.finding.entity_type == "PERSON" for f in api.scan_structured({"full_name": "Jane Doe"}).findings)
    mutated = api.capabilities()
    assert mutated is not snapshot
    mutated["supported_entities"].clear()
    mutated["default_entities"].append("FAKE")
    mutated["locales"]["de"]["enabled_entities"].clear()
    mutated["entities"]["EMAIL"]["scopes"].clear()
    mutated["entities"]["UUID"]["activation"]["scan_config"]["detect_uuid"] = False
    assert api.capabilities() == snapshot
