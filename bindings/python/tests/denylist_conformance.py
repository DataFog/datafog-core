"""Installed Python literal-deny configuration, precedence, and source offsets."""

from datafog_core import (EmbeddedCnnDetector, Finding, DataFogConfigurationError, scan, scan_and_transform, transform,
                          scan_structured, scan_and_transform_structured)


def verify() -> None:
    text = "👋 é Key ſecret Project Atlas"
    rules = [
        {"phrase": "key", "case_sensitive": False},
        {"phrase": "secret", "case_sensitive": False},
        {"phrase": "Project Atlas"},
    ]
    findings = scan(text, {"entities": ["EMAIL"], "deny": rules})
    assert len(findings) == 3
    model_only = EmbeddedCnnDetector.scan_disabled("Project Atlas jane@example.com", {
        "entities": ["PERSON"], "deny": [{"phrase": "Project Atlas"}],
    })
    assert len(model_only.findings) == 1
    assert model_only.findings[0].entity_type == "DENYLIST"
    assert model_only.findings[0].matched_text == "Project Atlas"
    for finding in findings:
        assert finding.entity_type == "DENYLIST"
        assert finding.detector_name == "datafog-core/denylist"
        assert text.encode()[finding.byte_range.start:finding.byte_range.end].decode() == finding.matched_text
        assert text[finding.codepoint_range.start:finding.codepoint_range.end] == finding.matched_text
    assert len(scan("Atlas atlas _Atlas Atlas2 中Atlas", {"deny": [{"phrase": "Atlas"}]})) == 1
    literal = scan("xA+B.y", {"deny": [{"phrase": "A+B.", "boundary": "substring"}]})
    assert len(literal) == 1 and literal[0].matched_text == "A+B."
    assert len(scan("x(A) (A)y (A)", {"deny": [{"phrase": "(A)"}]})) == 1
    assert not scan("STRASSE", {"deny": [{"phrase": "straße", "case_sensitive": False}]})
    assert not scan("é", {"deny": [{"phrase": "é"}]})
    original = "Project Atlas jane@example.com"
    scan_config = {"entities": ["EMAIL"], "deny": [{"phrase": "Project Atlas"}]}
    transform_config = {
        "default": {"strategy": "redact"},
        "entities": ["EMAIL"],
        "allow": {
            "exact": {"DENYLIST": ["Project Atlas"], "EMAIL": ["jane@example.com"]},
            "regex": {"DENYLIST": [{"pattern": ".*"}]},
        },
    }
    expected = "[DENYLIST] jane@example.com"
    assert scan_and_transform(original, {"scan": scan_config, "transform": transform_config}).text == expected
    assert transform(original, scan(original, scan_config), transform_config).text == expected
    deny_finding = scan("Project Atlas", {"deny": [{"phrase": "Project Atlas"}]})[0]
    legacy = Finding("DENYLIST", deny_finding.matched_text, deny_finding.byte_range,
                     deny_finding.codepoint_range, "custom/legacy")
    assert transform("Project Atlas", [legacy], transform_config).text == "Project Atlas"
    structured = scan_structured({"note": text}, {
        "deny": [{"phrase": "Project Atlas"}],
    })
    assert len(structured.findings) == 1
    assert structured.findings[0].path == "/note"
    assert structured.findings[0].finding.matched_text == "Project Atlas"
    result = scan_and_transform_structured({"note": original}, {
        "scan": {"deny": [{"phrase": "Project Atlas"}]}, "transform": transform_config,
    })
    assert result.data == {"note": expected}
    invalid = [None, "Atlas", [{}], [{"phrase": ""}], [{"phrase": "  "}], [{"phrase": 1}],
               [{"phrase": "Atlas", "case_sensitive": "false"}], [{"phrase": "Atlas", "boundary": "regex"}]]
    for deny in invalid:
        try:
            scan("Atlas", {"deny": deny})
        except DataFogConfigurationError:
            pass
        else:
            raise AssertionError(f"invalid deny accepted: {deny!r}")
