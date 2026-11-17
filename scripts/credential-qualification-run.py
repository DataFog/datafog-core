"""Build and qualify the real native credential provider; never publish packages.

Linux creates its own D-Bus session and GNOME keyring. macOS and Windows use
unique probe-owned accounts in their actual user credential stores. Desktop
settings and unrelated credentials remain unchanged. Disposable hosted macOS
fixtures temporarily select an owned keychain and restore the runner's settings.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import secrets
import shlex
import signal
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
SCHEMA = "datafog-real-credential-qualification/v1"
FULL_CASES = {
    "real_provider_selected", "credential_store_create", "fresh_process_restore",
    "concurrent_fresh_process_writers", "real_secret_lookup_latency", "real_open_restore_latency",
    "wrong_stored_key_fails_closed", "missing_stored_key_fails_closed",
    "recovered_key_restores", "owned_credentials_deleted",
}
BACKENDS = {"Darwin": "macos-keychain", "Windows": "windows-credential-manager", "Linux": "linux-secret-service"}


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def source_identity():
    names = subprocess.check_output(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"], cwd=ROOT,
    ).decode().split("\0")
    hashed = hashlib.sha256()
    for name in sorted(set(names)):
        path = ROOT / name
        if name and path.is_file():
            hashed.update(name.encode() + b"\0" + path.read_bytes() + b"\0")
    return {
        "commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "dirty": bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT, text=True).strip()),
        "sha256": hashed.hexdigest(),
    }


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


class QualificationError(Exception):
    pass


class Runner:
    def __init__(self, binary, directory, environment=None):
        self.binary, self.directory = binary, directory
        self.environment = dict(os.environ if environment is None else environment)
        self.steps = []
        self.termination_failed = False

    def command(self, phase, arguments, *, environment=None, input_bytes=None, timeout=45, log=False, evidence_arguments=None):
        step = {"id": phase, "command": [str(a) for a in (arguments if evidence_arguments is None else evidence_arguments)], "status": "FAIL"}
        self.steps.append(step)
        start = time.monotonic()
        try:
            process = subprocess.Popen(
                [str(a) for a in arguments], cwd=ROOT,
                env=self.environment if environment is None else environment,
                stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                start_new_session=os.name != "nt",
                creationflags=subprocess.CREATE_NEW_PROCESS_GROUP if os.name == "nt" else 0,
            )
            try:
                stdout, stderr = process.communicate(input_bytes, timeout=timeout)
            except subprocess.TimeoutExpired:
                step["timeout_seconds"] = timeout
                try:
                    if os.name == "nt":
                        # Kill the owned parent's complete tree while it still
                        # exists, so child writers cannot outlive its deadline.
                        killed = subprocess.run(["taskkill", "/PID", str(process.pid), "/T", "/F"],
                                                capture_output=True, timeout=15)
                        if killed.returncode and process.poll() is None:
                            raise OSError("owned process tree could not terminate")
                    else:
                        try:
                            os.killpg(process.pid, signal.SIGKILL)
                        except ProcessLookupError:
                            pass
                    process.communicate(timeout=15)
                except (OSError, subprocess.TimeoutExpired):
                    self.termination_failed = True
                    step["termination_status"] = "FAIL"
                    process.kill()
                    raise QualificationError(phase + "_process_tree_termination_failed") from None
                step["termination_status"] = "PASS"
                raise QualificationError(phase + "_timeout") from None
            step["exit_code"] = process.returncode
            if log:
                path = self.directory / (phase + ".log")
                path.write_bytes(stdout + stderr)
                step.update(log=str(path), log_sha256=digest(path))
            if process.returncode:
                raise QualificationError(phase + "_failed")
            step["status"] = "PASS"
            return stdout.decode("utf-8", errors="strict").strip()
        except subprocess.TimeoutExpired:
            step["timeout_seconds"] = timeout
            raise QualificationError(phase + "_timeout") from None
        except (OSError, UnicodeError):
            raise QualificationError(phase + "_unavailable") from None
        finally:
            step["duration_ms"] = (time.monotonic() - start) * 1000

    def probe(self, phase, mode="full", fixture=None, environment=None):
        path = self.directory / (phase + ".json")
        arguments = [self.binary]
        if mode != "full":
            arguments += ["--" + mode, fixture]
        arguments += ["--output", path]
        self.command(phase, arguments, environment=environment, timeout=180 if mode == "full" else 45)
        try:
            report = json.loads(path.read_text())
            cases = report["cases"]
            expected = FULL_CASES if mode == "full" else {
                "prepare": {"real_provider_selected", "credential_store_create"},
                "verify": {"fresh_process_restore"},
                "expect-unavailable": {"existing_store_unavailable_fails_closed"},
                "cleanup": {"owned_credentials_deleted"},
                "cleanup-journal": {"owned_credentials_deleted"},
                "assert-locked-keychain": {"isolated_keychain_confirmed_locked"},
            }[mode]
            names = [case["id"] for case in cases]
            if (report["schema"] != SCHEMA or report["status"] != "PASS"
                    or report["mode"] != mode or report["real_provider"] is not True
                    or report["backend"] != BACKENDS[platform.system()]
                    or not expected.issubset(names) or len(names) != len(set(names))
                    or any(case["status"] != "PASS" for case in cases)
                    or (mode in {"full", "cleanup", "cleanup-journal"} and report["cleanup_status"] != "PASS")):
                raise ValueError("invalid provider proof")
        except (OSError, ValueError, KeyError, TypeError):
            raise QualificationError(phase + "_invalid_evidence") from None
        self.steps[-1].update(report=str(path), report_sha256=digest(path))
        return report

    def cleanup_journals(self):
        if self.termination_failed:
            raise QualificationError("owned_process_cleanup_unproven_credentials_retained")
        failures = []
        for index, path in enumerate(sorted(self.directory.glob("*.cleanup.json"))):
            try:
                self.probe("journal-cleanup-" + str(index), "cleanup-journal", path)
            except QualificationError as error:
                failures.append(str(error))
        if failures:
            raise QualificationError("owned_credential_cleanup_failed")


def bus(runner, phase, method, *arguments):
    return runner.command(phase, [
        "gdbus", "call", "--session", "--dest", "org.freedesktop.secrets",
        "--object-path", "/org/freedesktop/secrets", "--method", method, *arguments,
    ], timeout=15)


def wait_for_owned_linux_service(runner, phase, control):
    # --daemonize returns before GNOME registers org.freedesktop.secrets.
    # Wait for actual bus ownership, then validate the exact private daemon.
    command = ["gdbus", "call", "--session", "--dest", "org.freedesktop.DBus", "--object-path",
               "/org/freedesktop/DBus", "--method", "org.freedesktop.DBus.GetConnectionUnixProcessID",
               "org.freedesktop.secrets"]
    step = {"id": phase + "-owner", "command": command, "status": "FAIL", "attempts": 0}
    runner.steps.append(step)
    started = time.monotonic()
    deadline = started + 15
    try:
        while time.monotonic() < deadline:
            step["attempts"] += 1
            response = subprocess.run(command, capture_output=True, env=runner.environment, timeout=3)
            if response.returncode == 0:
                match = re.fullmatch(rb"\(uint32 (\d+),\)\s*", response.stdout)
                if not match:
                    raise QualificationError("isolated_service_pid_unverified")
                pid = int(match.group(1))
                process = Path("/proc", str(pid))
                executable = process.joinpath("cmdline").read_bytes()
                if (process.stat().st_uid != os.getuid() or str(control).encode() not in executable
                        or b"gnome-keyring-daemon" not in executable):
                    raise QualificationError("isolated_service_identity_unverified")
                step.update(status="PASS", pid=pid)
                return pid
            time.sleep(0.05)
        raise QualificationError("isolated_secret_service_startup_timeout")
    except (OSError, subprocess.TimeoutExpired):
        raise QualificationError("isolated_secret_service_readiness_failed") from None
    finally:
        step["duration_ms"] = (time.monotonic() - started) * 1000


def stop_owned_linux_daemon(manifest):
    """Terminate only the daemon whose private control path we recorded."""
    if not manifest.exists():
        return
    value = json.loads(manifest.read_text())
    if value.get("schema") != "datafog-isolated-secret-service-daemon/v1":
        raise QualificationError("isolated_daemon_manifest_invalid")
    if value.get("terminated") is True:
        return
    pid, control = value.get("pid"), value.get("control_directory")
    if not isinstance(pid, int) or pid <= 1 or not isinstance(control, str) or not Path(control).is_absolute():
        raise QualificationError("isolated_daemon_manifest_invalid")
    process = Path("/proc", str(pid))
    def exited():
        try:
            return process.joinpath("stat").read_text().rsplit(")", 1)[1].split()[0] == "Z"
        except FileNotFoundError:
            return True
    if not exited():
        command = process.joinpath("cmdline").read_bytes()
        if (process.stat().st_uid != os.getuid() or control.encode() not in command
                or b"gnome-keyring-daemon" not in command):
            raise QualificationError("isolated_daemon_identity_changed")
        os.kill(pid, signal.SIGTERM)
        deadline = time.monotonic() + 5
        while not exited() and time.monotonic() < deadline:
            time.sleep(0.05)
        if not exited():
            # The exact same owned process may be forced down if graceful
            # shutdown fails. Revalidate identity before sending another signal.
            command = process.joinpath("cmdline").read_bytes()
            if control.encode() not in command or b"gnome-keyring-daemon" not in command:
                raise QualificationError("isolated_daemon_identity_changed")
            os.kill(pid, signal.SIGKILL)
            deadline = time.monotonic() + 5
            while not exited() and time.monotonic() < deadline:
                time.sleep(0.05)
            if not exited():
                raise QualificationError("isolated_daemon_did_not_exit")
    value["terminated"] = True
    write_json(manifest, value)


def linux_session(binary, directory):
    # Called only inside a newly created dbus-run-session; all storage/control
    # locations belong to this process's private temporary tree.
    runner = Runner(binary, directory)
    failure = None
    daemon_generations = []
    service_restarted = False
    daemon_manifest = directory / "isolated-daemon.json"
    with tempfile.TemporaryDirectory(prefix="datafog-real-secret-service-") as temporary:
        private = Path(temporary)
        for variable, name in (("XDG_DATA_HOME", "data"), ("XDG_CONFIG_HOME", "config"),
                               ("XDG_CACHE_HOME", "cache"), ("XDG_RUNTIME_DIR", "runtime"),
                               ("GNOME_KEYRING_CONTROL", "control")):
            path = private / name
            path.mkdir(mode=0o700)
            runner.environment[variable] = str(path)
        for variable in ("DISPLAY", "WAYLAND_DISPLAY", "SSH_AUTH_SOCK"):
            runner.environment.pop(variable, None)
        password = secrets.token_urlsafe(32).encode()
        daemon_command = ["gnome-keyring-daemon", "--daemonize", "--unlock", "--components=secrets",
                          "--control-directory", runner.environment["GNOME_KEYRING_CONTROL"]]
        fixture = private / "vault"
        def start_owned_daemon(phase):
            runner.command(phase, daemon_command, input_bytes=password, timeout=30)
            pid = wait_for_owned_linux_service(runner, phase, private / "control")
            write_json(daemon_manifest, {"schema": "datafog-isolated-secret-service-daemon/v1",
                       "pid": pid, "control_directory": str(private / "control"), "terminated": False})
            daemon_generations.append(pid)
            return pid
        try:
            original_pid = start_owned_daemon("start-isolated-secret-service")
            runner.probe("default-provider")
            runner.probe("prepare-locked-store", "prepare", fixture)
            alias = bus(runner, "read-test-collection", "org.freedesktop.Secret.Service.ReadAlias", "default")
            match = re.fullmatch(r"\(objectpath '(/org/freedesktop/secrets/collection/[A-Za-z0-9_]+)',\)", alias)
            if not match:
                raise QualificationError("default_test_collection_missing")
            collection = match.group(1)
            bus(runner, "lock-isolated-collection", "org.freedesktop.Secret.Service.Lock", json.dumps([collection]))
            locked = runner.command("verify-collection-locked", [
                "gdbus", "call", "--session", "--dest", "org.freedesktop.secrets", "--object-path", collection,
                "--method", "org.freedesktop.DBus.Properties.Get", "org.freedesktop.Secret.Collection", "Locked",
            ], timeout=15)
            if locked != "(<true>,)":
                raise QualificationError("isolated_collection_did_not_lock")
            runner.probe("locked-store-fails-closed", "expect-unavailable", fixture)
            runner.command("unlock-isolated-collection", daemon_command, input_bytes=password, timeout=30)
            runner.probe("unlocked-store-restores", "verify", fixture)
            unavailable = dict(runner.environment, DBUS_SESSION_BUS_ADDRESS="unix:path=" + str(private / "absent-bus"))
            runner.probe("unavailable-session-bus-fails-closed", "expect-unavailable", fixture, unavailable)
            runner.probe("restored-session-bus-restores", "verify", fixture)
            stopped = {"id": "stop-owned-secret-service-for-restart", "pid": original_pid, "status": "FAIL"}
            runner.steps.append(stopped)
            stop_owned_linux_daemon(daemon_manifest)
            if json.loads(daemon_manifest.read_text()).get("terminated") is not True:
                raise QualificationError("isolated_service_stop_unconfirmed")
            stopped["status"] = "PASS"
            restarted_pid = start_owned_daemon("restart-owned-secret-service")
            if restarted_pid == original_pid:
                raise QualificationError("isolated_service_did_not_restart")
            runner.probe("restarted-secret-service-restores", "verify", fixture)
            service_restarted = True
        except QualificationError as error:
            failure = str(error)
        except (OSError, ValueError):
            failure = "isolated_service_lifecycle_failed"
        finally:
            try:
                # Unlock only this isolated daemon so exact owned entries can be
                # removed even after the locked-provider deadline fails.
                runner.command("cleanup-unlock-isolated-service", daemon_command, input_bytes=password, timeout=30)
                runner.cleanup_journals()
            except QualificationError:
                failure = failure or "owned_credential_cleanup_failed"
            try:
                stop_owned_linux_daemon(daemon_manifest)
            except (OSError, ValueError, QualificationError):
                failure = failure or "isolated_daemon_cleanup_failed"
    report = {"schema": "datafog-linux-secret-service-session/v1", "status": "FAIL" if failure else "PASS",
              "isolation": "fresh-dbus-session-and-private-xdg", "physical_reboot_tested": False,
              "locked_collection_tested": any(s["id"] == "locked-store-fails-closed" and s["status"] == "PASS" for s in runner.steps),
              "unavailable_bus_tested": any(s["id"] == "unavailable-session-bus-fails-closed" and s["status"] == "PASS" for s in runner.steps),
              "secret_service_restart_tested": service_restarted, "daemon_generations": daemon_generations,
              "error": failure, "steps": runner.steps}
    write_json(directory / "linux-session.json", report)
    return not failure


def disposable_macos_keychain(runner):
    """Only disposable hosted runners may change their user Keychain defaults."""
    if (platform.system() != "Darwin" or os.environ.get("GITHUB_ACTIONS") != "true"
            or os.environ.get("RUNNER_ENVIRONMENT") != "github-hosted" or os.environ.get("RUNNER_OS") != "macOS"):
        raise QualificationError("disposable_macos_runner_required")
    runner.environment["DATAFOG_CREDENTIAL_PROBE_NONINTERACTIVE"] = "1"
    original_default = shlex.split(runner.command("read-runner-default-keychain", ["security", "default-keychain", "-d", "user"]))
    original_search = shlex.split(runner.command("read-runner-keychain-search-list", ["security", "list-keychains", "-d", "user"]))
    if len(original_default) != 1 or not original_search:
        raise QualificationError("runner_keychain_configuration_invalid")
    failure, created = None, False
    with tempfile.TemporaryDirectory(prefix="datafog-disposable-keychain-") as temporary:
        keychain = Path(temporary).resolve() / "qualification.keychain-db"
        password = secrets.token_urlsafe(32)
        fixture = Path(temporary).resolve() / "vault"
        def password_command(phase, action):
            arguments = ["security", action, "-p", password, keychain]
            evidence = ["security", action, "-p", "<isolated-test-password>", keychain]
            return runner.command(phase, arguments, evidence_arguments=evidence)
        try:
            password_command("create-owned-runner-keychain", "create-keychain")
            created = True
            password_command("unlock-owned-runner-keychain", "unlock-keychain")
            runner.command("select-owned-runner-keychain-search", ["security", "list-keychains", "-d", "user", "-s", keychain, *original_search])
            runner.command("select-owned-runner-default", ["security", "default-keychain", "-d", "user", "-s", keychain])
            runner.probe("default-provider")
            runner.probe("prepare-locked-keychain", "prepare", fixture)
            runner.command("lock-owned-runner-keychain", ["security", "lock-keychain", keychain])
            runner.probe("confirm-owned-keychain-locked", "assert-locked-keychain", keychain)
            runner.probe("locked-keychain-fails-closed", "expect-unavailable", fixture)
            password_command("unlock-owned-keychain-after-denial", "unlock-keychain")
            runner.probe("unlocked-keychain-restores", "verify", fixture)
        except QualificationError as error:
            failure = str(error)
        finally:
            try:
                if created:
                    password_command("cleanup-unlock-owned-keychain", "unlock-keychain")
                    runner.cleanup_journals()
            except QualificationError:
                failure = failure or "owned_credential_cleanup_failed"
            # Restore each setting even if credential cleanup or the other
            # setting fails. Never delete a pre-existing keychain.
            for phase, command in [
                ("restore-runner-default-keychain", ["security", "default-keychain", "-d", "user", "-s", *original_default]),
                ("restore-runner-keychain-search-list", ["security", "list-keychains", "-d", "user", "-s", *original_search]),
                ("delete-owned-runner-keychain", ["security", "delete-keychain", keychain]),
            ]:
                if phase == "delete-owned-runner-keychain" and not created:
                    continue
                try:
                    runner.command(phase, command)
                except QualificationError:
                    failure = failure or "runner_keychain_cleanup_failed"
    if failure:
        raise QualificationError(failure)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--linux-session", type=Path, help=argparse.SUPPRESS)
    parser.add_argument("--binary", type=Path, help=argparse.SUPPRESS)
    parser.add_argument("--disposable-macos-keychain", action="store_true", help="Qualify a private Keychain only on a disposable GitHub-hosted macOS runner")
    arguments = parser.parse_args()
    if arguments.linux_session:
        if platform.system() != "Linux" or not arguments.binary or not os.environ.get("DBUS_SESSION_BUS_ADDRESS"):
            parser.error("private Linux phase requires its own D-Bus session and built binary")
        return 0 if linux_session(arguments.binary.resolve(), arguments.linux_session.resolve()) else 1
    if not arguments.output:
        parser.error("--output is required")
    output = arguments.output.resolve()
    if output.exists():
        parser.error("Use a fresh evidence path; existing results are never overwritten")
    if output.parent.exists() and any(output.parent.iterdir()):
        parser.error("Use a fresh empty evidence directory; phase results are never overwritten")
    if output.is_relative_to(ROOT):
        ignored = subprocess.run(["git", "check-ignore", "--quiet", str(output)], cwd=ROOT).returncode == 0
        if not ignored:
            parser.error("Evidence inside the checkout must be under a Git-ignored directory")
    output.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    start = datetime.now(timezone.utc).isoformat()
    source = source_identity()
    target = ROOT / "target" / "credential-qualification"
    environment = dict(os.environ, CARGO_TARGET_DIR=str(target), CARGO_PROFILE_DEV_DEBUG="0", CARGO_PROFILE_TEST_DEBUG="0")
    binary = target / "release" / "examples" / ("credential-qualification.exe" if os.name == "nt" else "credential-qualification")
    runner = Runner(binary, output.parent, environment)
    failure = None
    tools = {}
    try:
        tools["rustc"] = runner.command("rust-version", ["rustc", "+1.94.1", "--version", "--verbose"])
        tools["cargo"] = runner.command("cargo-version", ["cargo", "+1.94.1", "--version"])
        hosts = re.findall(r"^host: ([A-Za-z0-9_.-]+)$", tools["rustc"], re.M)
        if len(hosts) != 1:
            raise QualificationError("native_rust_host_unverified")
        tools["native_target"] = hosts[0]
        binary = target / hosts[0] / "release" / "examples" / binary.name
        runner.binary = binary
        runner.command("build-fresh-probe", ["cargo", "+1.94.1", "build", "--release", "--locked", "--manifest-path",
                       "integrations/fogvault-native/Cargo.toml", "--target", hosts[0],
                       "--example", "credential-qualification"], timeout=1800, log=True)
        binary_hash = digest(binary)
        if platform.system() == "Linux":
            try:
                runner.command("isolated-linux-session", ["dbus-run-session", "--", sys.executable, Path(__file__).resolve(),
                               "--linux-session", output.parent, "--binary", binary], timeout=1200)
            finally:
                stop_owned_linux_daemon(output.parent / "isolated-daemon.json")
            session = json.loads((output.parent / "linux-session.json").read_text())
            if (session["status"] != "PASS" or not session["locked_collection_tested"]
                    or not session["unavailable_bus_tested"] or session.get("secret_service_restart_tested") is not True):
                raise QualificationError("incomplete_linux_provider_proof")
        elif arguments.disposable_macos_keychain:
            disposable_macos_keychain(runner)
        else:
            try:
                runner.probe("default-provider")
            finally:
                runner.cleanup_journals()
        if source_identity() != source or digest(binary) != binary_hash:
            raise QualificationError("qualification_inputs_changed")
    except (QualificationError, OSError, ValueError, KeyError, TypeError) as error:
        failure = str(error) if isinstance(error, QualificationError) else "qualification_evidence_invalid"
    report = {"schema": "datafog-native-credential-qualification/v1", "status": "FAIL" if failure else "PASS",
              "started_at": start, "finished_at": datetime.now(timezone.utc).isoformat(), "source": source,
              "artifact": {"path": str(binary), "sha256": digest(binary) if binary.is_file() else None},
              "platform": {"os": platform.system(), "architecture": platform.machine(), "python": platform.python_version()},
              "tools": tools,
              "backend": BACKENDS.get(platform.system()), "physical_reboot_tested": False,
              "existing_user_store_locked": False, "error": failure, "steps": runner.steps,
              "github": {"sha": os.environ.get("GITHUB_SHA"), "run_id": os.environ.get("GITHUB_RUN_ID")}}
    write_json(output, report)
    print(json.dumps({"status": report["status"], "report": str(output)}))
    return 1 if failure else 0


if __name__ == "__main__":
    raise SystemExit(main())
