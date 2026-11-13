#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Capture native demo screens. Needs Xvfb, libXtst, xwininfo, xclip and ImageMagick."""
import ctypes as c
import argparse
from contextlib import contextmanager
import json
import os
from pathlib import Path
import re
import select
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output-dir', type=Path, help='Screenshot destination (default: a new temporary directory, retained after capture)')
    parser.add_argument('--recording-only', action='store_true', help='Exercise recording using a private silent stereo test source and capture the Record page')
    parser.add_argument('--connection-only', action='store_true', help='Capture only the offline recovery screen')
    parser.add_argument('--grid-editor-only', action='store_true', help='Capture the beat-grid editor using fictional demo tracks')
    parser.add_argument('--device-settings-only', action='store_true', help='Capture CDJ/mixer settings using a fictional demo profile')
    parser.add_argument('--browser-settings-only', action='store_true', help='Capture CDJ browser customization using a fictional demo profile')
    parser.add_argument('--audio-settings-only', action='store_true', help='Capture audio output settings and the slimmer Inspect toolbar')
    parser.add_argument('--styles-only', action='store_true', help='Capture the four waveform palettes')
    parser.add_argument('--library-only', action='store_true', help='Capture the library table, sorting, playlists and search')
    parser.add_argument('--library-preview-only', action='store_true', help='Capture Collection and playlist audio preview controls with muted demo audio')
    parser.add_argument('--library-match-only', action='store_true', help='Capture live key/BPM highlights and their player chooser with demo tracks')
    parser.add_argument('--on-deck-only', action='store_true', help='Capture playing/cued player badges in Collection, playlists, search and Prepare using demo tracks')
    parser.add_argument('--key-matching-only', action='store_true', help='Capture saved neighboring-key choices and matching entry points using demo tracks')
    parser.add_argument('--search-only', action='store_true', help='Capture live search results above the keyboard')
    parser.add_argument('--cue-markers-only', action='store_true', help='Capture hot cue markers and scrolling waveform smoothing')
    parser.add_argument('--cue-editor-only', action='store_true', help='Capture the cue editor and its rekordbox color palette')
    parser.add_argument('--cue-defaults-only', action='store_true', help='Capture per-slot default hot cue colors in Settings')
    parser.add_argument('--prepare-only', action='store_true', help='Capture the shared Prepare playlist workflow using demo tracks')
    parser.add_argument('--inspect-grid-only', action='store_true', help='Capture Inspect grid density at Fit, 8x and 32x zoom')
    parser.add_argument('--waveform-curves-only', action='store_true', help='Capture detailed waveform contours at high zoom in all palettes')
    parser.add_argument('--player-phrases-only', action='store_true', help='Capture player phrase strips and countdowns in Cards, Waveforms and focus')
    parser.add_argument('--tag-picker-only', action='store_true', help='Capture track tag selection using a read-only fictional local catalog')
    parser.add_argument('--tag-manager-only', action='store_true', help='Capture tag/category management using a read-only fictional local catalog')
    parser.add_argument('--playlist-icons-only', action='store_true', help='Capture regular/smart playlists and folders using a fictional catalog')
    parser.add_argument('--load-only', action='store_true', help='Capture the player chooser and playing-deck confirmation')
    parser.add_argument('--beat-jump-only', action='store_true', help='Capture Inspect beat jumps and the size menu')
    parser.add_argument('--inspect-waveforms', '--waveform-overviews', action='store_true', help='Capture Inspect and Players half/full choices and the 3-band override')
    args = parser.parse_args()
    if args.output_dir is None:
        output_dir = Path(tempfile.mkdtemp(prefix='vynull-touch-screenshots-'))
    else:
        output_dir = args.output_dir.expanduser().resolve()
        output_dir.mkdir(parents=True, exist_ok=True)
    print(f'Screenshots: {output_dir}', flush=True)
    with tempfile.TemporaryDirectory(prefix='vynull-touch-capture-') as tmp:
        read_fd, write_fd = os.pipe()
        server = subprocess.Popen(['Xvfb', '-displayfd', str(write_fd), '-screen', '0', '1280x1080x24', '-nolisten', 'tcp'], pass_fds=(write_fd,), stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        os.close(write_fd)
        try:
            if not select.select([read_fd], [], [], 10)[0]:
                raise RuntimeError('Xvfb did not start within 10 seconds')
            with os.fdopen(read_fd) as display:
                number = display.readline().strip()
            if not number:
                raise RuntimeError(server.stderr.read().decode())
            env = dict(os.environ, DISPLAY=':' + number, LIBGL_ALWAYS_SOFTWARE='1', XDG_RUNTIME_DIR=tmp, LC_ALL='C')
            env['XDG_CONFIG_HOME'] = str(Path(tmp) / 'config')
            env.pop('WAYLAND_DISPLAY', None)
            if args.recording_only:
                with preview_output(env) as audio_env:
                    subprocess.run(['pactl', 'set-default-source', 'vynull_preview.monitor'], env=audio_env, check=True)
                    capture_interactions(audio_env, output_dir, recording_only=True)
                return
            if args.tag_picker_only or args.tag_manager_only or args.playlist_icons_only:
                from capture_tag_fixture import tag_fixture
                with tag_fixture(playlist_icons=args.playlist_icons_only) as origin:
                    capture_interactions(env, output_dir, tag_picker_only=args.tag_picker_only, tag_manager_only=args.tag_manager_only, playlist_icons_only=args.playlist_icons_only, server_origin=origin)
                return
            if args.browser_settings_only:
                capture_interactions(env, output_dir, browser_settings_only=True)
                return
            if args.cue_editor_only:
                capture_interactions(env, output_dir, cue_editor_only=True)
                return
            if args.cue_defaults_only:
                capture_interactions(env, output_dir, cue_defaults_only=True)
                return
            if args.prepare_only:
                capture_interactions(env, output_dir, prepare_only=True)
                return
            if args.device_settings_only:
                capture_interactions(env, output_dir, device_settings_only=True)
                return
            if args.grid_editor_only:
                capture_interactions(env, output_dir, grid_editor_only=True)
                return
            if args.audio_settings_only:
                capture_interactions(env, output_dir, audio_settings_only=True)
                return
            if args.waveform_curves_only:
                for style in ['auto', 'rgb', 'blue']:
                    capture_interactions(env, output_dir, inspect_waveform=(style, 'half'), waveform_curves_only=True)
                return
            if args.inspect_grid_only:
                capture_interactions(env, output_dir, inspect_grid_only=True)
                return
            if args.player_phrases_only:
                capture_interactions(env, output_dir, player_phrases_only=True)
                return
            if args.cue_markers_only:
                for style in ['auto', 'rgb', 'blue']:
                    capture_interactions(env, output_dir, inspect_waveform=(style, 'half'), cue_markers_only=True)
                return
            if args.search_only:
                capture_interactions(env, output_dir, search_only=True)
                return
            if args.beat_jump_only:
                capture_interactions(env, output_dir, beat_jump_only=True)
                return
            if args.load_only:
                capture_interactions(env, output_dir, load_only=True)
                return
            if args.library_only:
                for size in ['800x480', '1024x600', '1280x800']:
                    subprocess.run([str(ROOT / 'target/debug/vynull-touch'), '--demo', '--startup-page', 'collection', '--size', size, '--screenshot', str(output_dir / f'collection-{size}.png')], env=env, check=True, timeout=30)
                capture_interactions(env, output_dir, library_only=True)
                return
            if args.library_preview_only:
                with preview_output(env) as audio_env:
                    capture_interactions(audio_env, output_dir, library_preview_only=True)
                return
            if args.library_match_only:
                capture_interactions(env, output_dir, library_match_only=True)
                return
            if args.on_deck_only:
                capture_interactions(env, output_dir, on_deck_only=True)
                return
            if args.key_matching_only:
                capture_interactions(env, output_dir, key_matching_only=True)
                return
            if args.inspect_waveforms:
                for choice in [('rgb', 'half'), ('rgb', 'full'), ('blue', 'half'), ('blue', 'full'), ('3band', 'full'), ('auto', 'full')]:
                    capture_interactions(env, output_dir, inspect_waveform=choice)
                return
            if args.styles_only:
                capture_styles(env, output_dir)
                return
            if not args.connection_only:
                for size in ['800x480', '1024x600']:
                    subprocess.run([str(ROOT / 'target/debug/vynull-touch'), '--demo', '--size', size, '--screenshot', str(output_dir / f'collection-{size}.png')], env=env, check=True, timeout=30)
            subprocess.run([str(ROOT / 'target/debug/vynull-touch'), '--server', 'http://127.0.0.1:1', '--screenshot', str(output_dir / 'connection-recovery-800x480.png')], env=env, check=True, timeout=30)
            if not args.connection_only:
                capture_interactions(env, output_dir)
                capture_styles(env, output_dir)
        finally:
            server.terminate()
            server.wait(timeout=5)


@contextmanager
def preview_output(env):
    """Private PulseAudio server with a silent sink; never changes user audio."""
    with tempfile.TemporaryDirectory(prefix='vynull-preview-audio-') as directory:
        root = Path(directory)
        audio_env = dict(env)
        for name in ['runtime', 'state', 'config']:
            (root / name).mkdir(mode=0o700)
            audio_env[f'PULSE_{name.upper()}_PATH'] = str(root / name)
        audio_env['PULSE_SERVER'] = f'unix:{root}/native'
        audio_env['PULSE_COOKIE'] = str(root / 'cookie')
        audio_env['DBUS_SESSION_BUS_ADDRESS'] = f'unix:path={root}/no-session-bus'
        (root / 'cookie').write_bytes(os.urandom(256))
        config = root / 'pulse.pa'
        config.write_text(
            f'load-module module-native-protocol-unix socket={root}/native auth-cookie={root}/cookie\n'
            'load-module module-null-sink sink_name=vynull_preview rate=48000 channels=2 sink_properties=device.description=Preview_Test_Output\n'
            'set-default-sink vynull_preview\n'
        )
        with (root / 'pulse.log').open('w+') as log:
            audio = subprocess.Popen(['pulseaudio', '-n', '--daemonize=no', '--use-pid-file=no', '--exit-idle-time=-1', '--disable-shm=yes', '--log-target=stderr', f'--file={config}'], env=audio_env, stdout=log, stderr=subprocess.STDOUT)
            try:
                deadline = time.monotonic() + 10
                while time.monotonic() < deadline and audio.poll() is None:
                    ready = subprocess.run(['pactl', 'info'], env=audio_env, capture_output=True, timeout=3)
                    if ready.returncode == 0:
                        yield audio_env
                        return
                    time.sleep(.1)
                log.seek(0)
                raise RuntimeError(f'Private audio output failed to start: {log.read()}')
            finally:
                audio.terminate()
                try:
                    audio.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    audio.kill()
                    audio.wait(timeout=5)


def capture_styles(env, output_dir):
    settings = Path(env['XDG_CONFIG_HOME']) / 'vynull-touch' / 'settings.json'
    settings.parent.mkdir(parents=True, exist_ok=True)
    for style in ['auto', 'rgb', '3band', 'blue']:
        settings.write_text(json.dumps({'waveform_style': style}))
        subprocess.run([str(ROOT / 'target/debug/vynull-touch'), '--demo', '--startup-page', 'players', '--screenshot', str(output_dir / f'waveform-style-{style}-800x480.png')], env=env, check=True, timeout=30)


def capture_interactions(env, output_dir, inspect_waveform=None, library_only=False, load_only=False, beat_jump_only=False, search_only=False, cue_markers_only=False, cue_editor_only=False, inspect_grid_only=False, waveform_curves_only=False, player_phrases_only=False, tag_picker_only=False, tag_manager_only=False, playlist_icons_only=False, audio_settings_only=False, grid_editor_only=False, device_settings_only=False, browser_settings_only=False, cue_defaults_only=False, prepare_only=False, library_preview_only=False, library_match_only=False, on_deck_only=False, key_matching_only=False, recording_only=False, server_origin=None):
    # Keep the established Cards walkthrough, then capture Waveforms separately.
    settings = Path(env['XDG_CONFIG_HOME']) / 'vynull-touch' / 'settings.json'
    settings.parent.mkdir(parents=True, exist_ok=True)
    settings.write_text('{"players":{"compare":false}}')
    if recording_only:
        # Record to the default ~/Music/Vynull Recordings in a private home, so the
        # Record page shows the default folder.
        settings.write_text('{}')
        env = dict(env, HOME=str(output_dir / 'home'))
        recordings = output_dir / 'home' / 'Music' / 'Vynull Recordings'
    if library_preview_only:
        settings.write_text(json.dumps({'players': {'compare': False}, 'audio_volume': 0.0}))
    if cue_defaults_only:
        settings.write_text(json.dumps({'hot_cue_colors': [49, 56, 60, 62, 1, 5, 9, 14]}))
    if inspect_waveform:
        style, overview = inspect_waveform
        settings.write_text(json.dumps({'waveform_style': style, 'waveform_overview': overview}))
    if cue_markers_only:
        saved = json.loads(settings.read_text())
        saved['players'] = {'compare': True}
        settings.write_text(json.dumps(saved))
    xlib = c.CDLL('libX11.so.6')
    xtest = c.CDLL('libXtst.so.6')
    xlib.XOpenDisplay.argtypes = [c.c_char_p]
    xlib.XOpenDisplay.restype = c.c_void_p
    xlib.XFlush.argtypes = [c.c_void_p]
    xlib.XCloseDisplay.argtypes = [c.c_void_p]
    xlib.XResizeWindow.argtypes = [c.c_void_p, c.c_ulong, c.c_uint, c.c_uint]
    xlib.XSetInputFocus.argtypes = [c.c_void_p, c.c_ulong, c.c_int, c.c_ulong]
    xtest.XTestFakeMotionEvent.argtypes = [c.c_void_p, c.c_int, c.c_int, c.c_int, c.c_ulong]
    xtest.XTestFakeButtonEvent.argtypes = [c.c_void_p, c.c_uint, c.c_int, c.c_ulong]
    xtest.XTestFakeKeyEvent.argtypes = [c.c_void_p, c.c_uint, c.c_int, c.c_ulong]
    xlib.XKeysymToKeycode.argtypes = [c.c_void_p, c.c_ulong]
    xlib.XKeysymToKeycode.restype = c.c_uint
    display = xlib.XOpenDisplay(env['DISPLAY'].encode())
    if not display:
        raise RuntimeError('Cannot open the private display')
    mode = ['--server', server_origin, '--startup-page', 'collection'] if server_origin else ['--demo']
    app = subprocess.Popen([str(ROOT / 'target/debug/vynull-touch'), *mode], env=env)
    try:
        window = None
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            listing = subprocess.check_output(['xwininfo', '-root', '-tree'], env=env, text=True)
            match = re.search(r'(0x[0-9a-f]+) "Vynull Touch"', listing)
            if match:
                window = match.group(1)
                break
            time.sleep(.1)
        if not window:
            raise RuntimeError('Native window did not appear')
        while time.monotonic() < deadline:
            details = subprocess.check_output(['xwininfo', '-id', window], env=env, text=True)
            if 'Map State: IsViewable' in details:
                break
            time.sleep(.1)
        else:
            raise RuntimeError('Native window did not become viewable')
        xlib.XSetInputFocus(display, int(window, 16), 2, 0)
        xlib.XFlush(display)
        time.sleep(.3)

        def click(x, y, hold=.05):
            xtest.XTestFakeMotionEvent(display, 0, x, y, 0)
            xlib.XFlush(display)
            time.sleep(.1)
            xtest.XTestFakeButtonEvent(display, 1, 1, 0)
            xlib.XFlush(display)
            time.sleep(hold)
            xtest.XTestFakeButtonEvent(display, 1, 0, 0)
            xlib.XFlush(display)
            time.sleep(.2)

        def capture(name):
            subprocess.run(['import', '-window', window, str(output_dir / f'{name}-800x480.png')], env=env, check=True, timeout=10)

        def escape():
            code = xlib.XKeysymToKeycode(display, 0xff1b)
            for pressed in [True, False]:
                xtest.XTestFakeKeyEvent(display, code, pressed, 0)
                xlib.XFlush(display)
                time.sleep(.1)

        def copy_diagnostics():
            click(700, 92)
            report = subprocess.check_output(['xclip', '-o', '-selection', 'clipboard'], env=env, text=True, timeout=5)
            assert report.startswith('Vynull Touch diagnostic report'), 'System clipboard did not receive the report'
            assert 'Build:' in report and 'SERVER LOG SNAPSHOT' in report

        if recording_only:
            import math
            import struct
            import wave
            click(44, 400)
            capture('recording-idle')
            tone = output_dir / 'stereo-test-signal.wav'
            with wave.open(str(tone), 'wb') as wav:
                wav.setparams((2, 2, 48000, 0, 'NONE', 'not compressed'))
                period = b''.join(struct.pack('<hh', int(8192 * math.sin(2 * math.pi * 440 * n / 48000)), int(4096 * math.sin(2 * math.pi * 880 * n / 48000))) for n in range(48000))
                wav.writeframes(period * 60)
            playback = subprocess.Popen(['paplay', str(tone)], env=env)
            try:
                click(600, 367)  # Check levels: capture without creating a file.
                time.sleep(8)
                capture('recording-levels')
                assert not list(recordings.glob('*.wav'))
                click(280, 367)  # Start recording on the already-open input.
                time.sleep(2)
                files = list(recordings.glob('*.wav'))
                assert len(files) == 1, 'The Record button did not create a WAV'
                capture('session-recording')
                for width, height in [(800, 480), (1024, 600), (1280, 800)]:
                    xlib.XResizeWindow(display, int(window, 16), width, height)
                    xlib.XFlush(display)
                    time.sleep(.5)
                    subprocess.run(['import', '-window', window, str(output_dir / f'recording-active-{width}x{height}.png')], env=env, check=True, timeout=10)
                xlib.XResizeWindow(display, int(window, 16), 800, 480)
                xlib.XFlush(display)
                time.sleep(.5)
                before = files[0].stat().st_size
                click(44, 94)  # Browsing another tab must not stop capture.
                time.sleep(1.5)
                capture('recording-players')
                assert files[0].stat().st_size > before, 'Recording stopped when switching tabs'
                click(44, 405)
                click(280, 367)  # Stop & save.
                time.sleep(.5)
                capture('recording-saved')
                final_size = files[0].stat().st_size
                time.sleep(.5)
                assert files[0].stat().st_size == final_size, 'Recording continued after Stop'
                with wave.open(str(files[0]), 'rb') as wav:
                    assert (wav.getnchannels(), wav.getsampwidth(), wav.getframerate()) == (2, 3, 48000)
                    assert wav.getnframes() > 48000 * 3
                    data = wav.readframes(wav.getnframes())
                    assert len(data) + 44 == final_size, 'Final WAV header does not match saved audio'
                peaks = [max(abs(int.from_bytes(data[i:i+3], 'little', signed=True)) for i in range(channel * 3, len(data), 6)) / 8388608 for channel in [0, 1]]
                assert abs(peaks[0] - .25) < .01 and abs(peaks[1] - .125) < .01, f'Stereo levels changed: {peaks}'
                print(f'Recording verified: {len(data) / 6 / 48000:.2f}s, PCM24 stereo, channel peaks {peaks}', flush=True)
                # Change the persisted format, then record directly from Idle.
                click(680, 167)
                for width, height in [(800, 480), (1024, 600), (1280, 800)]:
                    xlib.XResizeWindow(display, int(window, 16), width, height)
                    xlib.XFlush(display)
                    time.sleep(.5)
                    subprocess.run(['import', '-window', window, str(output_dir / f'recording-format-{width}x{height}.png')], env=env, check=True, timeout=10)
                xlib.XResizeWindow(display, int(window, 16), 800, 480)
                xlib.XFlush(display)
                time.sleep(.5)
                click(400, 192)  # 96 kHz.
                click(250, 330)  # 16-bit.
                chosen = json.loads(settings.read_text())['recording']
                assert chosen['sample_rate'] == 'hz96000', chosen
                assert chosen['bit_depth'] == 'pcm16', chosen
                escape()
                click(280, 367)
                time.sleep(4)
                capture('recording-96k-16bit')
                class ClientMessage(c.Structure):
                    _fields_ = [('type', c.c_int), ('serial', c.c_ulong), ('send_event', c.c_int), ('display', c.c_void_p), ('window', c.c_ulong), ('message_type', c.c_ulong), ('format', c.c_int), ('data', c.c_long * 5)]
                class XEvent(c.Union):
                    _fields_ = [('message', ClientMessage), ('padding', c.c_long * 24)]
                xlib.XInternAtom.argtypes = [c.c_void_p, c.c_char_p, c.c_int]
                xlib.XInternAtom.restype = c.c_ulong
                xlib.XSendEvent.argtypes = [c.c_void_p, c.c_ulong, c.c_int, c.c_long, c.POINTER(XEvent)]
                event = XEvent()
                event.message.type = 33
                event.message.display = display
                event.message.window = int(window, 16)
                event.message.message_type = xlib.XInternAtom(display, b'WM_PROTOCOLS', 0)
                event.message.format = 32
                event.message.data[0] = xlib.XInternAtom(display, b'WM_DELETE_WINDOW', 0)
                xlib.XSendEvent(display, int(window, 16), 0, 0, c.byref(event))
                xlib.XFlush(display)
                time.sleep(.5)
                assert app.poll() is None, 'Window close bypassed the recording guard'
                capture('recording-close-confirmation')
                click(440, 270)  # Stop & quit in the confirmation.
                app.wait(timeout=10)
                completed = sorted(recordings.glob('*.wav'))
                assert len(completed) == 2, 'Second recording did not start'
                with wave.open(str(completed[-1]), 'rb') as wav:
                    assert (wav.getnchannels(), wav.getsampwidth(), wav.getframerate()) == (2, 2, 96000)
                    assert wav.getnframes() > 0
                    assert wav.getnframes() * 4 + 44 == completed[-1].stat().st_size, 'Quit did not finalize the WAV'
                    data = wav.readframes(wav.getnframes())
                peaks = [max(abs(int.from_bytes(data[i:i+2], 'little', signed=True)) for i in range(channel * 2, len(data), 4)) / 32768 for channel in [0, 1]]
                assert abs(peaks[0] - .25) < .01 and abs(peaks[1] - .125) < .01, f'PCM16 stereo levels changed: {peaks}'
                print(f'Stop & quit finalized the second recording: PCM16 stereo at 96 kHz, channel peaks {peaks}', flush=True)
            finally:
                playback.terminate()
                playback.wait(timeout=5)
            return

        if key_matching_only:
            click(760, 26)
            capture('key-matching-settings')
            click(550, 153)
            for width, height in [(800, 480), (1024, 600), (1280, 800)]:
                xlib.XResizeWindow(display, int(window, 16), width, height)
                xlib.XFlush(display)
                time.sleep(.4)
                subprocess.run(['import', '-window', window, str(output_dir / f'key-matching-{width}x{height}.png')], env=env, check=True, timeout=10)
            xlib.XResizeWindow(display, int(window, 16), 800, 480)
            xlib.XFlush(display)
            time.sleep(.4)
            # Toggle relative and previous neighbors; verify the saved choice below.
            click(230, 149)
            click(230, 261)
            chosen = json.loads(settings.read_text()).get('key_matching')
            assert chosen == {'relative': False, 'next': True, 'previous': False}, chosen
            capture('key-matching-custom')
            escape()
            click(44, 174)
            click(330, 26)
            capture('key-matching-library-chooser')
            escape()
            click(44, 94)
            click(480, 26)
            capture('key-matching-player-results')
            return

        if on_deck_only:
            click(44, 174)
            click(300, 142)
            click(720, 430)  # Load the same demo track onto Player 2, leaving it cued.
            click(400, 278)
            for page in ['collection', 'playlist', 'prepare']:
                if page == 'playlist':
                    click(44, 250)
                    click(240, 100)
                elif page == 'prepare':
                    click(44, 322)
                    click(608, 92)
                    click(300, 216)  # Use the existing Warm-up selections playlist.
                for width, height in [(800, 480), (1024, 600), (1280, 800)]:
                    xlib.XResizeWindow(display, int(window, 16), width, height)
                    xlib.XFlush(display)
                    time.sleep(.4)
                    subprocess.run(['import', '-window', window, str(output_dir / f'{page}-on-deck-{width}x{height}.png')], env=env, check=True, timeout=10)
                xlib.XResizeWindow(display, int(window, 16), 800, 480)
                xlib.XFlush(display)
                time.sleep(.4)
            click(44, 174)
            click(200, 26)
            capture('search-on-deck')
            return

        if library_match_only:
            click(44, 174)
            for width, height in [(800, 480), (1024, 600), (1280, 800)]:
                xlib.XResizeWindow(display, int(window, 16), width, height)
                xlib.XFlush(display)
                time.sleep(.4)
                subprocess.run(['import', '-window', window, str(output_dir / f'collection-matches-{width}x{height}.png')], env=env, check=True, timeout=10)
            xlib.XResizeWindow(display, int(window, 16), 800, 480)
            xlib.XFlush(display)
            time.sleep(.4)
            click(334, 26)
            for width, height in [(800, 480), (1024, 600), (1280, 800)]:
                xlib.XResizeWindow(display, int(window, 16), width, height)
                xlib.XFlush(display)
                time.sleep(.4)
                subprocess.run(['import', '-window', window, str(output_dir / f'library-match-picker-{width}x{height}.png')], env=env, check=True, timeout=10)
            escape()
            xlib.XResizeWindow(display, int(window, 16), 800, 480)
            xlib.XFlush(display)
            time.sleep(.4)
            click(44, 250)
            click(240, 100)
            click(300, 142)
            capture('playlist-matches')
            return

        if library_preview_only:
            for playlist in [False, True]:
                xlib.XResizeWindow(display, int(window, 16), 800, 480)
                xlib.XFlush(display)
                time.sleep(.4)
                if playlist:
                    click(44, 250)
                    click(240, 100)
                else:
                    click(44, 174)
                click(300, 142)
                click(506, 430)
                time.sleep(.7)
                streams = subprocess.check_output(['pactl', 'list', 'short', 'sink-inputs'], env=env, text=True, timeout=5)
                assert streams.strip(), 'Preview did not open the private audio output'
                name = 'playlist' if playlist else 'collection'
                for width, height in [(800, 480), (1024, 600), (1280, 800)]:
                    xlib.XResizeWindow(display, int(window, 16), width, height)
                    xlib.XFlush(display)
                    time.sleep(.4)
                    subprocess.run(['import', '-window', window, str(output_dir / f'{name}-preview-{width}x{height}.png')], env=env, check=True, timeout=10)
                click(986, 750)
                subprocess.run(['import', '-window', window, str(output_dir / f'{name}-preview-paused-1280x800.png')], env=env, check=True, timeout=10)
            return

        if prepare_only:
            click(44, 322)
            capture('prepare-unconfigured')
            click(608, 92)
            capture('prepare-picker')
            click(300, 216)  # Use the existing Warm-up selections demo playlist.
            click(300, 150)
            for width, height in [(800, 480), (1024, 600), (1280, 800)]:
                xlib.XResizeWindow(display, int(window, 16), width, height)
                xlib.XFlush(display)
                time.sleep(.4)
                subprocess.run(['import', '-window', window, str(output_dir / f'prepare-list-{width}x{height}.png')], env=env, check=True, timeout=10)
            xlib.XResizeWindow(display, int(window, 16), 800, 480)
            xlib.XFlush(display)
            time.sleep(.4)
            click(440, 430)  # Inspect the selected Prepare track.
            capture('prepare-inspect')
            escape()
            click(44, 174)
            click(300, 142)
            capture('prepare-collection')
            click(44, 94)
            click(480, 26)  # Match a player.
            click(300, 316)
            capture('prepare-match')
            return

        if cue_defaults_only:
            click(760, 26)
            click(380, 153)
            xtest.XTestFakeMotionEvent(display, 0, 30, 26, 0)
            xlib.XFlush(display)
            for width, height in [(800, 480), (1024, 600), (1280, 800)]:
                xlib.XResizeWindow(display, int(window, 16), width, height)
                xlib.XFlush(display)
                time.sleep(.4)
                subprocess.run(['import', '-window', window, str(output_dir / f'cue-defaults-{width}x{height}.png')], env=env, check=True, timeout=10)
            return

        if browser_settings_only:
            click(760, 26)
            click(195, 153)
            click(520, 148)
            for width, height in [(800, 480), (1024, 600), (1280, 800)]:
                xlib.XResizeWindow(display, int(window, 16), width, height)
                xlib.XFlush(display)
                time.sleep(.4)
                for name, x in [('categories', 160), ('sorts', 280)]:
                    click(x, 208)
                    subprocess.run(['import', '-window', window, str(output_dir / f'browser-{name}-{width}x{height}.png')], env=env, check=True, timeout=10)
                click(520, 208)
                subprocess.run(['import', '-window', window, str(output_dir / f'browser-detail-picker-{width}x{height}.png')], env=env, check=True, timeout=10)
                escape()
            xlib.XResizeWindow(display, int(window, 16), 800, 480)
            xlib.XFlush(display)
            time.sleep(.4)
            click(160, 208)
            click(650, 321)  # Move Album above Artist.
            click(550, 267)  # Hide Album in the draft.
            capture('browser-draft')
            click(520, 208)
            capture('browser-detail-picker')
            click(400, 192)  # Artist detail column.
            click(176, 410)
            capture('browser-saved')
            return

        if device_settings_only:
            click(760, 26)
            click(195, 153)
            for width, height in [(800, 480), (1024, 600), (1280, 800)]:
                xlib.XResizeWindow(display, int(window, 16), width, height)
                xlib.XFlush(display)
                time.sleep(.4)
                for name, x in [('cdj', 160), ('display', 280), ('mixer', 400)]:
                    click(x, 148)
                    subprocess.run(['import', '-window', window, str(output_dir / f'device-settings-{name}-{width}x{height}.png')], env=env, check=True, timeout=10)
            xlib.XResizeWindow(display, int(window, 16), 800, 480)
            xlib.XFlush(display)
            time.sleep(.4)
            click(650, 205)
            capture('device-setting-choice')
            click(400, 214)  # Off stays in the draft until Save.
            capture('device-settings-draft')
            click(160, 410)
            capture('device-settings-saved')
            click(160, 148)  # CDJ: Auto cue level has a longer option list.
            click(650, 270)
            capture('device-setting-long-list')
            return

        if grid_editor_only:
            click(300, 141)
            click(610, 412)
            click(720, 412)
            click(720, 320)
            for width, height in [(800, 480), (1024, 600), (1280, 800)]:
                xlib.XResizeWindow(display, int(window, 16), width, height)
                xlib.XFlush(display)
                time.sleep(.4)
                subprocess.run(['import', '-window', window, str(output_dir / f'grid-editor-{width}x{height}.png')], env=env, check=True, timeout=10)
                click(width - 120, 96 if height == 480 else 108)
                subprocess.run(['import', '-window', window, str(output_dir / f'grid-locked-{width}x{height}.png')], env=env, check=True, timeout=10)
                click(width - 120, 96 if height == 480 else 108)
            xlib.XResizeWindow(display, int(window, 16), 800, 480)
            xlib.XFlush(display)
            time.sleep(.4)
            click(486, 26)
            capture('grid-undone')
            click(582, 26)
            capture('grid-redone')
            click(225, 378)
            capture('grid-bpm-keypad')
            return

        if audio_settings_only:
            click(300, 141)
            click(610, 412)
            capture('inspect-actions')
            click(760, 26)
            xtest.XTestFakeMotionEvent(display, 0, 600, 350, 0)
            for _ in range(16):
                xtest.XTestFakeButtonEvent(display, 5, 1, 0)
                xtest.XTestFakeButtonEvent(display, 5, 0, 0)
            xlib.XFlush(display)
            time.sleep(.5)
            capture('settings-audio')
            click(170, 354)
            time.sleep(.5)
            capture('settings-audio-output')
            return

        if playlist_icons_only:
            time.sleep(.4)
            click(40, 260)
            for width, height in [(800, 480), (1280, 800)]:
                xlib.XResizeWindow(display, int(window, 16), width, height)
                xlib.XFlush(display)
                time.sleep(.4)
                subprocess.run(['import', '-window', window, str(output_dir / f'playlist-icons-{width}x{height}.png')], env=env, check=True, timeout=10)
            xlib.XResizeWindow(display, int(window, 16), 800, 480)
            xlib.XFlush(display)
            time.sleep(.3)
            click(300, 250)
            capture('playlist-icons-folder')
            return

        if tag_manager_only:
            time.sleep(.4)
            click(300, 141)
            click(610, 412)
            click(600, 94)
            click(290, 140)
            click(100, 424)  # Manage tags from the track editor.
            for width, height in [(800, 480), (1024, 600), (1280, 800)]:
                xlib.XResizeWindow(display, int(window, 16), width, height)
                xlib.XFlush(display)
                time.sleep(.3)
                subprocess.run(['import', '-window', window, str(output_dir / f'tag-manager-{width}x{height}.png')], env=env, check=True, timeout=10)
            xlib.XResizeWindow(display, int(window, 16), 800, 480)
            xlib.XFlush(display)
            time.sleep(.3)
            click(80, 236)  # Energy category.
            capture('tag-manager-category')
            click(700, 424)  # New tag inherits Energy.
            capture('tag-manager-new')
            click(560, 424)  # Cancel, no writes to the fixture.
            click(210, 424)  # Categories.
            capture('tag-manager-categories')
            click(120, 184)  # Energy category editor.
            capture('tag-manager-edit-category')
            click(530, 424)  # Cancel.
            click(80, 424)  # Back to tags.
            click(340, 184)  # Edit Building.
            capture('tag-manager-edit')
            click(120, 424)  # Show deletion confirmation without submitting it.
            capture('tag-manager-delete')
            return

        if tag_picker_only:
            time.sleep(.4)
            click(300, 141)
            click(610, 412)
            click(600, 94)
            click(290, 140)
            capture('tag-picker')
            for width, height in [(1024, 600), (1280, 800)]:
                xlib.XResizeWindow(display, int(window, 16), width, height)
                xlib.XFlush(display)
                time.sleep(.3)
                subprocess.run(['import', '-window', window, str(output_dir / f'tag-picker-{width}x{height}.png')], env=env, check=True, timeout=10)
            xlib.XResizeWindow(display, int(window, 16), 800, 480)
            xlib.XFlush(display)
            time.sleep(.3)
            click(80, 345)  # Energy category.
            capture('tag-picker-category')
            click(80, 290)  # Review just the assigned tags.
            click(340, 240)  # Remove one from the draft, without saving.
            capture('tag-picker-selected')
            click(80, 235)
            click(360, 182)
            click(320, 82)
            for char in 'peak':
                code = xlib.XKeysymToKeycode(display, ord(char))
                for pressed in [True, False]:
                    xtest.XTestFakeKeyEvent(display, code, pressed, 0)
                    xlib.XFlush(display)
                    time.sleep(.04)
            escape()
            capture('tag-picker-search')
            return

        if player_phrases_only:
            def player_sizes(name):
                for width, height in [(800, 480), (1024, 600), (1280, 800)]:
                    xlib.XResizeWindow(display, int(window, 16), width, height)
                    xlib.XFlush(display)
                    time.sleep(.3)
                    subprocess.run(['import', '-window', window, str(output_dir / f'player-phrases-{name}-{width}x{height}.png')], env=env, check=True, timeout=10)
                xlib.XResizeWindow(display, int(window, 16), 800, 480)
                xlib.XFlush(display)
                time.sleep(.3)

            click(300, 191)  # Select a different track for the stopped second deck.
            click(45, 95)
            click(660, 404)
            player_sizes('cards')
            click(570, 92)
            player_sizes('waveforms')
            click(450, 180)
            player_sizes('focus')
            return

        if waveform_curves_only:
            click(300, 141)
            click(610, 412)
            click(228, 305)  # Cue B, away from the start of the track.
            for _ in range(5):
                click(758, 359)  # 8x -> 256x.
            for width, height in [(800, 480), (1280, 800)]:
                xlib.XResizeWindow(display, int(window, 16), width, height)
                xlib.XFlush(display)
                time.sleep(.3)
                subprocess.run(['import', '-window', window, str(output_dir / f'waveform-curves-{style}-{width}x{height}.png')], env=env, check=True, timeout=10)
            xlib.XResizeWindow(display, int(window, 16), 800, 480)
            xlib.XFlush(display)
            time.sleep(.3)
            click(45, 95)
            click(230, 155)
            for _ in range(3):
                click(608, 414)  # Focused player: zoom to a four-second window.
            for width, height in [(800, 480), (1280, 800)]:
                xlib.XResizeWindow(display, int(window, 16), width, height)
                xlib.XFlush(display)
                time.sleep(.3)
                subprocess.run(['import', '-window', window, str(output_dir / f'waveform-curves-player-{style}-{width}x{height}.png')], env=env, check=True, timeout=10)
            return

        if inspect_grid_only:
            click(300, 141)
            click(610, 412)
            for zoom in ['8x', '32x', 'fit']:
                if zoom == '32x':
                    click(758, 359)
                    click(758, 359)
                elif zoom == 'fit':
                    click(657, 359)
                capture(f'inspect-grid-{zoom}')
                xlib.XResizeWindow(display, int(window, 16), 1280, 800)
                xlib.XFlush(display)
                time.sleep(.3)
                subprocess.run(['import', '-window', window, str(output_dir / f'inspect-grid-{zoom}-1280x800.png')], env=env, check=True, timeout=10)
                xlib.XResizeWindow(display, int(window, 16), 800, 480)
                xlib.XFlush(display)
                time.sleep(.3)
            return

        if cue_editor_only:
            click(300, 141)
            click(610, 412)
            click(228, 305, hold=.9)
            xtest.XTestFakeMotionEvent(display, 0, 30, 26, 0)
            xlib.XFlush(display)
            for width, height in [(800, 480), (1024, 600), (1280, 800)]:
                xlib.XResizeWindow(display, int(window, 16), width, height)
                xlib.XFlush(display)
                time.sleep(.4)
                subprocess.run(['import', '-window', window, str(output_dir / f'cue-editor-{width}x{height}.png')], env=env, check=True, timeout=10)
            return

        if cue_markers_only:
            if style == 'auto':
                capture('cue-preview')
            click(300, 141)
            click(610, 412)
            if style == 'auto':
                capture('cue-inspect')
            click(657, 359)  # Fit: show the full track in the scrolling waveform.
            click(575, 359)  # Hide grid lines to inspect the antialiased contour.
            suffix = '' if style == 'auto' else '-' + style
            capture('cue-inspect-fit' + suffix)
            xlib.XResizeWindow(display, int(window, 16), 1280, 800)
            xlib.XFlush(display)
            time.sleep(.3)
            subprocess.run(['import', '-window', window, str(output_dir / f'cue-inspect-fit{suffix}-1280x800.png')], env=env, check=True, timeout=10)
            if style != 'auto':
                return
            xlib.XResizeWindow(display, int(window, 16), 800, 480)
            xlib.XFlush(display)
            time.sleep(.3)
            click(45, 95)
            capture('cue-players-waveforms')
            click(570, 92)
            capture('cue-player-cards')
            click(230, 155)
            capture('cue-player-focus')
            return

        if search_only:
            click(220, 26)
            for char in 'rain':
                code = xlib.XKeysymToKeycode(display, ord(char))
                for pressed in [True, False]:
                    xtest.XTestFakeKeyEvent(display, code, pressed, 0)
                    xlib.XFlush(display)
                    time.sleep(.05)
            time.sleep(.2)
            capture('keyboard')
            for width, height in [(1024, 600), (1280, 800)]:
                xlib.XResizeWindow(display, int(window, 16), width, height)
                xlib.XFlush(display)
                time.sleep(.3)
                subprocess.run(['import', '-window', window, str(output_dir / f'search-results-{width}x{height}.png')], env=env, check=True, timeout=10)
            xlib.XResizeWindow(display, int(window, 16), 800, 480)
            xlib.XFlush(display)
            time.sleep(.3)
            click(203, 350)  # On-screen z: no matching demo titles/artists.
            capture('search-empty')
            click(730, 246)  # Backspace restores the results.
            click(330, 112)  # Selecting a result dismisses the keyboard.
            capture('search-selected')
            return

        if beat_jump_only:
            click(300, 141)
            click(610, 412)
            click(228, 305)
            click(422, 359)
            capture('beat-jump')
            click(330, 359)
            capture('beat-jump-menu')
            escape()
            click(480, 368)
            capture('inspect-quantize')
            for width, height in [(1024, 600), (1280, 800)]:
                xlib.XResizeWindow(display, int(window, 16), width, height)
                xlib.XFlush(display)
                time.sleep(.3)
                subprocess.run(['import', '-window', window, str(output_dir / f'beat-jump-{width}x{height}.png')], env=env, check=True, timeout=10)
            return

        if load_only:
            click(300, 191)
            click(720, 412)
            capture('load-picker')
            click(400, 215)
            capture('load-confirmation')
            xlib.XResizeWindow(display, int(window, 16), 1024, 600)
            xlib.XFlush(display)
            time.sleep(.3)
            subprocess.run(['import', '-window', window, str(output_dir / 'load-confirmation-1024x600.png')], env=env, check=True, timeout=10)
            return

        if library_only:
            click(300, 141)
            capture('selected')
            click(480, 92)
            capture('collection-artist-sort')
            click(570, 26)
            capture('sort-menu')
            escape()
            click(450, 26)
            capture('filters-bpm')
            escape()
            click(45, 260)
            capture('playlists')
            click(240, 100)
            capture('playlist-tracks')
            click(220, 26)
            capture('keyboard')
            escape()
            click(760, 26)
            capture('setup')
            click(572, 240)
            capture('diagnostics-status')
            copy_diagnostics()
            click(368, 92)
            capture('diag')
            click(540, 92)
            capture('diag-paused')
            return

        if inspect_waveform:
            click(260, 141)
            click(610, 412)
            shown = 'half' if style in ['3band', 'auto'] else overview
            capture(f'inspect-{style}-{shown}')
            click(45, 95)
            capture(f'players-waveforms-{style}-{shown}')
            click(570, 92)
            capture(f'players-cards-{style}-{shown}')
            click(230, 155)
            capture(f'player-focus-{style}-{shown}')
            click(760, 26)
            xtest.XTestFakeMotionEvent(display, 0, 600, 350, 0)
            for _ in range(10):
                xtest.XTestFakeButtonEvent(display, 5, 1, 0)
                xtest.XTestFakeButtonEvent(display, 5, 0, 0)
            xlib.XFlush(display)
            time.sleep(.5)
            capture(f'inspect-settings-{style}-{shown}')
            return

        click(570, 26)
        capture('sort-menu')
        escape()
        click(450, 26)
        capture('filters-bpm')
        escape()
        click(260, 141)
        capture('selected')
        click(610, 412)
        capture('detail')
        click(440, 182)
        click(758, 359)
        capture('detail-zoomed')
        click(486, 305)
        capture('hot-cues')
        click(228, 305, hold=.9)
        capture('cue-editor')
        click(125, 375)
        click(228, 305)
        click(605, 412)
        click(308, 359)
        capture('loop-preview')
        click(681, 359)
        capture('loop-editor')
        click(125, 375)
        click(145, 94)
        click(45, 95)
        capture('players')
        for width, height in [(800, 1000), (1280, 800)]:
            xlib.XResizeWindow(display, int(window, 16), width, height)
            xlib.XFlush(display)
            time.sleep(.3)
            subprocess.run(['import', '-window', window, str(output_dir / f'players-{width}x{height}.png')], env=env, check=True, timeout=10)
        xlib.XResizeWindow(display, int(window, 16), 800, 480)
        xlib.XFlush(display)
        time.sleep(.3)
        click(230, 155)
        capture('player-focus')
        click(608, 414)
        capture('player-focus-zoomed')
        click(304, 92)
        capture('player-focus-cued')
        click(416, 92)
        capture('player-focus-disconnected')
        click(155, 92)
        click(660, 404)
        click(570, 92)
        capture('players-compare')
        click(482, 92)
        capture('players-compare-zoomed')
        for width, height in [(1024, 600), (1280, 800)]:
            xlib.XResizeWindow(display, int(window, 16), width, height)
            xlib.XFlush(display)
            time.sleep(.3)
            subprocess.run(['import', '-window', window, str(output_dir / f'players-waveforms-{width}x{height}.png')], env=env, check=True, timeout=10)
        xlib.XResizeWindow(display, int(window, 16), 800, 480)
        xlib.XFlush(display)
        time.sleep(.3)
        click(500, 250)
        capture('players-compare-expanded')
        click(155, 92)
        click(708, 92)
        capture('players-pair')
        click(476, 366)
        click(45, 260)
        capture('playlists')
        click(760, 26)
        capture('setup')
        click(572, 240)
        capture('diagnostics-status')
        copy_diagnostics()
        click(368, 92)
        capture('diag')
        click(540, 92)
        capture('diag-paused')
        click(160, 92)
        xtest.XTestFakeMotionEvent(display, 0, 600, 350, 0)
        for _ in range(5):
            xtest.XTestFakeButtonEvent(display, 5, 1, 0)
            xtest.XTestFakeButtonEvent(display, 5, 0, 0)
        xlib.XFlush(display)
        time.sleep(.5)
        capture('waveform-settings')
        click(45, 180)
        click(220, 26)
        capture('keyboard')
    finally:
        app.terminate()
        app.wait(timeout=5)
        xlib.XCloseDisplay(display)


if __name__ == '__main__':
    main()
