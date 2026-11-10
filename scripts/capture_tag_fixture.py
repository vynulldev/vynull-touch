#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Read-only, fictional tag catalog for native UI captures; never uses a real library."""
from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from threading import Thread
from urllib.parse import urlsplit


@contextmanager
def tag_fixture(*, playlist_icons=False):
    groups = {
        'Energy': ['Building', 'Driving', 'High energy', 'Laid back', 'Peak time', 'Steady'],
        'Mood': ['Atmospheric', 'Dark', 'Euphoric', 'Hypnotic', 'Melancholic', 'Playful', 'Soulful', 'Uplifting'],
        'Set role': ['After hours', 'Closing track', 'Dance floor', 'Opening track', 'Transition', 'Warm up'],
        'Sound': ['Acid', 'Deep bass', 'Driving', 'Instrumental', 'Percussive', 'Piano', 'Vocal'],
    }
    categories = [{'id': i, 'name': name} for i, name in enumerate(groups, 1)]
    tags = []
    for category, names in enumerate(groups.values(), 1):
        for name in names:
            tags.append({'id': len(tags) + 1, 'name': name, 'category_id': category})
    tags.append({'id': len(tags) + 1, 'name': 'Try next set', 'category_id': 0})
    selected = [tag for tag in tags if tag['name'] in ['Driving', 'Hypnotic', 'Vocal']]
    track = {'id': 1, 'title': 'After the Rain', 'artist': 'Mira Vale', 'bpm': 120,
             'key': '1B', 'duration': 284, 'file_path': '/fictional/after-the-rain.flac',
             'file_type': 'FLAC', 'tags': [tag['name'] for tag in selected]}
    revision = {'library_session': 'tag-capture-fixture', 'rev': 1}
    data = {
        '/api/tracks/rev': revision, '/api/cues/rev': revision,
        '/api/tracks': [track], '/api/tracks/1': track, '/api/playlists': [],
        '/api/status': {'peers': [], 'players': []}, '/api/link': {'linked': False},
        '/api/tags': tags, '/api/tag-categories': categories, '/api/tracks/1/tags': selected,
        '/api/tracks/1/cues': [], '/api/cues': {}, '/api/tracks/1/phrases': [],
        '/api/analysis/1': {'track_id': 1, 'beats': []},
        '/api/analysis/waveform/1': {'track_id': 1, 'sample_rate': 1, 'entries': [{'r': 80, 'g': 130, 'b': 220, 'h': 20}] * 284},
    }
    if playlist_icons:
        data['/api/playlists'] = [
            {'id': 1, 'name': 'Warm-up selections', 'track_ids': [1]},
            {'id': 2, 'name': 'High energy', 'is_smart': True},
            {'id': 3, 'name': 'Tonight', 'is_folder': True},
            {'id': 4, 'parent_id': 3, 'name': 'Opening set', 'track_ids': [1]},
            {'id': 5, 'parent_id': 3, 'name': 'Deep cuts', 'is_smart': True},
        ]
        for playlist_id in [2, 5]:
            data[f'/api/playlists/{playlist_id}/tracks'] = [track]

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_GET(self):
            path = urlsplit(self.path).path
            body = json.dumps(data.get(path, {})).encode()
            self.send_response(200 if path in data and 'style=3band' not in self.path else 404)
            self.send_header('Content-Type', 'application/json')
            self.send_header('X-Vynull-Library-Session', 'tag-capture-fixture')
            self.send_header('Content-Length', str(len(body)))
            self.end_headers()
            self.wfile.write(body)

    server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    thread = Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        yield f'http://127.0.0.1:{server.server_port}'
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=2)
