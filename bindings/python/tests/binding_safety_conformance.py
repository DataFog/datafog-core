"""Regression contracts for configuration, provider and native-object safety."""

import asyncio
import copy
import pickle
import subprocess
import sys
import warnings

import datafog_core as df


def verify_config_limits():
    cycle = {}
    cycle["entities"] = cycle
    deep = []
    for _ in range(5000):
        deep = [deep]
    for value in (cycle, {"entities": deep}):
        for operation in (
            lambda: df.scan("safe text", value),
            lambda: df.scan_and_transform("safe text", value),
            lambda: df.transform("safe text", [], value),
        ):
            try:
                operation()
            except df.DataFogConfigurationError as error:
                assert error.code == "invalid_configuration"
                assert error.reason == "invalid_type"
            else:
                raise AssertionError("cyclic/deep configuration was accepted")
    # Shared children are not cycles.
    labels = ["EMAIL"]
    assert df.scan("a@b.com", {"entities": labels})[0].entity_type == "EMAIL"


def verify_native_objects():
    assert df.__version__ and "panic_probe" not in df.__all__
    with warnings.catch_warnings(record=True) as recorded:
        warnings.simplefilter("always")
        try:
            df.panic_probe()
        except df.DataFogInternalError as error:
            assert error.code == "internal_error"
        else:
            raise AssertionError("deprecated panic helper returned")
        assert len(recorded) == 1 and recorded[0].category is DeprecationWarning
    for name in df.__all__:
        value = getattr(df, name)
        if isinstance(value, type):
            assert value.__module__ == "datafog_core", (name, value.__module__)
    for start, end in ((0, 0), (1, 5)):
        value = df.TextRange(start, end)
        assert pickle.loads(pickle.dumps(value)) == value
        assert copy.deepcopy(value) == value
        assert hash(value) == hash(df.TextRange(start, end))
    try:
        df.TextRange(5, 1)
    except ValueError:
        pass
    else:
        raise AssertionError("inverted range was accepted")
    finding = df.scan("héllo a@b.com")[0]
    assert "Some(" not in repr(finding)
    assert "confidence=None" in repr(finding)
    for duplicate in (pickle.loads(pickle.dumps(finding)), copy.deepcopy(finding)):
        assert duplicate == finding and hash(duplicate) == hash(finding)
    positive_zero = df.Finding("EMAIL", "a@b.com", df.TextRange(0, 7), df.TextRange(0, 7), "test", 0.0)
    negative_zero = df.Finding("EMAIL", "a@b.com", df.TextRange(0, 7), df.TextRange(0, 7), "test", -0.0)
    assert positive_zero == negative_zero and hash(positive_zero) == hash(negative_zero)


async def verify_provider_contracts():
    class SynchronousProvider:
        def resolve_key(self, *args):
            return {"key": b"k" * 32, "resolved_version": "1"}

        def tokenize_batch(self, *args):
            return []

        def restore_batch(self, *args):
            return []

    class AsyncTokens:
        async def restore_batch(self, scope, items):
            return [{"id": item["id"], "value": "a@b.com"} for item in items]

        async def tokenize_batch(self, scope, items):
            return [{"id": item["id"], "payload": b"synthetic", "resolved_version": "1"} for item in items]

    context = {"scope": "test"}
    text = "email a@b.com"
    sync = SynchronousProvider()
    manager = df.PrivacyManager(sync, sync)
    token_config = {"transform": {"default": {"strategy": "tokenize", "token_ref": "test"}}}
    pseudonym_config = {"transform": {"default": {"strategy": "pseudonymize", "key_ref": "test"}}}
    generated = await df.PrivacyManager(token_provider=AsyncTokens()).scan_and_transform(text, token_config, context)
    for operation in (
        lambda: manager.scan_and_transform(text, pseudonym_config, context),
        lambda: manager.scan_and_transform(text, token_config, context),
        lambda: manager.restore(generated.text, context),
    ):
        try:
            await asyncio.wait_for(operation(), 2)
        except df.DataFogKeyProviderError as error:
            assert error.code in {"key_provider_error", "token_provider_error"}
        else:
            raise AssertionError("synchronous provider was accepted")


def verify_panic_privacy():
    result = subprocess.run([sys.executable, "-c", """
from datafog_core.datafog_core import _panic_probe
from datafog_core import DataFogInternalError
try:
    _panic_probe()
except DataFogInternalError as error:
    assert str(error) == 'unexpected Rust panic probe failure'
else:
    raise AssertionError('missing panic conversion')
"""], capture_output=True, text=True, timeout=15)
    assert result.returncode == 0, result.stderr
    assert not result.stderr, result.stderr


def verify():
    verify_config_limits()
    verify_native_objects()
    asyncio.run(verify_provider_contracts())
    verify_panic_privacy()


if __name__ == "__main__":
    verify()
    print("Python binding safety contracts passed.")
