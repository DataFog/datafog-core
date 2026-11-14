"""Embedded statements-v1 CNN: construction, routing, limits and parity rules."""

from __future__ import annotations

from datafog_core import (
    DataFogConfigurationError,
    DataFogModelError,
    EmbeddedCnnDetector,
    Finding,
)

US_BLOCK = """CHASE STATEMENT
Statement period: 01/01 - 01/31
John A. Whitfield
1408 Elm Street Apt 4B
Springfield, IL 62704

Account summary follows."""

UK_BLOCK = """BARCLAYS
Mrs Eleanor Rigby
14 Penny Lane
Liverpool L18 1DE"""


def _labels(findings: list[Finding]) -> dict[str, set[str]]:
    grouped: dict[str, set[str]] = {}
    for finding in findings:
        grouped.setdefault(finding.entity_type, set()).add(finding.matched_text)
    return grouped


def verify_descriptor() -> None:
    detector = EmbeddedCnnDetector()
    descriptor = detector.describe()
    assert descriptor["backend"] == "embedded_cnn"
    assert descriptor["model"].startswith("statements-v1")
    assert set(descriptor["labels"]) == {"PERSON", "ADDRESS"}
    assert descriptor["adapter_version"] == "statements-cnn-v1"


def verify_model_only_detection() -> None:
    detector = EmbeddedCnnDetector()
    labels = _labels(detector.scan_model(US_BLOCK).findings)
    assert labels["PERSON"] == {"John A"}
    assert labels["ADDRESS"] == {"1408 Elm Street Apt 4B\nSpringfield, IL 62704"}

    uk = _labels(detector.scan_model(UK_BLOCK).findings)
    assert uk["PERSON"] == {"Eleanor Rigby"}
    assert uk["ADDRESS"] == {"14 Penny Lane\nLiverpool L18 1DE"}


def verify_rule_merge() -> None:
    text = f"{US_BLOCK}\nQuestions: help@example.invalid"
    result = EmbeddedCnnDetector().scan(text, {"entities": ["PERSON", "ADDRESS", "EMAIL"]})
    labels = {finding.entity_type for finding in result.findings}
    assert {"PERSON", "ADDRESS", "EMAIL"} <= labels
    assert result.model.startswith("statements-v1")
    assert result.contract_version == 1
    assert len(result.evidence) == len(result.findings)


def verify_model_only_request_returns_no_rules() -> None:
    text = "Reach me at jane@example.invalid"
    merged = EmbeddedCnnDetector().scan(text)
    assert "EMAIL" in {finding.entity_type for finding in merged.findings}
    model_only = EmbeddedCnnDetector().scan_model(text)
    assert "EMAIL" not in {finding.entity_type for finding in model_only.findings}


def verify_selection_validation() -> None:
    detector = EmbeddedCnnDetector()
    for config, reason in (
        ({"entities": []}, "empty_value"),
        ({"entities": ["EMAIL"]}, None),
        ({"entities": ["NOT_A_LABEL"]}, "invalid_value"),
    ):
        try:
            detector.scan("jane@example.invalid", config)
        except DataFogConfigurationError as error:
            assert reason is not None
            assert error.reason == reason
        else:
            assert reason is None


def verify_disabled_path_matches_selection() -> None:
    text = f"{US_BLOCK}\nQuestions: help@example.invalid"
    disabled = EmbeddedCnnDetector.scan_disabled(text, {"entities": ["EMAIL"]})
    assert {finding.entity_type for finding in disabled.findings} == {"EMAIL"}
    assert disabled.model.startswith("statements-v1")

    model_only = EmbeddedCnnDetector.scan_disabled(text, {"entities": ["PERSON"]})
    assert model_only.findings == []

    try:
        EmbeddedCnnDetector.scan_disabled(text, {"entities": ["NOT_A_LABEL"]})
    except DataFogConfigurationError as error:
        assert error.reason == "invalid_value"
    else:
        raise AssertionError("disabled path skipped selection validation")


def verify_input_limits() -> None:
    detector = EmbeddedCnnDetector()
    at_limit = "a" * 65_536
    detector.scan_model(at_limit)
    try:
        detector.scan_model("a" * 65_537)
    except DataFogModelError as error:
        assert error.code == "model_resource_exhausted"
    else:
        raise AssertionError("oversized input was accepted")


def verify_ranges_are_codepoints() -> None:
    finding = EmbeddedCnnDetector().scan_model("👋\nJohn A. Whitfield\n1 Elm St").findings
    person = next(item for item in finding if item.entity_type == "PERSON")
    text = "👋\nJohn A. Whitfield\n1 Elm St"
    assert text[person.codepoint_range.start : person.codepoint_range.end] == person.matched_text
    assert person.byte_range.start > person.codepoint_range.start


def verify() -> None:
    verify_descriptor()
    verify_model_only_detection()
    verify_rule_merge()
    verify_model_only_request_returns_no_rules()
    verify_selection_validation()
    verify_disabled_path_matches_selection()
    verify_input_limits()
    verify_ranges_are_codepoints()


if __name__ == "__main__":
    verify()
    print("embedded cnn conformance: ok")
