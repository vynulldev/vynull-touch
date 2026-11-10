#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Native Linux releases for Vynull Touch; Python 3.9+, standard library only."""
import argparse
import contextlib
import datetime
import fcntl
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shlex
import shutil
import signal
import struct
import subprocess
import sys
import tarfile
import tempfile
import time
import uuid

APP = 'vynull-touch'
ROOT = Path(__file__).resolve().parents[1]
GIB = 1024 ** 3
SOURCE_ITEMS = ('Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', 'build.rs', 'LICENSE',
                'README.md', 'ROADMAP.md', 'src', 'scripts', 'packaging', '.cargo', 'tests')
PACKAGES = ('x11', 'xkbcommon', 'wayland-client', 'egl', 'gl', 'alsa', 'libpulse')


class ReleaseError(Exception):
    pass


def require(condition, message):
    if not condition:
        raise ReleaseError(message)


def digest(path):
    result = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            result.update(chunk)
    return result.hexdigest()


def run_id():
    return datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%SZ-') + uuid.uuid4().hex[:8]


def absolute(value):
    path = Path(value).expanduser()
    require(path.is_absolute(), f'Use an absolute path: {value}')
    return path.resolve()


def capture(command, **kwargs):
    result = subprocess.run(command, text=True, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, timeout=30, **kwargs)
    require(result.returncode == 0, f'{shlex.join(map(str, command))} failed:\n{result.stdout.strip()}')
    return result.stdout.strip()


def write_json(path, data):
    path = Path(path)
    fd, stage = tempfile.mkstemp(prefix='.' + path.name + '-', dir=path.parent)
    try:
        with os.fdopen(fd, 'w') as stream:
            json.dump(data, stream, indent=2, sort_keys=True)
            stream.write('\n')
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(stage, path)
        sync_dir(path.parent)
    finally:
        if os.path.exists(stage):
            os.unlink(stage)


def sync_dir(path):
    fd = os.open(path, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


@contextlib.contextmanager
def lock(path):
    with Path(path).open('a') as stream:
        try:
            fcntl.flock(stream, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise ReleaseError(f'Another release command holds {path}') from None
        yield


def space(path, required):
    free = shutil.disk_usage(path).free
    require(free >= required, f'{path}: {free / GIB:.1f} GiB free; need {required / GIB:.1f} GiB')
    return free


def storage(build_dir, mount, min_free):
    # Never create a missing build directory: a missing USB disk must not send
    # a large Cargo build into the Pi's root filesystem instead.
    path = absolute(build_dir)
    require(path.is_dir(), f'Build directory is missing: {path}. Mount the build disk first, then create the directory.')
    if mount:
        mounted = absolute(mount)
        require(path == mounted or mounted in path.parents, f'{path} is outside required mount {mounted}')
        points = set()
        for line in Path('/proc/self/mountinfo').read_text().splitlines():
            points.add(re.sub(r'\\([0-7]{3})', lambda m: chr(int(m[1], 8)), line.split()[4]))
        require(str(mounted) in points, f'Required build disk is not mounted at {mounted}')
    require(os.access(path, os.W_OK), f'Build directory is not writable: {path}')
    return path, space(path, min_free * GIB)


def source_files(source):
    source = absolute(source)
    for item in ('Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', 'src/main.rs'):
        require((source / item).is_file(), f'Missing source file: {source / item}')
    files = []
    for item in SOURCE_ITEMS:
        root = source / item
        if not root.exists():
            continue
        for path in ([root] if not root.is_dir() else [root, *sorted(root.rglob('*'))]):
            if '__pycache__' in path.parts or path.suffix == '.pyc':
                continue
            require(not path.is_symlink(), f'Source snapshot cannot include a symlink: {path}')
            if path.is_file():
                files.append(path.relative_to(source))
    return sorted(files)


def toolchain(source, env):
    text = (source / 'rust-toolchain.toml').read_text()
    match = re.search(r'^channel\s*=\s*"([^"]+)"', text, re.M)
    require(match, 'rust-toolchain.toml must pin a toolchain')
    pinned = match[1]
    rustup = shutil.which('rustup', path=env['PATH'])
    if rustup:
        # `rustup run` fails when the toolchain is absent; preflight never installs it.
        prefix = [rustup, 'run', pinned]
        cargo, rustc = prefix + ['cargo'], prefix + ['rustc']
    else:
        cargo, rustc = ['cargo'], ['rustc']
    version = capture(rustc + ['--version'], env=env, cwd=source)
    require(version.split()[1] == pinned, f'Expected Rust {pinned}, found {version}')
    capture(cargo + ['--version'], env=env, cwd=source)
    details = capture(rustc + ['-vV'], env=env, cwd=source)
    host = next(line.split(': ', 1)[1] for line in details.splitlines() if line.startswith('host: '))
    return cargo, version, host


def check(args):
    require(sys.platform == 'linux', 'Native release commands require Linux')
    source = absolute(args.source)
    source_files(source)
    build_dir, free = storage(args.build_dir, args.require_mount, args.min_free_gib)
    env = os.environ.copy()
    for name, value in [('CARGO_HOME', args.cargo_home), ('RUSTUP_HOME', args.rustup_home)]:
        if value:
            path = absolute(value)
            require(path.is_dir(), f'{name} is missing: {path}')
            env[name] = str(path)
    if env.get('CARGO_HOME'):
        env['PATH'] = str(Path(env['CARGO_HOME']) / 'bin') + os.pathsep + env.get('PATH', os.defpath)
    else:
        env.setdefault('PATH', os.defpath)
    require(not env.get('CARGO_BUILD_TARGET'), 'Unset CARGO_BUILD_TARGET; this command builds natively')
    for name in ('cc', 'c++', 'ar', 'cmake', 'pkg-config', 'ldd', 'nice'):
        require(shutil.which(name, path=env['PATH']), f'Missing build dependency: {name}')
    missing = [name for name in PACKAGES if subprocess.run(
        ['pkg-config', '--exists', name], env=env, stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL, timeout=10).returncode != 0]
    require(not missing, 'Missing native development packages: ' + ', '.join(missing))
    cargo, rustc, host = toolchain(source, env)
    require(host.split('-')[0] == machine(), f'Rust target {host} does not match this machine ({machine()})')
    env.update(CARGO_TARGET_DIR=str(build_dir / 'target'), TMPDIR=str(build_dir / 'tmp'))
    result = dict(source=str(source), build_dir=str(build_dir), free_gib=round(free / GIB, 2),
                  rustc=rustc, target=host, packages=list(PACKAGES))
    print(json.dumps(result, indent=2), flush=True)
    return result, env, cargo


def machine():
    name = platform.machine()
    return {'arm64': 'aarch64', 'amd64': 'x86_64'}.get(name, name)


def verify_binary(path, expected=None):
    path = Path(path)
    require(path.is_file() and not path.is_symlink(), f'Expected a regular executable: {path}')
    if expected:
        require(digest(path) == expected, f'Checksum mismatch: {path}')
    with path.open('rb') as stream:
        header = stream.read(20)
    require(len(header) == 20 and header[:6] == b'\x7fELF\x02\x01', 'Expected a 64-bit little-endian ELF binary')
    arch = {62: 'x86_64', 183: 'aarch64'}.get(struct.unpack('<H', header[18:20])[0])
    require(arch == machine(), f'Binary architecture {arch} does not match {machine()}')
    dependencies = capture(['ldd', str(path)])
    require('not found' not in dependencies, f'Missing runtime libraries:\n{dependencies}')
    require(capture([str(path), '--help']).startswith('Vynull Touch'), f'Unexpected executable: {path}')
    return dependencies


def copy_verified(source, destination, checksum):
    shutil.copyfile(source, destination)
    os.chmod(destination, 0o755)
    with Path(destination).open('rb') as stream:
        os.fsync(stream.fileno())
    require(digest(destination) == checksum, f'Staged checksum mismatch: {destination}')


def build(args):
    info, env, cargo = check(args)
    directory = Path(info['build_dir'])
    with lock(directory / '.vynull-touch-build.lock'):
        run = directory / 'releases' / run_id()
        source = run / 'source'
        source.mkdir(parents=True)
        original = Path(info['source'])
        hashes = []
        for relative in source_files(original):
            target = source / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(original / relative, target)
            hashes.append(f'{digest(target)}  {relative.as_posix()}\n')
        inventory = ''.join(hashes)
        (run / 'source-files.sha256').write_text(inventory)
        (directory / 'tmp').mkdir(exist_ok=True)
        print(f'Build record: {run}', flush=True)
        env['VYNULL_TOUCH_BUILD_ID'] = run.name
        write_json(run / 'status.json', dict(status='building', target=info['target']))
        try:
            with (run / 'build.log').open('w') as log:
                for verb in ('test', 'build'):
                    command = ['nice', '-n', '10', *cargo, verb, '--release', '--locked',
                               '--target', info['target'], '-j', str(args.jobs)]
                    if not args.online:
                        command.append('--offline')
                    print(shlex.join(command), flush=True)
                    log.write(shlex.join(command) + '\n')
                    log.flush()
                    result = subprocess.run(command, cwd=source, env=env, stdout=log, stderr=subprocess.STDOUT)
                    require(result.returncode == 0, f'{verb} failed. See {run / "build.log"}')
            binary = directory / 'target' / info['target'] / 'release' / APP
            dependencies = verify_binary(binary)
            artifact = run / 'artifact'
            artifact.mkdir()
            checksum = digest(binary)
            copy_verified(binary, artifact / APP, checksum)
            (artifact / 'SHA256SUMS').write_text(f'{checksum}  {APP}\n')
            (artifact / 'dependencies.txt').write_text(dependencies + '\n')
            manifest = dict(schema=1, application=APP, build_id=run.name,
                            sha256=checksum, source_sha256=hashlib.sha256(inventory.encode()).hexdigest(),
                            target=info['target'], rustc=info['rustc'], tests_passed=True,
                            hardware_audio_test='not requested', system=platform.platform())
            write_json(artifact / 'manifest.json', manifest)
            write_json(run / 'status.json', dict(status='passed', artifact=str(artifact)))
            if args.result:
                write_json(absolute(args.result), dict(artifact=str(artifact)))
            print(f'Ready: {artifact}\nSHA256: {checksum}', flush=True)
            return artifact
        except BaseException:
            write_json(run / 'status.json', dict(status='failed', log=str(run / 'build.log')))
            raise


def read_artifact(artifact, tested=True):
    artifact = absolute(artifact)
    manifest = json.loads((artifact / 'manifest.json').read_text())
    require(manifest.get('schema') == 1 and manifest.get('application') == APP, 'Unsupported release manifest')
    require(re.fullmatch('[0-9a-f]{64}', manifest.get('sha256', '')), 'Invalid manifest checksum')
    require(not tested or manifest.get('tests_passed') is True, 'Release has no successful test record')
    verify_binary(artifact / APP, manifest['sha256'])
    return artifact, manifest


def process_identity(pid):
    # The process start tick distinguishes reuse of the same numeric PID.
    return Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()[19]


def running_ui(installed):
    found = []
    for directory in Path('/proc').iterdir():
        if not directory.name.isdigit():
            continue
        try:
            if directory.stat().st_uid != os.getuid():
                continue
            executable = os.readlink(directory / 'exe').removesuffix(' (deleted)')
            if executable != str(installed):
                continue
            args = (directory / 'cmdline').read_bytes().split(b'\0')
            environment = {}
            for entry in (directory / 'environ').read_bytes().split(b'\0'):
                key, sep, value = entry.partition(b'=')
                if sep:
                    environment[os.fsdecode(key)] = os.fsdecode(value)
            found.append(dict(pid=int(directory.name), start=process_identity(directory.name),
                              args=[os.fsdecode(arg) for arg in args[1:] if arg], env=environment))
        except (FileNotFoundError, ProcessLookupError, PermissionError):
            continue
    return found


def restart_ui(process, installed, record):
    pid = process['pid']
    require(process_identity(pid) == process['start'], 'UI process changed before restart; reopen it from the desktop')
    os.kill(pid, signal.SIGTERM)
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        try:
            alive = process_identity(pid) == process['start'] and Path(f'/proc/{pid}/exe').exists()
        except FileNotFoundError:
            alive = False
        if not alive:
            break
        time.sleep(.1)
    else:
        raise ReleaseError('UI did not exit after SIGTERM; the new binary is installed. Close and reopen the UI manually.')
    with (record / 'ui.log').open('ab') as log:
        child = subprocess.Popen([str(installed), *process['args']], env=process['env'],
                                 stdin=subprocess.DEVNULL, stdout=log, stderr=log, start_new_session=True)
    time.sleep(2)
    require(child.poll() is None, f'UI exited during startup; see {record / "ui.log"}. Use rollback to restore the previous release.')
    print(f'UI restarted: PID {child.pid}', flush=True)
    return child


def publish(artifact, manifest, installed, state, restart):
    require(not installed.is_symlink(), f'Install destination is a symlink: {installed}')
    require(not installed.exists() or installed.is_file(), f'Install destination is not a file: {installed}')
    processes = running_ui(installed)
    require(not restart or len(processes) <= 1, 'Multiple UIs are open; close extras before using --restart')
    if installed.exists() and digest(installed) == manifest['sha256']:
        print('This release is already installed; the rollback record is unchanged.', flush=True)
        if restart and processes:
            record = state / 'restarts' / run_id()
            record.mkdir(parents=True)
            return restart_ui(processes[0], installed, record)
        return
    space(installed.parent, (artifact / APP).stat().st_size + 1024 * 1024)
    space(state, (installed.stat().st_size if installed.exists() else 0) + 1024 * 1024)
    record = state / 'records' / run_id()
    record.mkdir(parents=True)
    previous = None
    if installed.exists():
        previous = record / 'previous'
        previous.mkdir()
        checksum = digest(installed)
        copy_verified(installed, previous / APP, checksum)
        write_json(previous / 'manifest.json', dict(schema=1, application=APP, sha256=checksum,
                                                  tests_passed=None, origin='previously installed binary'))
    receipt = dict(status='prepared', installed=str(installed), sha256=manifest['sha256'],
                   build_id=manifest.get('build_id'), previous=str(previous) if previous else None)
    write_json(record / 'receipt.json', receipt)
    fd, stage_name = tempfile.mkstemp(prefix='.vynull-touch-stage-', dir=installed.parent)
    os.close(fd)
    stage = Path(stage_name)
    replaced = False
    try:
        copy_verified(artifact / APP, stage, manifest['sha256'])
        verify_binary(stage, manifest['sha256'])
        os.replace(stage, installed)
        replaced = True
        sync_dir(installed.parent)
        require(digest(installed) == manifest['sha256'], 'Installed checksum differs')
        receipt['status'] = 'installed'
        write_json(record / 'receipt.json', receipt)
        write_json(state / 'latest.json', dict(record=str(record)))
    except BaseException:
        if replaced:
            if previous:
                old = json.loads((previous / 'manifest.json').read_text())
                copy_verified(previous / APP, stage, old['sha256'])
                os.replace(stage, installed)
            else:
                installed.unlink()
            sync_dir(installed.parent)
        receipt['status'] = 'failed; prior executable retained'
        write_json(record / 'receipt.json', receipt)
        raise
    finally:
        if stage.exists():
            stage.unlink()
    print(f'Installed: {installed}\nSHA256: {manifest["sha256"]}\nRecord: {record}', flush=True)
    if restart and processes:
        return restart_ui(processes[0], installed, record)
    elif processes:
        print('The open UI still runs its previous binary. Reopen it to use this release.', flush=True)
    else:
        print('Open Vynull Touch from the desktop to start this release.', flush=True)


def install(args, rollback=False):
    bin_dir = absolute(args.bin_dir)
    state = absolute(args.state_dir)
    bin_dir.mkdir(parents=True, exist_ok=True)
    state.mkdir(parents=True, exist_ok=True)
    installed = bin_dir / APP
    with lock(bin_dir / '.vynull-touch-install.lock'), lock(state / '.lock'):
        if rollback:
            record = absolute(args.record) if args.record else Path(json.loads((state / 'latest.json').read_text())['record'])
            receipt = json.loads((record / 'receipt.json').read_text())
            require(receipt['installed'] == str(installed), 'Rollback record belongs to another install destination')
            require(receipt.get('previous'), 'This was the first installation; no previous executable exists')
            require(args.record or (installed.exists() and digest(installed) == receipt['sha256']),
                    'Installed binary changed since the last release; choose an explicit --record after reviewing it')
            artifact, manifest = read_artifact(receipt['previous'], tested=False)
        else:
            artifact, manifest = read_artifact(args.artifact)
        return publish(artifact, manifest, installed, state, args.restart)


# Extract only our regular source files, never symlinks or archive-controlled
# destinations. Arguments reach the remote CLI as JSON and subprocess arrays.
BOOTSTRAP = r'''
import json, pathlib, subprocess, sys, tarfile, tempfile
options = json.loads(sys.argv[1])
with tempfile.TemporaryDirectory(prefix='vynull-touch-source-') as temporary:
    root = pathlib.Path(temporary)
    total = 0
    with tarfile.open(fileobj=sys.stdin.buffer, mode='r|gz') as archive:
        for member in archive:
            relative = pathlib.PurePosixPath(member.name)
            if not member.isfile() or relative.is_absolute() or '..' in relative.parts:
                raise RuntimeError('Invalid source archive entry')
            total += member.size
            if total > 64 * 1024 * 1024:
                raise RuntimeError('Source archive exceeds 64 MiB')
            target = root / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            with archive.extractfile(member) as source, target.open('wb') as destination:
                import shutil
                shutil.copyfileobj(source, destination)
    script = root / 'scripts/release.py'
    if options.get('rollback'):
        subprocess.run([sys.executable, str(script), 'rollback', *options['install']], check=True)
        sys.exit(0)
    result = root / 'result.json'
    command = [sys.executable, str(script), 'check' if options['check'] else 'build', '--source', str(root), *options['build']]
    if not options['check']:
        command += ['--result', str(result)]
    subprocess.run(command, check=True)
    if not options['check'] and not options['build_only']:
        artifact = json.loads(result.read_text())['artifact']
        subprocess.run([sys.executable, str(script), 'install', '--artifact', artifact, *options['install']], check=True)
'''


def deploy(args):
    require(not args.restart or not (args.check or args.build_only), '--restart requires installation')
    build_args = ['--build-dir', args.build_dir, '--min-free-gib', str(args.min_free_gib), '--jobs', str(args.jobs)]
    for flag, value in [('--require-mount', args.require_mount), ('--cargo-home', args.cargo_home), ('--rustup-home', args.rustup_home)]:
        if value:
            build_args += [flag, value]
    if args.online:
        build_args.append('--online')
    install_args = ['--bin-dir', args.bin_dir, '--state-dir', args.state_dir]
    if args.restart:
        install_args.append('--restart')
    options = dict(check=args.check, build_only=args.build_only, build=build_args, install=install_args)
    remote(args.host, args.source, options)


def remote(host, source, options):
    require(re.fullmatch(r'[A-Za-z0-9_@.\[\]:-]+', host) and not host.startswith('-'), 'Invalid SSH host')
    require(shutil.which('ssh'), 'Install an OpenSSH client first')
    source = absolute(source)
    with tempfile.TemporaryFile() as packed:
        with tarfile.open(fileobj=packed, mode='w:gz') as archive:
            for relative in source_files(source):
                archive.add(source / relative, arcname=relative.as_posix(), recursive=False)
        packed.seek(0)
        remote = shlex.join(['python3', '-c', BOOTSTRAP, json.dumps(options)])
        result = subprocess.run(['ssh', '-o', 'BatchMode=yes', '--', host, remote], stdin=packed)
        require(result.returncode == 0, 'Remote release failed. Check the printed build/install record before retrying.')


def positive(value):
    number = int(value)
    if number <= 0:
        raise argparse.ArgumentTypeError('must be positive')
    return number


def parser():
    cli = argparse.ArgumentParser(description=__doc__)
    commands = cli.add_subparsers(dest='command', required=True)
    for name in ('check', 'build', 'deploy'):
        sub = commands.add_parser(name)
        sub.add_argument('--source', default=str(ROOT))
        sub.add_argument('--build-dir', required=True, help='Existing absolute directory on build storage')
        sub.add_argument('--require-mount', help='Fail unless this absolute path is an active mount point')
        sub.add_argument('--min-free-gib', type=positive, default=8)
        sub.add_argument('--cargo-home', help='Existing Cargo cache/tool directory')
        sub.add_argument('--rustup-home', help='Existing rustup toolchain directory')
        sub.add_argument('--jobs', type=positive, default=1)
        sub.add_argument('--online', action='store_true', help='Allow Cargo to download locked dependencies')
        if name == 'build':
            sub.add_argument('--result', help='Write artifact path as JSON to this file')
        if name == 'deploy':
            sub.add_argument('--host', required=True, help='SSH destination, e.g. user@pi.local')
            sub.add_argument('--check', action='store_true', help='Only check remote prerequisites')
            sub.add_argument('--build-only', action='store_true', help='Build/test on remote host without installing')
    for name in ('install', 'rollback'):
        sub = commands.add_parser(name)
        if name == 'install':
            sub.add_argument('--artifact', required=True, help='Artifact directory produced by build')
        else:
            sub.add_argument('--record', help='Specific install record; default is the latest')
            sub.add_argument('--host', help='Run rollback over SSH instead of on this machine')
            sub.add_argument('--source', default=str(ROOT), help='Local source of the remote release helper')
    for name in ('install', 'rollback', 'deploy'):
        sub = commands.choices[name]
        sub.add_argument('--bin-dir', default='~/.local/bin')
        sub.add_argument('--state-dir', default='~/.local/state/vynull-touch/releases')
        sub.add_argument('--restart', action='store_true', help='Restart the open UI with its existing arguments/session; never start a second instance')
    return cli


def main():
    args = parser().parse_args()
    try:
        if args.command == 'check':
            check(args)
        elif args.command == 'build':
            build(args)
        elif args.command == 'deploy':
            deploy(args)
        elif args.command == 'rollback' and args.host:
            arguments = ['--bin-dir', args.bin_dir, '--state-dir', args.state_dir]
            if args.record:
                arguments += ['--record', args.record]
            if args.restart:
                arguments.append('--restart')
            remote(args.host, args.source, dict(rollback=True, install=arguments))
        else:
            install(args, rollback=args.command == 'rollback')
        return 0
    except (ReleaseError, OSError, ValueError, subprocess.SubprocessError) as error:
        print(f'Release failed: {error}', file=sys.stderr, flush=True)
        return 1


if __name__ == '__main__':
    sys.exit(main())
