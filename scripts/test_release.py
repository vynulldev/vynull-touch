# SPDX-License-Identifier: GPL-3.0-or-later
"""Exercise release failure paths using small native executables, never the desktop."""
import contextlib
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest import mock

import release


class ReleaseTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.fixture = tempfile.TemporaryDirectory(prefix='vynull-release-fixtures-')
        cls.root = Path(cls.fixture.name)
        code = cls.root / 'fixture.c'
        code.write_text('''#include <stdio.h>
#include <string.h>
#include <unistd.h>
int main(int argc, char **argv) {
  if (argc > 1 && !strcmp(argv[1], "--help")) {
    puts("Vynull Touch fixture " VERSION); return 0;
  }
  for (;;) pause();
}
''')
        cls.binaries = []
        for version in ['one', 'two']:
            binary = cls.root / version
            subprocess.run(['cc', str(code), '-DVERSION="' + version + '"', '-o', str(binary)], check=True)
            cls.binaries.append(binary)

    @classmethod
    def tearDownClass(cls):
        cls.fixture.cleanup()

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='vynull-release-test-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.bin = self.root / 'bin with spaces'
        self.state = self.root / 'release state'
        self.old = self.artifact('old', self.binaries[0])
        self.new = self.artifact('new', self.binaries[1])
        self.output = contextlib.redirect_stdout(io.StringIO())
        self.output.__enter__()
        self.addCleanup(self.output.__exit__, None, None, None)

    def artifact(self, name, binary):
        directory = self.root / name
        directory.mkdir()
        shutil.copy2(binary, directory / release.APP)
        release.write_json(directory / 'manifest.json', dict(schema=1, application=release.APP,
            sha256=release.digest(binary), tests_passed=True, build_id=name))
        return directory

    def args(self, verb, *extra):
        return release.parser().parse_args([verb, '--bin-dir', str(self.bin), '--state-dir', str(self.state), *map(str, extra)])

    def install(self, artifact, *extra):
        return release.install(self.args('install', '--artifact', artifact, *extra))

    def installed(self):
        return self.bin / release.APP

    def latest(self):
        return Path(json.loads((self.state / 'latest.json').read_text())['record'])

    def test_atomic_install_and_rollback_preserve_previous_executable_and_preferences(self):
        preferences = self.root / 'settings.json'
        preferences.write_text('{"server":"http://example.local:9443"}')
        self.install(self.old)
        alias = self.bin / 'vynull-pi'
        alias.symlink_to(release.APP)
        launcher = self.bin / 'vynull-touch-launch'
        launcher.write_text('existing launcher\n')
        self.install(self.new)
        record = self.latest()
        self.assertEqual(release.digest(record / 'previous' / release.APP), release.digest(self.binaries[0]))
        self.assertEqual(release.digest(self.installed()), release.digest(self.binaries[1]))
        self.assertEqual(release.digest(alias), release.digest(self.binaries[1]))
        release.install(self.args('rollback'), rollback=True)
        self.assertEqual(release.digest(self.installed()), release.digest(self.binaries[0]))
        self.assertEqual(launcher.read_text(), 'existing launcher\n')
        self.assertEqual(preferences.read_text(), '{"server":"http://example.local:9443"}')
        self.assertFalse(list(self.bin.glob('.vynull-touch-stage-*')))

    def test_bad_checksum_and_missing_test_record_leave_installed_binary_unchanged(self):
        self.install(self.old)
        record = self.latest()
        manifest_path = self.new / 'manifest.json'
        manifest = json.loads(manifest_path.read_text())
        for bad in [dict(manifest, sha256='0' * 64), dict(manifest, tests_passed=False)]:
            release.write_json(manifest_path, bad)
            with self.assertRaises(release.ReleaseError):
                self.install(self.new)
            self.assertEqual(self.latest(), record)
            self.assertEqual(release.digest(self.installed()), release.digest(self.binaries[0]))

    def test_failed_stage_validation_leaves_live_binary_and_rollback_record_untouched(self):
        self.install(self.old)
        record = self.latest()
        original = release.verify_binary

        def fail_stage(path, expected=None):
            if '.vynull-touch-stage-' in str(path):
                raise release.ReleaseError('simulated missing runtime dependency')
            return original(path, expected)

        with mock.patch.object(release, 'verify_binary', side_effect=fail_stage):
            with self.assertRaisesRegex(release.ReleaseError, 'dependency'):
                self.install(self.new)
        self.assertEqual(release.digest(self.installed()), release.digest(self.binaries[0]))
        self.assertEqual(self.latest(), record)
        self.assertFalse(list(self.bin.glob('.vynull-touch-stage-*')))

    def test_receipt_failure_after_atomic_swap_restores_previous_executable(self):
        self.install(self.old)
        record = self.latest()
        original = release.write_json

        def fail_latest(path, data):
            if Path(path).name == 'latest.json':
                raise OSError('simulated disk failure')
            return original(path, data)

        with mock.patch.object(release, 'write_json', side_effect=fail_latest):
            with self.assertRaisesRegex(OSError, 'disk failure'):
                self.install(self.new)
        self.assertEqual(release.digest(self.installed()), release.digest(self.binaries[0]))
        self.assertEqual(self.latest(), record)

    def test_first_install_has_no_rollback_and_repeat_install_keeps_history(self):
        self.install(self.old)
        with self.assertRaisesRegex(release.ReleaseError, 'first installation'):
            release.install(self.args('rollback'), rollback=True)
        self.install(self.new)
        record = self.latest()
        self.install(self.new)
        self.assertEqual(self.latest(), record)

    def test_changed_destination_requires_explicit_rollback_record(self):
        self.install(self.old)
        self.install(self.new)
        record = self.latest()
        self.installed().write_bytes(b'changed outside release tooling')
        with self.assertRaisesRegex(release.ReleaseError, 'changed since'):
            release.install(self.args('rollback'), rollback=True)
        release.install(self.args('rollback', '--record', record), rollback=True)
        self.assertEqual(release.digest(self.installed()), release.digest(self.binaries[0]))

    def test_symlink_destination_and_corrupted_rollback_are_rejected(self):
        self.bin.mkdir()
        self.installed().symlink_to(self.binaries[0])
        with self.assertRaisesRegex(release.ReleaseError, 'symlink'):
            self.install(self.new)
        self.installed().unlink()
        self.install(self.old)
        self.install(self.new)
        (self.latest() / 'previous' / release.APP).write_bytes(b'broken backup')
        with self.assertRaisesRegex(release.ReleaseError, 'Checksum mismatch'):
            release.install(self.args('rollback'), rollback=True)
        self.assertEqual(release.digest(self.installed()), release.digest(self.binaries[1]))

    def test_missing_drive_and_insufficient_space_fail_before_creating_directories(self):
        missing = self.root / 'missing-drive' / 'build'
        with self.assertRaisesRegex(release.ReleaseError, 'missing'):
            release.storage(missing, None, 8)
        self.assertFalse(missing.exists())
        with self.assertRaisesRegex(release.ReleaseError, 'not mounted'):
            release.storage(self.root, self.root, 8)
        with mock.patch.object(shutil, 'disk_usage', return_value=type('Space', (), dict(free=10))()):
            with self.assertRaisesRegex(release.ReleaseError, 'need'):
                release.storage(self.root, None, 8)

    def test_install_lock_prevents_concurrent_replacement(self):
        self.install(self.old)
        with release.lock(self.bin / '.vynull-touch-install.lock'):
            with self.assertRaisesRegex(release.ReleaseError, 'Another release'):
                self.install(self.new)
        self.assertEqual(release.digest(self.installed()), release.digest(self.binaries[0]))

    def test_architecture_check_happens_before_executing_binary(self):
        binary = self.new / release.APP
        data = bytearray(binary.read_bytes())
        data[18:20] = (183 if release.machine() == 'x86_64' else 62).to_bytes(2, 'little')
        binary.write_bytes(data)
        with mock.patch.object(release, 'capture') as execute:
            with self.assertRaisesRegex(release.ReleaseError, 'architecture'):
                release.verify_binary(binary)
            execute.assert_not_called()

    def test_restart_is_opt_in_and_restarts_old_process_even_when_artifact_is_installed(self):
        self.install(self.old)
        environment = dict(os.environ, VYNULL_RELEASE_TEST='preserved')
        old = subprocess.Popen([str(self.installed()), '--fullscreen'], env=environment)
        new = None
        try:
            self.install(self.new)
            self.assertIsNone(old.poll(), 'install without --restart must not stop the UI')
            record = self.latest()
            new = self.install(self.new, '--restart')
            old.wait(timeout=3)
            current = release.running_ui(self.installed())
            self.assertEqual(len(current), 1)
            self.assertNotEqual(current[0]['pid'], old.pid)
            self.assertEqual(current[0]['args'], ['--fullscreen'])
            self.assertEqual(current[0]['env']['VYNULL_RELEASE_TEST'], 'preserved')
            self.assertEqual(self.latest(), record, 'restart must not erase rollback history')
        finally:
            if new is not None:
                new.terminate()
                new.wait(timeout=3)
            for process in release.running_ui(self.installed()):
                os.kill(process['pid'], 15)
                try:
                    os.waitpid(process['pid'], 0)
                except ChildProcessError:
                    pass
            if old.poll() is None:
                old.terminate()
            old.wait(timeout=3)

    def test_source_snapshot_excludes_builds_and_rejects_links(self):
        source = self.root / 'source'
        (source / 'src').mkdir(parents=True)
        (source / 'target').mkdir()
        (source / 'scripts' / '__pycache__').mkdir(parents=True)
        for name in ['Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', 'src/main.rs',
                     'target/large-binary', 'scripts/__pycache__/release.pyc']:
            (source / name).write_text('fixture')
        files = release.source_files(source)
        self.assertEqual(len(files), 4)
        (source / 'src' / 'outside.rs').symlink_to(self.root / 'secret')
        with self.assertRaisesRegex(release.ReleaseError, 'symlink'):
            release.source_files(source)

    def test_remote_command_uses_quoted_arguments_and_rejects_option_hosts(self):
        args = release.parser().parse_args(['deploy', '--host', 'pi', '--build-dir', '/disk/space $(touch bad)',
                                            '--check'])
        calls = []
        with mock.patch.object(release.subprocess, 'run', side_effect=lambda cmd, **kw: calls.append(cmd) or subprocess.CompletedProcess(cmd, 0)):
            release.deploy(args)
        import shlex
        remote = shlex.split(calls[0][-1])
        self.assertEqual(remote[:2], ['python3', '-c'])
        options = json.loads(remote[3])
        self.assertIn('/disk/space $(touch bad)', options['build'])
        args.host = '-oProxyCommand=anything'
        with self.assertRaisesRegex(release.ReleaseError, 'Invalid SSH host'):
            release.deploy(args)

    def test_build_publishes_only_after_tests_and_build_succeed(self):
        import sys
        source = self.root / 'source'
        (source / 'src').mkdir(parents=True)
        for name in ['Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', 'build.rs', 'src/main.rs']:
            (source / name).write_text('fixture')
        cargo = self.root / 'fake_cargo.py'
        cargo.write_text('''import os, pathlib, shutil, sys
print('fixture cargo ' + sys.argv[1], flush=True)
print('build-id ' + os.environ['VYNULL_TOUCH_BUILD_ID'], flush=True)
if sys.argv[1] == 'test':
    sys.exit(7 if os.environ.get('FAIL_TEST') else 0)
out = pathlib.Path(os.environ['CARGO_TARGET_DIR']) / sys.argv[sys.argv.index('--target') + 1] / 'release'
out.mkdir(parents=True, exist_ok=True)
shutil.copy2(os.environ['FIXTURE_BINARY'], out / 'vynull-touch')
''')
        for fail in [True, False]:
            directory = self.root / ('failed-build' if fail else 'passed-build')
            directory.mkdir()
            env = dict(os.environ, CARGO_TARGET_DIR=str(directory / 'target'),
                       TMPDIR=str(directory / 'tmp'), FIXTURE_BINARY=str(self.binaries[0]))
            if fail:
                env['FAIL_TEST'] = '1'
            info = dict(source=str(source), build_dir=str(directory), target=release.machine() + '-unknown-linux-gnu', rustc='fixture')
            args = release.parser().parse_args(['build', '--source', str(source), '--build-dir', str(directory)])
            with mock.patch.object(release, 'check', return_value=(info, env, [sys.executable, str(cargo)])):
                if fail:
                    with self.assertRaisesRegex(release.ReleaseError, 'test failed'):
                        release.build(args)
                    self.assertFalse(list(directory.glob('releases/*/artifact')))
                    status = json.loads(next(directory.glob('releases/*/status.json')).read_text())
                    self.assertEqual(status['status'], 'failed')
                else:
                    artifact = release.build(args)
                    _, manifest = release.read_artifact(artifact)
                    self.assertTrue(manifest['tests_passed'])
                    self.assertEqual(manifest['sha256'], release.digest(self.binaries[0]))
                    self.assertTrue((artifact.parent / 'source/src/main.rs').is_file())
                    self.assertTrue((artifact.parent / 'source/build.rs').is_file())
                    self.assertIn('build-id ' + manifest['build_id'], (artifact.parent / 'build.log').read_text())
                    self.assertIn('fixture cargo test', (artifact.parent / 'build.log').read_text())

    def test_remote_extraction_rejects_paths_outside_snapshot_and_links(self):
        import sys
        import tarfile
        for name, kind in [('../outside', tarfile.REGTYPE), ('linked', tarfile.SYMTYPE)]:
            data = io.BytesIO()
            with tarfile.open(fileobj=data, mode='w:gz') as archive:
                entry = tarfile.TarInfo(name)
                entry.type = kind
                entry.linkname = '/tmp/outside'
                archive.addfile(entry)
            result = subprocess.run([sys.executable, '-c', release.BOOTSTRAP, json.dumps(dict(check=True))],
                                    input=data.getvalue(), stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn(b'Invalid source archive entry', result.stderr)

    def test_remote_rollback_bootstrap_restores_previous_release(self):
        import shlex
        import sys
        self.install(self.old)
        self.install(self.new)
        run = subprocess.run

        def local_transport(command, **kwargs):
            self.assertEqual(command[0], 'ssh')
            # Execute the exact remote Python command against temporary fixture
            # paths locally, with the same streamed source archive as SSH.
            remote = shlex.split(command[-1])
            return run([sys.executable, *remote[1:]], **kwargs,
                       stdout=subprocess.PIPE, stderr=subprocess.PIPE)

        with mock.patch.object(release.subprocess, 'run', side_effect=local_transport):
            release.remote('fixture-pi', release.ROOT, dict(rollback=True, install=[
                '--bin-dir', str(self.bin), '--state-dir', str(self.state)]))
        self.assertEqual(release.digest(self.installed()), release.digest(self.binaries[0]))


if __name__ == '__main__':
    unittest.main()
